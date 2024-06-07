mod result;
pub use result::Result;

use parking_lot::Mutex;
use std::{collections::hash_map::Entry, sync::Arc};
use std::collections::HashMap;
use crate::room::{RoomId, WeakRoom, Room};
use mediasoup::prelude::*;


#[derive(Debug, Default, Clone)]
pub struct RoomsRegistry {
	rooms: Arc<Mutex<HashMap<RoomId, WeakRoom>>>
}
impl RoomsRegistry {
	pub fn new() -> Self {
		RoomsRegistry { rooms: Arc::new(Mutex::new(HashMap::new())) }
	}

	pub async fn get_or_create(
		&self,
		room_id: RoomId,
		worker_manager: &WorkerManager) -> Result<Room> {

		// First lock the rooms and check if a room exists with that ID
		if let Entry::Occupied(entry) = self.rooms.lock().entry(room_id.clone()) {
			if let Some(room) = entry.get().upgrade() {
				return Ok(room)
			}
		}

		// No room exists, create a new one
		let room = Room::new_with_id(worker_manager, room_id).await?;

		let mut rooms = self.rooms.lock();
		rooms.insert(room_id, room.downgrade());

		room.on_close({
			let rooms = Arc::clone(&self.rooms);
			let room_id = room.id();

			move || {
				tokio::spawn(async move {
					rooms.lock().remove(&room_id);
				});
			}
		})
		.detach();

		Ok(room)
	}

	pub async fn create_room(&self, worker_manager: &WorkerManager) -> Result<Room> {
		let room = Room::new(worker_manager).await?;

		self.rooms
				.lock()
				.insert(room.id(), room.downgrade());

		room.on_close({
			let rooms = Arc::clone(&self.rooms);
			let room_id = room.id();

			move || {
				tokio::spawn(async move {
					rooms.lock().remove(&room_id);
				});
			}
		})
		.detach();

		Ok(room)
	}
}
