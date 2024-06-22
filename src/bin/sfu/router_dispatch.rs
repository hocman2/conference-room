use std::{env, net::{IpAddr, Ipv4Addr}, num::{NonZeroU32, NonZeroU8}, sync::Arc};
use event_listener_primitives::HandlerId;
use mediasoup::{prelude::*, worker::{CreateRouterError, CreateWebRtcServerError, WorkerId, WorkerLogTag}};
use parking_lot::Mutex;

pub const ANNOUNCED_ADDRESS_ENV_KEY: &str = "PUBLIC_IP";

fn supported_codecs() -> Vec<RtpCodecCapability> {
	vec![
	   RtpCodecCapability::Audio {
	            mime_type: MimeTypeAudio::Opus,
	            preferred_payload_type: None,
	            clock_rate: NonZeroU32::new(48000).unwrap(),
	            channels: NonZeroU8::new(2).unwrap(),
	            parameters: RtpCodecParametersParameters::from([("useinbandfec", 1_u32.into())]),
	            rtcp_feedback: vec![RtcpFeedback::TransportCc],
		},
		RtpCodecCapability::Video {
			mime_type: MimeTypeVideo::Vp8,
			preferred_payload_type: None,
			clock_rate: NonZeroU32::new(90000).unwrap(),
			parameters: RtpCodecParametersParameters::default(),
			rtcp_feedback: vec![
                RtcpFeedback::Nack,
                RtcpFeedback::NackPli,
                RtcpFeedback::CcmFir,
                RtcpFeedback::GoogRemb,
                RtcpFeedback::TransportCc,
            ]
		},
	]
}

pub struct RouterDispatchConfig {
	pub max_workers: usize,
	pub consumers_per_worker: u32,
}

struct WorkerData {
	worker: Worker,
	/// A bunch of handlers attached here so they are not lost in the wild
	attached_handlers: Vec<HandlerId>,
	/// A webrtc server used for all new participants in this worker
	webrtc_server: WebRtcServer,
	/// The number of consumers allows us to know if the worker is full or not
	consumer_count: u32,
	/// When the number of router reaches 0 we can release the worker
	router_count: u32,
}

#[derive(Clone)]
pub struct RouterDispatch {
	// Todo make this a inner struct for faster cloning
	worker_manager: WorkerManager,
	workers: Arc<Mutex<Vec<WorkerData>>>,
	max_workers: usize,
	consumers_per_worker: u32,
}

impl Default for RouterDispatchConfig {
	fn default() -> Self {
		RouterDispatchConfig {
			max_workers: 1,
			consumers_per_worker: 500,
		}
	}
}

impl Default for RouterDispatch {
	fn default() -> Self {
		RouterDispatch::new(RouterDispatchConfig::default())
	}
}

impl WorkerData {
	async fn new(worker: Worker) -> Result<Self, String> {
		let webrtc_server = match WorkerData::create_webrtc_server(&worker).await {
			Ok(server) => server,
			Err(e) => {return Err("Failed to create webrtc server: {e}".into());}
		};

		Ok(WorkerData {
			worker,
			webrtc_server,
			attached_handlers: Vec::new(),
			consumer_count: 0,
			router_count: 0,
		})
	}

	fn get_num_consumers(&self) -> u32 {
		unimplemented!()
	}

	async fn create_webrtc_server(worker: &Worker) -> Result<WebRtcServer, CreateWebRtcServerError> {
		let (listen_ip, announced_address) = {
			// Listen to all incoming connections, announce the server public address in ICE candidate
			if let Ok(public_ip) = env::var(ANNOUNCED_ADDRESS_ENV_KEY) {
				(Ipv4Addr::UNSPECIFIED, Some(public_ip))
			} else {
				// Run in local mode, listen only to localhost connections
				(Ipv4Addr::LOCALHOST, None)
			}
		};

		let preferred_listen_info = ListenInfo {
			protocol: Protocol::Udp,
			ip: IpAddr::V4(listen_ip),
			announced_address,
			port: None,
			port_range: None,
			flags: None,
			send_buffer_size: None,
			recv_buffer_size: None
		};

		worker.create_webrtc_server(WebRtcServerOptions::new(
			WebRtcServerListenInfos::new(
				preferred_listen_info
			)
		)).await
	}
}

impl RouterDispatch {
	pub fn new(config: RouterDispatchConfig) -> Self {
		RouterDispatch {
			worker_manager: WorkerManager::new(),
			workers: Arc::new(Mutex::new(Vec::with_capacity(config.max_workers))),
			max_workers: config.max_workers,
			consumers_per_worker: config.consumers_per_worker,
		}
	}

	/// Creates a new router for use in a room. That router can be dropped if uneeded.
	/// Note that this might cause the associated worker to die as well
	pub async fn create_router(&self) -> Result<(Router, WebRtcServer), CreateRouterError> {
		let worker = self.get_or_create_appropriate_worker().await;
		let router = worker.create_router(RouterOptions::new(supported_codecs())).await?;

		let handler = router.on_close({
			let worker_id = worker.id();
			let other_self = self.clone();
			move || {
				other_self.on_router_closed(worker_id);
			}
		});
		push_handler(&self.workers, &worker.id(), handler);

		let webrtc_server = {
			let workers = self.workers.lock();
			let worker_data = workers.iter().find(|w| w.worker.id() == worker.id()).unwrap();
			worker_data.webrtc_server.clone()
		};

		Ok((self.count_consumers_on_router(router), webrtc_server))
	}

	fn count_consumers_on_router(&self, router: Router) -> Router {
		let worker = router.worker();
		// ⚠️this is an ugly function
		let new_transport_handler = router.on_new_transport({
			let workers_ref = self.workers.clone();
			let associated_worker_id = worker.id();
			move |new_transport| {
				let new_consumer_handler = new_transport.on_new_consumer(Arc::new({
					let workers_ref = workers_ref.clone();
					let associated_worker_id = associated_worker_id.clone();
					move |consumer| {
						increase_consumer_count(&workers_ref, &associated_worker_id);

						let consumer_close_handler = consumer.on_close({
							let workers_ref = workers_ref.clone();
							let associated_worker_id = associated_worker_id.clone();
							move || {
								decrease_consumer_count(&workers_ref, &associated_worker_id);
						}});
						push_handler(&workers_ref, &associated_worker_id, consumer_close_handler);
					}
				}));
				push_handler(&workers_ref, &associated_worker_id, new_consumer_handler);
			}
		});
		push_handler(&self.workers, &worker.id(), new_transport_handler);
		router
	}

	/// Gets a worker ready to accept new routers or creates one if conditions permit it
	/// This function can panic if no worker is stored and no worker can be created
	async fn get_or_create_appropriate_worker(&self) -> Worker {
		// Create new workers while the vec is not filled
		if self.workers.lock().len() < self.max_workers {
			let worker_maybe = self.worker_manager.create_worker({
				let mut settings = WorkerSettings::default();
				settings.log_tags = vec![
					WorkerLogTag::Info,
					WorkerLogTag::Ice,
					WorkerLogTag::Dtls,
					WorkerLogTag::Rtp,
					WorkerLogTag::Rtcp,
					WorkerLogTag::Srtp,
					WorkerLogTag::Rtx,
					WorkerLogTag::Bwe,
					WorkerLogTag::Score,
					WorkerLogTag::Simulcast,
					WorkerLogTag::Svc,
					WorkerLogTag::Sctp,
					WorkerLogTag::Message
				];
				settings
			}).await;

			match worker_maybe {
				Ok(worker) => {
					log::info!("Created new worker {}", worker.id());

					// Create the structure that'll hold the worker alive
					match WorkerData::new(worker.clone()).await {
						Ok(worker_data) => {
							self.workers.lock().push(worker_data);
							log::debug!("There are {}/{} workers currently", self.workers.lock().len(), self.max_workers);

							let handler = worker.on_dead({
								let other_self = self.clone();
								let worker_id = worker.id();
								move |r| {
									log::warn!("Worker died for reason {}", r.err().unwrap());
									other_self.on_worker_dead(worker_id);
								}
							});
							push_handler(&self.workers, &worker.id(), handler);

							return worker;
						},
						Err(e) => {
							log::error!("Failed to create Worker data: {e}");
						}
					}
				},
				Err(e) => {
					log::error!("Failed to create Worker: {e}");
				}
			};
		}

		match self.workers.lock().iter().min_by(|w1, w2| u32::cmp(&w1.get_num_consumers(), &w2.get_num_consumers())) {
			None => panic!("Fatal error: Could not create any worker."),
			Some(worker_data) => worker_data.worker.clone()
		}
	}

	/// Release a worker if it's in a dead state
	fn on_worker_dead(&self, worker_id: WorkerId) {
		let mut workers = self.workers.lock();
		match workers
			.iter()
			.position(|w| w.worker.id() == worker_id) {
				Some(index) => {
					let worker_last_breath = workers.remove(index);
					if worker_last_breath.consumer_count > 0 {
						log::warn!("A Worker was terminated early but had participants, this should not happen. Their connections will be closed");
					}
				},
				None => log::error!("A worker is dead but couldn't be found in the workers list")
		};
	}

	/// Count down the number of active routers, if there is 0, release the worker
	fn on_router_closed(&self, worker_id: WorkerId) {
		let mut workers = self.workers.lock();

		for i in 0..workers.len() {
			let worker_data = unsafe {workers.get_unchecked_mut(i)};
			if worker_data.worker.id() != worker_id { continue; }

			// Simply decrease the counter
			if worker_data.router_count > 1 {
				worker_data.router_count -= 1;
				return;
			}

			// Delete the worker
			workers.remove(i);
			return;
		}
	}
}

// Some utility functions for when consumers are added or removed
fn push_handler(workers: &Arc<Mutex<Vec<WorkerData>>>, worker_id: &WorkerId, handler_id: HandlerId) {
	let mut workers = workers.lock();
	let worker_maybe = workers.iter_mut().find(|w| w.worker.id() == *worker_id);
	if let Some(worker_data) = worker_maybe {
		worker_data.attached_handlers.push(handler_id);
	}
}

fn increase_consumer_count(workers: &Arc<Mutex<Vec<WorkerData>>>, worker_id: &WorkerId) {
	let mut workers = workers.lock();
	let worker_maybe = workers.iter_mut().find(|w| w.worker.id() == *worker_id);
	if let Some(worker_data) = worker_maybe {
		worker_data.consumer_count += 1;
		log::info!("There are now {} consumers on Worker {}", worker_data.consumer_count, worker_data.worker.id());
	}
}

fn decrease_consumer_count(workers: &Arc<Mutex<Vec<WorkerData>>>, worker_id: &WorkerId) {
	let mut workers = workers.lock();
	let worker_maybe = workers.iter_mut().find(|w| w.worker.id() == *worker_id);
	if let Some(worker_data) = worker_maybe {
		if worker_data.consumer_count > 0 {
			worker_data.consumer_count -= 1;
			log::debug!("There are now {} consumers on Worker {}", worker_data.consumer_count, worker_data.worker.id());
		}
	}
}
