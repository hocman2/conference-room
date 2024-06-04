mod error;
pub use error::{Result, Error};

use uuid::Uuid;
use std::{collections::HashMap, sync::{Arc, Mutex, Weak}};
use mediasoup::prelude::*;

pub struct RoomId(Uuid);
impl RoomId {
	fn new() -> Self {
		RoomId(Uuid::new_v4())
	}
}

pub struct Inner {
	id: RoomId
}

pub struct Room {
	inner: Arc<Inner>
}
impl Room {
	pub async fn new(worker_manager: &WorkerManager) -> Result<Self> {
		Self::new_with_id(worker_manager, RoomId::new()).await
	}

	pub async fn new_with_id(worker_manager: &WorkerManager, id: RoomId) -> Result<Self> {
		Ok(Room {
			inner: Arc::new(Inner { id })
		})
	}

	pub fn downgrade(&self) -> WeakRoom {
		WeakRoom { inner: Arc::downgrade(&self.inner) }
	}
}

pub struct WeakRoom {
	inner: Weak<Inner>
}
impl WeakRoom {
	pub fn upgrade(&self) -> Option<Room> {
		self.inner.upgrade().map(|inner| Room {inner})
	}
}

pub struct RoomsRegistry {
	rooms: Arc<Mutex<HashMap<RoomId, WeakRoom>>>
}
impl RoomsRegistry {
	pub fn new() -> Self {
		RoomsRegistry { rooms: Arc::new(Mutex::new(HashMap::new())) }
	}
}
