mod result;
pub use result::Result;

use async_lock::Mutex;
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
			let mut rooms = self.rooms.lock().await;

			match rooms.entry(room_id.clone()) {
				Entry::Occupied(mut entry) => match entry.get().upgrade() {
					Some(room) => Ok(room),
					None => {
						let room = Room::new_with_id(worker_manager, room_id).await?;
						entry.insert(room.downgrade());

						room.on_close({
							let rooms = Arc::clone(&self.rooms);
							let room_id = room.id();

							move || {
								tokio::spawn(async move {
									rooms.lock().await.remove(&room_id);
								});
							}
						})
						.detach();

						Ok(room)
					}
				},
				Entry::Vacant(entry) => {
					let room = Room::new_with_id(worker_manager, room_id).await?;
					entry.insert(room.downgrade());

					room.on_close({
						let rooms = Arc::clone(&self.rooms);
						let room_id = room.id();

						move || {
							tokio::spawn(async move {
								rooms.lock().await.remove(&room_id);
							});
						}
					})
					.detach();

					Ok(room)
				}
			}
	}

	pub async fn create_room(&self, worker_manager: &WorkerManager) -> Result<Room> {
		let mut rooms = self.rooms.lock().await;
		let room = Room::new(worker_manager).await?;

		rooms.insert(room.id(), room.downgrade());

		room.on_close({
			let rooms = Arc::clone(&self.rooms);
			let room_id = room.id();

			move || {
				tokio::spawn(async move {
					rooms.lock().await.remove(&room_id);
				});
			}
		})
		.detach();

		Ok(room)
	}
}
