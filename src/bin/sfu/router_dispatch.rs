use std::{num::{NonZeroU32, NonZeroU8}, sync::Arc};
use event_listener_primitives::HandlerId;
use mediasoup::{prelude::*, worker::{CreateRouterError, WorkerId, WorkerLogTag}};
use parking_lot::Mutex;

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
	consumer_count: u32,
	attached_handlers: Vec<HandlerId>,
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
	fn new(worker: Worker) -> Self {
		WorkerData {
			worker,
			consumer_count: 0,
			attached_handlers: Vec::new(),
		}
	}

	fn get_num_consumers(&self) -> u32 {
		unimplemented!()
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
	pub async fn create_router(&self) -> Result<Router, CreateRouterError> {
		let worker = self.get_or_create_appropriate_worker().await;
		let router = worker.create_router(RouterOptions::new(supported_codecs())).await?;

		Ok(self.count_consumers_on_router(router))
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
					self.workers.lock().push(WorkerData::new(worker.clone()));
					let handler = worker.on_dead({
						let workers = self.workers.clone();
						let worker_id = worker.id();
						move |r| {
							log::warn!("Worker died for reason {}", r.err().unwrap());
							RouterDispatch::on_worker_dead(workers, worker_id);
						}
					});
					push_handler(&self.workers, &worker.id(), handler);
					return worker;
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

	fn on_worker_dead(workers: Arc<Mutex<Vec<WorkerData>>>, worker_id: WorkerId) {
		let mut workers = workers.lock();
		match workers
			.iter()
			.position(|w| w.worker.id() == worker_id) {
				Some(index) => {
					let worker_last_breath = workers.remove(index);
					if worker_last_breath.consumer_count > 0 {
						log::warn!("A Worker was terminated early but had participants, this should not happen. Their connections will be closed");
					}
					//todo!("Do some participants closing if needed idk")
				},
				None => log::error!("A worker is dead but couldn't be found in the workers list")
		};
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
	}
}

fn decrease_consumer_count(workers: &Arc<Mutex<Vec<WorkerData>>>, worker_id: &WorkerId) {
	let mut workers = workers.lock();
	let worker_maybe = workers.iter_mut().find(|w| w.worker.id() == *worker_id);
	if let Some(worker_data) = worker_maybe {
		if worker_data.consumer_count > 0 {
			worker_data.consumer_count -= 1;
		}
	}
}
