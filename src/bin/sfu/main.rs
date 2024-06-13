mod message;
mod room;
mod participant;
mod rooms_registry;
mod monitor_dispatch;
mod security;
mod web_server;

use web_server::WebServer;
use std::sync::Arc;

#[tokio::main]
async fn main() {

	Arc::new(WebServer::default())
		.run()
		.await;
}
