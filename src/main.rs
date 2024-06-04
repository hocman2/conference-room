mod websocket;
mod room;
mod participant;

use mediasoup::prelude::*;
use warp::Filter;
use warp::ws::{WebSocket, Ws};

use std::sync::Arc;
use tokio::sync::Mutex;

use room::{Room, RoomsRegistry};
use participant::ParticipantConnection;

struct Server {
	worker_manager: WorkerManager,
	rooms: RoomsRegistry,
}

async fn handle_websocket(websocket: WebSocket, server_data: Arc<Mutex<Server>>) {

	let room: Room = {
		let server_data = server_data.lock().await;
		//Do some room creation/fetching here
		match Room::new(&server_data.worker_manager).await {
			Ok(room) => room,
			Err(e) => {
				eprintln!("Error creating a room: {e}");
				return;
			}
		}
	};

	match ParticipantConnection::new(room) {
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
        .and(with_server_data)
        .map(|ws: Ws, server_data: Arc<Mutex<Server>>| {
        	ws.on_upgrade(move |websocket| {
         		handle_websocket(websocket, server_data)
         	})
        });

    warp::serve(routes).run(([127, 0, 0, 1], 8000)).await;
}
