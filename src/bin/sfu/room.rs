use confroom_server::monitoring::SFUEvent;
use confroom_server::uuids::{RoomId, ParticipantId};
use mediasoup::prelude::*;
use event_listener_primitives::{Bag, BagOnce, HandlerId};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use crate::monitor_dispatch::MonitorDispatch;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Default)]
struct Handlers {
	producer_add: Bag<Arc<dyn Fn(&ParticipantId, &Producer) + Send + Sync + 'static>, ParticipantId, Producer>,
	producer_remove: Bag<Arc<dyn Fn(&ParticipantId, &ProducerId) + Send + Sync + 'static>, ParticipantId, ProducerId>,
	close: BagOnce<Box<dyn FnOnce() + Send + 'static>>
}

// Room internal
pub struct Inner {
	id: RoomId,
	router: Router,
	clients: Mutex<HashMap<ParticipantId, Vec<Producer>>>,
	handlers: Handlers,
}
impl std::fmt::Display for Inner {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Inner")
    		.field("id", &self.id)
      		.field("router", &self.router)
        	.field("clients", &self.clients)
         	.field("handlers", &"...")
          	.finish()
	}
}
impl Drop for Inner {
	fn drop(&mut self) {
		println!("Room {} closed", self.id);
		self.handlers.close.call_simple();
	}
}

/// A Room can hold multiple participants and send events when new participants enter or leave
/// A room is cheap to clone and can be passed to different threads safely as it's data is heap allocated
#[derive(Clone)]
pub struct Room {
	inner: Arc<Inner>
}
impl Room {
	pub async fn new(router: Router) -> Result<Self, Error> {
		Self::new_with_id(router, RoomId::new()).await
	}

	pub async fn new_with_id(router: Router, id: RoomId) -> Result<Self, Error> {
		let _ = MonitorDispatch::send_event(SFUEvent::RoomOpened { id: id.clone() });
		println!("Room {id} opened");

		Ok(Room {
			inner: Arc::new(Inner {
				id,
				router,
				clients: Mutex::new(HashMap::new()),
				handlers: Handlers::default()
		 })
		})
	}

	pub fn downgrade(&self) -> WeakRoom {
		WeakRoom { inner: Arc::downgrade(&self.inner) }
	}

	pub fn id(&self) -> RoomId { self.inner.id }
	pub fn router(&self) -> &Router { &self.inner.router }

	pub fn add_producer(&self, participant_id: ParticipantId, producer: Producer) {
		self.inner
    		.clients
      		.lock()
        	.entry(participant_id)
         	.or_default()
          	.push(producer.clone());

		self.inner.handlers.producer_add.call_simple(&participant_id, &producer);
	}

	pub fn remove_participant(&self, participant_id: &ParticipantId) {
		let producers = self.inner.clients.lock().remove(participant_id);

		for producer in producers.unwrap_or_default() {
			let producer_id = &producer.id();
			self.inner.handlers.producer_remove.call_simple(participant_id, producer_id);
		}
	}

	pub fn get_all_producers(&self) -> Vec<(ParticipantId, ProducerId)> {
		self.inner.clients
					.lock()
    				.iter()
        			.flat_map(|(participant_id, producers)| {
           				let participant_id = *participant_id;
						producers
							.iter()
							.map(move |producer| (participant_id, producer.id()))
           			})
              		.collect()
	}

	pub fn on_producer_add<F: Fn(&ParticipantId, &Producer) + Send + Sync + 'static>(&self, callback: F) -> HandlerId {
		self.inner.handlers.producer_add.add(Arc::new(callback))
	}

	pub fn on_producer_remove<F: Fn(&ParticipantId, &ProducerId) + Send + Sync + 'static>(&self, callback: F) -> HandlerId {
		self.inner.handlers.producer_remove.add(Arc::new(callback))
	}

	pub fn on_close<F: FnOnce() + Send + 'static>(&self, callback: F) -> HandlerId {
		let _ = MonitorDispatch::send_event(SFUEvent::RoomClosed {
			id: self.id()
		});

		self.inner.handlers.close.add(Box::new(callback))
	}
}

#[derive(Debug)]
pub struct WeakRoom {
	inner: Weak<Inner>
}
impl WeakRoom {
	pub fn upgrade(&self) -> Option<Room> {
		self.inner.upgrade().map(|inner| Room {inner})
	}
}
