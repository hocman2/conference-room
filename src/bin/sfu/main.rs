mod message;
mod room;
mod participant;
mod rooms_registry;

use confroom_server::MONITORING_SFU_PORT;
use mediasoup::prelude::*;
use serde::Deserialize;
use tokio::net::TcpListener;
use warp::filters::query::query;
use warp::Filter;
use warp::ws::{WebSocket, Ws};

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use std::env;
use parking_lot::Mutex;

use room::{Room, RoomId};
use rooms_registry::RoomsRegistry;
use participant::ParticipantConnection;

struct Server {
	worker_manager: WorkerManager,
	rooms: RoomsRegistry,
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct QueryParameters {
	room_id: Option<RoomId>
}

const SSL_MODE_ENV_KEY: &str = "SSL_MODE";
const SSL_CERT_PATH_ENV_KEY: &str = "SSL_CERT_PATH";
const SSL_KEY_PATH_ENV_KEY: &str = "SSL_KEY_PATH";
struct SSLModeSettings {
	cert_path: String,
	key_path: String
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
		Ok(conn) => conn.run(websocket).await,
		Err(e) => eprintln!("Error creating participant connection: {e}")
	}
}

fn get_ssl_mode_settings() -> Option<SSLModeSettings> {
	let secure_mode = match env::var(SSL_MODE_ENV_KEY) {
    	Ok(val) =>  match val.parse::<i32>() {
     		Ok(val) => val > 0,
       		Err(e) => panic!("Error parsing SSL_MODE value: {e}")
     	},
    	Err(_) => {
     		println!("{SSL_MODE_ENV_KEY} was not found. Running in non-secure mode.");
     		println!("The environment variable {SSL_MODE_ENV_KEY}=1 is required on an environment using HTTPS");
       		false
     	}
    };

	if !secure_mode {
		return None;
	}

	let cert_path = match env::var(SSL_CERT_PATH_ENV_KEY) {
		Ok(path) => path,
		Err(_) => panic!("The {SSL_CERT_PATH_ENV_KEY} environment variable is required when {SSL_MODE_ENV_KEY}=1")
	};

	let key_path = match env::var(SSL_KEY_PATH_ENV_KEY) {
		Ok(path) => path,
		Err(_) => panic!("The {SSL_KEY_PATH_ENV_KEY} environment variable is required when {SSL_MODE_ENV_KEY}=1")
	};

	Some(SSLModeSettings {
		cert_path,
		key_path
	})
}

#[tokio::main]
async fn main() {

	let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::new(0,0,0,0), MONITORING_SFU_PORT)).await.unwrap().accept().await.unwrap();

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

    // Stupid syntax
    let server = warp::serve(routes);
    if let Some(ssl_settings) = get_ssl_mode_settings() {
    	println!("Serving on {socket_addr}");
    	server
     		.tls()
       		.cert_path(ssl_settings.cert_path)
       		.key_path(ssl_settings.key_path)
     		.run(socket_addr).await;
    } else {
    	println!("Serving on {socket_addr}");
    	server.run(socket_addr).await;
    }

}
