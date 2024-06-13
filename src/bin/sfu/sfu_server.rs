use std::{net::{Ipv4Addr, SocketAddrV4}, sync::Arc};

use confroom_server::{monitoring::SFUEvent, uuids::RoomId};
use mediasoup::worker_manager::WorkerManager;
use parking_lot::Mutex;
use serde::Deserialize;
use std::sync::mpsc::Sender;
use crate::participant::ParticipantConnection;
use crate::room::Room;
use crate::rooms_registry::RoomsRegistry;
use crate::security::get_ssl_mode_settings;
use warp::{filters::{query::query, ws::{WebSocket, Ws}}, Filter};


#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct QueryParameters {
	room_id: Option<RoomId>
}

pub struct SFUServerConfig {
	pub port: u16,
}

pub struct SFUServerRuntime {
	pub worker_manager: WorkerManager,
	pub rooms: RoomsRegistry,
	pub event_sender: Option<Sender<SFUEvent>>
}

pub struct SFUServer {
	pub description: SFUServerConfig,
	pub runtime: Arc<Mutex<SFUServerRuntime>>,
}

impl SFUServerRuntime {
	fn new() -> Self {
		SFUServerRuntime {
			worker_manager: WorkerManager::new(),
			rooms: RoomsRegistry::new(),
			event_sender: None
		}
	}
}

impl Default for SFUServerConfig {
	fn default() -> Self {
		SFUServerConfig {
			port: 8000
		}
	}
}

impl Default for SFUServer {
	fn default() -> Self {
		SFUServer {
			description: SFUServerConfig::default(),
			runtime: Arc::new(Mutex::new(SFUServerRuntime::new())),
		}
	}
}

impl SFUServer {
	pub async fn run(&self) {

		let with_server_data = warp::any().map({
			let runtime = self.runtime.clone();
			move || runtime.clone()
		});

	   	let routes = warp::path!("ws")
	        .and(warp::ws())
	        .and(query::<QueryParameters>())
	        .and(with_server_data)
	        .map(|ws: Ws, query_parameters: QueryParameters, server_data: Arc<Mutex<SFUServerRuntime>>| {
	        	ws.on_upgrade(move |websocket| {
	         		handle_websocket(websocket, query_parameters, server_data)
	         	})
	    });

	    let socket_addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), self.description.port);

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

		self.send_sfu_event(SFUEvent::ServerStarted);
	}

	pub fn attach_event_sender(&self, sender: Sender<SFUEvent>) {
		self.runtime.lock().event_sender = Some(sender);
	}

	fn send_sfu_event(&self, evt: SFUEvent) {
		let runtime = self.runtime.lock();
		if let Some(sender) = &runtime.event_sender {
			// maybe add error handling here
			sender.send(evt);
		}
	}
}

async fn handle_websocket(websocket: WebSocket, query_parameters: QueryParameters, server_data: Arc<Mutex<SFUServerRuntime>>) {

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
