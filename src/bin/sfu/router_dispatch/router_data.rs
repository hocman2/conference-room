use mediasoup::prelude::*;
use event_listener_primitives::{BagOnce, HandlerId};
use std::sync::Arc;

pub struct RouterData {
	pub router: Router,
	pub webrtc_server: WebRtcServer,
	pub(super) worker_died_unexpectedly: Arc<BagOnce<Box<dyn FnOnce() + Send + Sync + 'static>>>
}

impl RouterData {
	pub fn on_worker_died_unexpectedly<F>(&self, callback: F) -> HandlerId
	where F: FnOnce() + Send + Sync + 'static {
		self.worker_died_unexpectedly.add(Box::new(callback))
	}
}
