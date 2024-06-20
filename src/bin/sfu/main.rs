mod message;
mod room;
mod participant;
mod rooms_registry;
mod monitor_dispatch;
mod security;
mod sfu_server;
mod router_dispatch;

use monitor_dispatch::MonitorDispatch;
use router_dispatch::RouterDispatchConfig;
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
	/// The port for the websocket listener
	#[arg(short='p', long="port", default_value_t=8000)]
	port: u16,
	/// The maximum number of workers, there is one worker per CPU logical unit. 0 means max CPU (physical + logic),
	/// any other value is bound to max CPU
	#[arg(short='w', long="max-workers", default_value_t=0)]
	max_workers: usize,
	/// The approximate number of consumers per worker unit. It might be a bit higher than that in real execution
	/// Mediasoup documentation recommands ~500 but it depends on the CPU
	#[arg(short='c', long="consumers", default_value_t=500)]
	consumers_per_worker: u32,
}

#[tokio::main]
async fn main() {
	env_logger::init();
	let args = Args::parse();

	let max_workers = if args.max_workers == 0 || args.max_workers > num_cpus::get() {
		num_cpus::get()
	} else {
		args.max_workers
	};

	let sfu_server = SFUServer::new(SFUServerConfig {
		port: args.port,
		router_dispatch_config: Some(RouterDispatchConfig {
			max_workers,
			consumers_per_worker: args.consumers_per_worker,
		})
	});

	if args.monitoring_mode != MonitoringMode::NoMonitoring {
		tokio::spawn(async move {
			MonitorDispatch::run().await;
		});
	}

	sfu_server.run().await;
}
