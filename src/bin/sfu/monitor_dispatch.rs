mod monitor_connection;
mod error;
use error::Error;

use log::{debug, info};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use lazy_static::lazy_static;
use monitor_connection::MonitorConnection;
use parking_lot::{Mutex, RwLock};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use confroom_server::monitoring::{SFUEvent, SFU_PORT};

lazy_static! {
	static ref DISPATCH: RwLock<Option<MonitorDispatch>> = RwLock::new(None);
}

/// Data returned by a call of "new" on the MonitorDispatch
struct MonitorDispatchConstructionPayload {
	dispatch: MonitorDispatch,
	sfu_evt_rx: UnboundedReceiver<SFUEvent>,
}

struct Inner {
	monitors: Vec<MonitorConnection>,
	/// An internal sender if the dispatch itself needs to send a message
	sfu_evt_tx: UnboundedSender<SFUEvent>,
}

#[derive(Clone)]
pub struct MonitorDispatch {
	inner: Arc<Mutex<Inner>>
}

impl MonitorDispatch {
	// this is the actual API function to send events
	/// Orders the global MonitorDispatch instance to send an event through the network to interested monitors
	pub fn send_event(evt: SFUEvent) -> Result<(), Error> {
		match *DISPATCH.read() {
			Some(ref dispatch) => {
				// 🚂
				dispatch.inner.lock().sfu_evt_tx.send(evt).unwrap();
				Ok(())
			},
			None => {
				// TODO: panic if not in NoMonitor mode
				Err(Error::NoDispatchRunning)
			}
		}
	}

	/// Run a MonitorDispatch ready to receive SFUEvents from anywhere using the `MonitorDispatch::send()` function
	/// Only one MonitorDispatch can run through the program, attempting to run another will panic.
	pub async fn run() {

		let sfu_evt_rx = MonitorDispatch::create_global_dispatch();
		// DISPATCH can safely be unwrapped now

		let dispatch = DISPATCH.read().clone().unwrap();

		tokio::spawn({
			let other_dispatch = dispatch.clone();
			async move {
				if let Err(e) = other_dispatch.accept_incoming_connections().await {
					debug!("Error accepting incoming connections: {e:?}");
				}
			}
		});

		info!("Monitor dispatch is running");
		dispatch.receive_events(sfu_evt_rx).await;
	}

	fn new() -> MonitorDispatchConstructionPayload {

		let sfu_evt_channel = mpsc::unbounded_channel();

		let dispatch = MonitorDispatch {
			inner: Arc::new(Mutex::new(Inner {
				monitors: Vec::new(),
				sfu_evt_tx: sfu_evt_channel.0,
			}))
		};

		MonitorDispatchConstructionPayload {
			dispatch,
			sfu_evt_rx: sfu_evt_channel.1,
		}
	}

	/// Creates a new MonitorDispatch instance and inserts it in the global DISPATCH variable
	fn create_global_dispatch() -> UnboundedReceiver<SFUEvent> {
		let mut dispatch = DISPATCH.write();
		match *dispatch {
			None => {
				let payload = MonitorDispatch::new();
				dispatch.replace(payload.dispatch);

				return payload.sfu_evt_rx;
			},
			Some(_) => panic!("Attempted to run a MonitorDispatch when one is already running")
		}
	}

	async fn receive_events(self, mut sfu_evt_rx: UnboundedReceiver<SFUEvent>) {
		while let Some(evt) = sfu_evt_rx.recv().await {

			let monitors = &self.inner.lock().monitors;

			for monitor in monitors.iter() {
				if monitor.is_interested_in(&evt) {
					monitor.send(evt.clone());
				}
			}
		}

		info!("Monitor dispatch closing");
	}

	fn add_monitor(&self, monitor: MonitorConnection) {
		self.inner.lock().monitors.push(monitor);
	}

	async fn accept_incoming_connections(self) -> Result<(), Error> {
		let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::new(0,0,0,0), SFU_PORT)).await?;

		info!("Monitor dispatch now accepting connections on port {SFU_PORT}");

		loop {
			let (stream, _) = listener.accept().await?;
			// Todo: Make sure this is not a malicious connection, initiate the handshake, etc.

			let monitor = MonitorConnection::new(stream);
			monitor.on_connection_closed({
				let other_self = self.clone();
				let monitor_id = monitor.id();
				move || {
					let monitors = &mut other_self.inner.lock().monitors;
					if let Some(index) = monitors.iter().position(|m| m.id() == monitor_id) {
						monitors.remove(index);
					}
				}
			}).detach();

			self.add_monitor(monitor);
			MonitorDispatch::send_event(SFUEvent::MonitorAccepted)?;
		}
	}
}
