mod monitor;

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use lazy_static::lazy_static;
use monitor::Monitor;
use parking_lot::{Mutex, RwLock};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use confroom_server::monitoring::{SFUEvent, SFU_PORT};

lazy_static! {
	static ref DISPATCH: RwLock<Option<MonitorDispatch>> = RwLock::new(None);
}

#[derive(Default)]
struct Inner {
	monitors: Vec<Monitor>,
	/// An internal sender if the dispatch itself needs to send a message
	/// Safe to unwrap after run() has been called
	sender: Option<UnboundedSender<SFUEvent>>,
}

#[derive(Clone)]
pub struct MonitorDispatch {
	inner: Arc<Mutex<Inner>>
}

impl MonitorDispatch {

	fn new() -> Self {
		MonitorDispatch {
			inner: Arc::new(Mutex::new(Inner::default()))
		}
	}

	pub fn add_monitor(&self, monitor: Monitor) {
		self.inner.lock().monitors.push(monitor);
	}

	fn create_global_dispatch() {
		let mut dispatch = DISPATCH.write();
		match *dispatch {
			None => {
				dispatch.replace(MonitorDispatch::new());
			},
			Some(_) => panic!("Attempted to run a MonitorDispatch when one is already running")
		}
	}

	pub async fn run(channel: (UnboundedSender<SFUEvent>, UnboundedReceiver<SFUEvent>)) {

		MonitorDispatch::create_global_dispatch();
		// DISPATCH can safely be unwrapped now

		let dispatch = DISPATCH.read().clone().unwrap();

		dispatch.inner.lock().sender.replace(channel.0);
		// Sender is now available and can be safely unwrapped

		tokio::spawn({
			let other_dispatch = dispatch.clone();
			async move {
				other_dispatch.accept_incoming_connections().await;
			}
		});

		println!("Monitor dispatch is running");
		dispatch.receive_events(channel.1).await;
	}

	async fn receive_events(self, mut receiver: UnboundedReceiver<SFUEvent>) {
		while let Some(evt) = receiver.recv().await {

			let monitors = &self.inner.lock().monitors;

			for monitor in monitors.iter() {
				if monitor.is_interested(&evt) {
					monitor.sender.send(evt.clone());
				}
			}
		}

		println!("Monitor dispatch closing");
	}

	async fn accept_incoming_connections(self) {
		let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::new(0,0,0,0), SFU_PORT))
    		.await
			.expect("Failed to create TcpListener");

		println!("Monitor dispatch now accepting connections on port {SFU_PORT}");

		loop {
			let (stream, _) = listener.accept().await.expect("Failed to accept connection");
			// Todo: Make sure this is not a malicious connection, initiate the handshake, etc.

			self.add_monitor(Monitor::new(stream));
			MonitorDispatch::send_event(SFUEvent::MonitorAccepted);
		}
	}

	/// Orders all running MonitorDispatch to forward an event through the network
	pub fn send_event(evt: SFUEvent) {
		match *DISPATCH.read() {
			Some(ref dispatch) => {
				dispatch.inner.lock().sender.as_ref().unwrap().send(evt);
			},
			None => {
				// TODO: panic if not in NoMonitor mode
				println!("Trying to send an event to monitors but no MonitorDispatch is running");
			}
		}
	}
}
