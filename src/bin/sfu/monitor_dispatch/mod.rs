mod monitor;

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use monitor::Monitor;
use parking_lot::Mutex;
use tokio::net::TcpListener;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use confroom_server::monitoring::{SFUEvent, SFU_PORT};

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

	pub fn new() -> Self {
		MonitorDispatch {
			inner: Arc::new(Mutex::new(Inner::default()))
		}
	}

	pub fn add_monitor(&self, monitor: Monitor) {
		self.inner.lock().monitors.push(monitor);
	}

	pub async fn run(self, channel: (UnboundedSender<SFUEvent>, UnboundedReceiver<SFUEvent>)) {

		{
			// Sender is now available
			self.inner.lock().sender = Some(channel.0);
		}

		tokio::spawn({
			let other_self = self.clone();
			async move {
				other_self.accept_incoming_connections().await;
			}
		});

		println!("Monitor dispatch is running");
		self.receive_events(channel.1).await;
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
			self.send_inner_message(SFUEvent::MonitorAccepted);
		}
	}


	/// Sends a message to monitors from the dispatch itself
	fn send_inner_message(&self, evt: SFUEvent) {
		// 🚂
		let sender = self.inner.lock().sender.clone().unwrap();
		sender.send(evt);
	}
}
