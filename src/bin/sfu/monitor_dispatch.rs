use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use parking_lot::Mutex;
use std::net::{TcpListener, TcpStream};
use confroom_server::monitoring::{MonitoringEventCategory, SFUEvent, SFU_PORT};

pub struct Monitor {
	stream: TcpStream,
	listening_category: MonitoringEventCategory,
}

#[derive(Default)]
struct Inner {
	monitors: Vec<Monitor>
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

	pub async fn run(self, receiver: Receiver<SFUEvent>) {
		let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::new(0,0,0,0), SFU_PORT))
									.expect("Failed to create TcpListener");

		tokio::spawn({
			let dispatch = self.clone();
			dispatch.accept_incoming_connections(listener)
		});

		self.receive_events(receiver);
	}

	fn receive_events(self, receiver: Receiver<SFUEvent>) {
		while let Ok(evt) = receiver.recv() {
			let inner = self.inner.lock();
			for monitor in inner.monitors.iter() {
				if monitor.is_interested(&evt) {
					todo!("Send event through the wire");
				}
			}
		}
	}

	async fn accept_incoming_connections(self, listener: TcpListener) {
		loop {
			let (stream, _) = listener.accept().expect("Failed to accept connection");

			self.add_monitor(Monitor {
				stream,
				listening_category: MonitoringEventCategory::Global
			});
		}
	}
}

impl Monitor {
	/// Returns true if that event is of interest for this monitor
	pub fn is_interested(&self, evt: &SFUEvent) -> bool {

		// Requires some manual maintenance ...
		match self.listening_category {
		    MonitoringEventCategory::Global => {
				matches!(evt,
					SFUEvent::ServerStarted |
					SFUEvent::RoomCreated{..} |
					SFUEvent::RoomDestroyed {..}
				)
			},
		    MonitoringEventCategory::Room(listening_room_id) => {
				matches!(evt,
					SFUEvent::ParticipantEntered {room_id:listening_room_id, ..} |
					SFUEvent::ParticipantLeft {room_id:listening_room_id, ..}
				)
			},
		}
	}
}
