use std::{env, net::{Ipv4Addr, SocketAddrV4}, sync::Arc};

use confroom_server::{monitoring::SFUEvent, uuids::RoomId};
use parking_lot::Mutex;
use serde::Deserialize;
use crate::{monitor_dispatch::MonitorDispatch, participant::ParticipantConnection, router_dispatch::{RouterDispatch, RouterDispatchConfig, ANNOUNCED_ADDRESS_ENV_KEY}};
use crate::room::Room;
use crate::rooms_registry::RoomsRegistry;
use crate::security::get_tls_mode_settings;
use warp::{filters::{query::query, ws::{WebSocket, Ws}}, Filter};

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct QueryParameters {
	room_id: Option<RoomId>
}

pub struct SFUServerConfig {
	pub port: u16,
	pub router_dispatch_config: Option<RouterDispatchConfig>
}

pub struct SFUServerRuntime {
	router_dispatch: RouterDispatch,
	rooms: RoomsRegistry,
}

#[derive(Clone)]
pub struct SFUServer {
	port: u16,
	pub runtime: Arc<Mutex<SFUServerRuntime>>,
}

impl SFUServerRuntime {
	fn new(dispatch_config: RouterDispatchConfig) -> Self {
		SFUServerRuntime {
			router_dispatch: RouterDispatch::new(dispatch_config),
			rooms: RoomsRegistry::new(),
		}
	}
}

impl Default for SFUServerRuntime {
	fn default() -> Self {
		SFUServerRuntime {
			router_dispatch: RouterDispatch::default(),
			rooms: RoomsRegistry::new()
		}
	}
}

impl Default for SFUServerConfig {
	fn default() -> Self {
		SFUServerConfig {
			port: 8000,
			router_dispatch_config: None,
		}
	}
}

impl Default for SFUServer {
	fn default() -> Self {
		SFUServer {
			port: SFUServerConfig::default().port,
			runtime: Arc::new(Mutex::new(SFUServerRuntime::default())),
		}
	}
}

impl SFUServer {
	pub fn new(config: SFUServerConfig) -> Self {
		SFUServer {
			port: config.port,
			runtime: Arc::new(
				Mutex::new(
					SFUServerRuntime::new(
						config.router_dispatch_config.unwrap_or(RouterDispatchConfig::default()))
				)
			)
		}
	}

	pub async fn run(&self) {
		let with_server_data = warp::any().map({
			let server = self.clone();
			move || server.clone()
		});

	   	let routes = warp::path!("ws")
	        .and(warp::ws())
	        .and(query::<QueryParameters>())
	        .and(with_server_data)
	        .map(|ws: Ws, query_parameters: QueryParameters, server: SFUServer| {
	        	ws.on_upgrade(move |websocket| {
	         		handle_websocket(websocket, query_parameters, server)
	         	})
	    });

	    let socket_addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), self.port);

		let _ = MonitorDispatch::send_event(SFUEvent::ServerStarted);

		if let Err(_) = env::var(ANNOUNCED_ADDRESS_ENV_KEY) {
			println!("The {ANNOUNCED_ADDRESS_ENV_KEY} environment variable isn't set. The server will only listen to localhost participants");
		}

	    // Stupid syntax
	    let server = warp::serve(routes);
	    if let Some(tls_settings) = get_tls_mode_settings() {
	    	println!("Serving on {socket_addr} in secure mode");
	    	server
	     		.tls()
	       		.cert_path(tls_settings.cert_path)
	       		.key_path(tls_settings.key_path)
	     		.run(socket_addr).await;
	    } else {
	    	println!("Serving on {socket_addr} in non-secure mode");
	    	server.run(socket_addr).await;
	    }

		let _ = MonitorDispatch::send_event(SFUEvent::ServerClosed);
	}
}

async fn handle_websocket(websocket: WebSocket, query_parameters: QueryParameters, server: SFUServer) {

	let router_dispatch = server.runtime.lock().router_dispatch.clone();
	let router_data = match router_dispatch.create_router().await {
		Ok(data) => data,
		Err(e) => {
			log::error!("Failed to create router, no room will be fetched/created: {e}");
			// Return early, don't create any connection
			return;
		}
	};

	let room: Room = {
		let rooms = server.runtime.lock().rooms.clone();
		let room_maybe = match query_parameters.room_id.clone() {
			Some(room_id) => rooms.get_or_create(room_id, router_data).await,
			None => rooms.create_room(router_data).await
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
		Err(e) => {
			log::error!("{e}");
			eprintln!("Error creating participant connection");
		}
	}
}
