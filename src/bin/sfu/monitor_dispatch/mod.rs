mod monitor;
mod error;
use error::Error;

use log::{debug, info};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use lazy_static::lazy_static;
use monitor::{Monitor, MonitorToDispatchMessage};
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
	event_receiver: UnboundedReceiver<SFUEvent>,
	monitor_message_receiver: UnboundedReceiver<MonitorToDispatchMessage>,
}

struct Inner {
	monitors: Vec<Monitor>,
	/// An internal sender if the dispatch itself needs to send a message
	/// Safe to unwrap after run() has been called
	event_sender: UnboundedSender<SFUEvent>,
	monitor_message_sender: UnboundedSender<MonitorToDispatchMessage>,
}

#[derive(Clone)]
pub struct MonitorDispatch {
	inner: Arc<Mutex<Inner>>
}

impl MonitorDispatch {

	fn new() -> MonitorDispatchConstructionPayload {

		let event_channel = mpsc::unbounded_channel();
		let monitor_message_channel = mpsc::unbounded_channel();

		let dispatch = MonitorDispatch {
			inner: Arc::new(Mutex::new(Inner {
				monitors: Vec::new(),
				event_sender: event_channel.0,
				monitor_message_sender: monitor_message_channel.0,
			}))
		};

		MonitorDispatchConstructionPayload {
			dispatch,
			event_receiver: event_channel.1,
			monitor_message_receiver: monitor_message_channel.1,
		}
	}

	pub fn add_monitor(&self, monitor: Monitor) {
		self.inner.lock().monitors.push(monitor);
	}

	/// Creates a new MonitorDispatch instance and inserts it in the global DISPATCH variable
	fn create_global_dispatch() -> (UnboundedReceiver<SFUEvent>, UnboundedReceiver<MonitorToDispatchMessage>) {
		let mut dispatch = DISPATCH.write();
		match *dispatch {
			None => {
				let payload = MonitorDispatch::new();
				dispatch.replace(payload.dispatch);

				return (payload.event_receiver, payload.monitor_message_receiver);
			},
			Some(_) => panic!("Attempted to run a MonitorDispatch when one is already running")
		}
	}

	pub async fn run() {

		let (event_receiver, monitor_msg_receiver) = MonitorDispatch::create_global_dispatch();
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

		tokio::spawn({
			let other_dispatch = dispatch.clone();
			async move {
				other_dispatch.receive_monitor_message(monitor_msg_receiver).await;
			}
		});

		info!("Monitor dispatch is running");
		dispatch.receive_events(event_receiver).await;
	}

	async fn receive_events(self, mut receiver: UnboundedReceiver<SFUEvent>) {
		while let Some(evt) = receiver.recv().await {

			let monitors = &self.inner.lock().monitors;

			for monitor in monitors.iter() {
				if monitor.is_interested(&evt) {
					monitor.send(evt.clone());
				}
			}
		}

		info!("Monitor dispatch closing");
	}

	async fn accept_incoming_connections(self) -> Result<(), Error> {
		let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::new(0,0,0,0), SFU_PORT)).await?;

		info!("Monitor dispatch now accepting connections on port {SFU_PORT}");

		loop {
			let (stream, _) = listener.accept().await?;
			// Todo: Make sure this is not a malicious connection, initiate the handshake, etc.

			let message_sender = self.inner.lock().monitor_message_sender.clone();
			self.add_monitor(Monitor::new(stream, message_sender));
			MonitorDispatch::send_event(SFUEvent::MonitorAccepted)?;
		}
	}

	async fn receive_monitor_message(self, mut monitor_message_receiver: UnboundedReceiver<MonitorToDispatchMessage>) {
		while let Some(msg) = monitor_message_receiver.recv().await {
			match msg {
				MonitorToDispatchMessage::Close(id) => {
					let monitors = &mut self.inner.lock().monitors;
					if let Some(index) = monitors.iter().position(|m| m.id == id) {
						monitors.remove(index);
					}
				}
				MonitorToDispatchMessage::SendMeThisMessage(id, evt) => {
					let monitors = &self.inner.lock().monitors;
					if let Some(monitor) = monitors.iter().find(|m| m.id == id) {
						monitor.send(evt);
					}
				}
			}
		}
	}

	/// Orders the global MonitorDispatch instance to send an event through the network to interested monitors
	pub fn send_event(evt: SFUEvent) -> Result<(), Error> {
		match *DISPATCH.read() {
			Some(ref dispatch) => {
				// 🚂
				dispatch.inner.lock().event_sender.send(evt).unwrap();
				Ok(())
			},
			None => {
				// TODO: panic if not in NoMonitor mode
				Err(Error::NoDispatchRunning)
			}
		}
	}

}
