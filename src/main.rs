mod message;
mod websocket;
mod room;
mod participant;
mod rooms_registry;

use mediasoup::prelude::*;
use serde::Deserialize;
use warp::filters::query::query;
use warp::Filter;
use warp::ws::{WebSocket, Ws};

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use parking_lot::Mutex;

use room::{Room, RoomId};
use rooms_registry::RoomsRegistry;
use participant::ParticipantConnection;

struct Server {
	worker_manager: WorkerManager,
	rooms: RoomsRegistry,
}

#[derive(Deserialize)]
struct QueryParameters {
	room_id: Option<RoomId>
}

async fn handle_websocket(websocket: WebSocket, query_parameters: QueryParameters, server_data: Arc<Mutex<Server>>) {

	let room: Room = {

		// Retrieve internal of server data
		let (worker_manager, rooms) = {
			let server_data = server_data.lock();
			(server_data.worker_manager.clone(), server_data.rooms.clone())
		};

		let room_maybe = match query_parameters.room_id.clone() {
			Some(room_id) => rooms.get_or_create(room_id, &worker_manager).await,
			None => rooms.create_room(&worker_manager).await
		};

		match room_maybe {
			Ok(room) => room,
			Err(e) => {
				eprintln!("Error creating or fetching room with id {:?}: {e}", query_parameters.room_id);
				// We should probably send a message to the client here
				return;
			}
		}
	};

	match ParticipantConnection::new(room).await {
		Ok(mut conn) => conn.run(websocket).await,
		Err(e) => eprintln!("Error creating participant connection: {e}")
	}
}

#[tokio::main]
async fn main() {
	let server_data = Arc::new(Mutex::new(
		Server {
			worker_manager: WorkerManager::new(),
			rooms: RoomsRegistry::new(),
	}));

	let with_server_data = warp::any().map(move || server_data.clone());

    let routes =
    	warp::path!("ws")
        .and(warp::ws())
        .and(query::<QueryParameters>())
        .and(with_server_data)
        .map(|ws: Ws, query_parameters: QueryParameters, server_data: Arc<Mutex<Server>>| {
        	ws.on_upgrade(move |websocket| {
         		handle_websocket(websocket, query_parameters, server_data)
         	})
        });

    let socket_addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8000);
    println!("Serving on {socket_addr}");
    warp::serve(routes).run(socket_addr).await;
}
