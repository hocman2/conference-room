use mediasoup::prelude::*;
use mediasoup::worker::CreateWebRtcServerError;
use event_listener_primitives::{BagOnce, HandlerId};
use std::sync::Arc;
use std::env;
use std::net::{IpAddr, Ipv4Addr};

pub const ANNOUNCED_ADDRESS_ENV_KEY: &str = "PUBLIC_IP";

pub(super) struct WorkerData {
	pub(super) worker: Worker,
	/// A bunch of handlers attached here so they are not lost in the wild
	pub(super) attached_handlers: Vec<HandlerId>,
	/// A webrtc server used for all new participants in this worker
	pub(super) webrtc_server: WebRtcServer,
	/// The number of consumers allows us to know if the worker is full or not
	pub(super) consumer_count: u32,
	/// When the number of router reaches 0 we can release the worker
	pub(super) router_count: u32,
	/// Event called only when a worker dies or closes in an unexpected fashion
	// This doesn't need to be wrapped into an Arc in theory, but it is just to enforce we are sharing the same
	// bag between this and data returned in create_router()
	pub(super) worker_died_unexpectedly: Arc<BagOnce<Box<dyn FnOnce() + Send + Sync + 'static>>>
}

impl WorkerData {
	pub(super) async fn new(worker: Worker) -> Result<Self, String> {
		let webrtc_server = match WorkerData::create_webrtc_server(&worker).await {
			Ok(server) => server,
			Err(e) => {return Err(format!("Failed to create webrtc server: {e}").into());}
		};

		Ok(WorkerData {
			worker,
			webrtc_server,
			attached_handlers: Vec::new(),
			consumer_count: 0,
			router_count: 0,
			worker_died_unexpectedly: Arc::new(BagOnce::default()),
		})
	}

	pub(super) fn get_num_consumers(&self) -> u32 {
		unimplemented!()
	}

	pub(super) async fn create_webrtc_server(worker: &Worker) -> Result<WebRtcServer, CreateWebRtcServerError> {
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
