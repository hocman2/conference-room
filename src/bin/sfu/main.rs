mod message;
mod room;
mod participant;
mod rooms_registry;
mod monitor_dispatch;
mod security;
mod sfu_server;

use monitor_dispatch::MonitorDispatch;
use sfu_server::{SFUServer, SFUServerConfig};
use clap::Parser;

#[derive(Clone, clap::ValueEnum, PartialEq)]
enum MonitoringMode {
	Secure,
	Unsecure,
	NoMonitoring
}

#[derive(clap::Parser)]
struct Args {
	#[arg(short='m', long="monitoring", value_enum, default_value_t=MonitoringMode::Secure)]
	monitoring_mode: MonitoringMode,
	#[arg(short='p', long="port", default_value_t=8000)]
	port: u16
}

#[tokio::main]
async fn main() {
	env_logger::init();
	let args = Args::parse();

	let sfu_server = SFUServer {
		description: SFUServerConfig {
			port: args.port
		},
		..Default::default()
	};

	if args.monitoring_mode != MonitoringMode::NoMonitoring {
		tokio::spawn(async move {
			MonitorDispatch::run().await;
		});
	}

	sfu_server.run().await;
}
