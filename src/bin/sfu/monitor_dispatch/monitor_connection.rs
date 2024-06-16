use std::sync::Arc;

use crate::monitor_dispatch::Error;
use confroom_server::monitoring::{MonitorMessage, MonitoringEventCategory, SFUErrorReturn, SFUEvent};
use confroom_server::uuids::MonitorId;
use log::{debug, info};
use parking_lot::RwLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, tcp::OwnedReadHalf, tcp::OwnedWriteHalf};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::mpsc;
use event_listener_primitives::{Bag, HandlerId};

struct Inner {
	id: MonitorId,
	sfu_evt_tx: UnboundedSender<SFUEvent>,
	listening_category: RwLock<MonitoringEventCategory>,
	connection_closed: Bag<Arc<dyn Fn() + Send + Sync + 'static>>
}

#[derive(Clone)]
pub struct MonitorConnection {
	inner: Arc<Inner>
}

impl MonitorConnection {
	pub fn new(stream: TcpStream) -> Self {
		// this sfu event channel is not meant to be interacted with from the outside
		// it's sole purpose is to pass around events between internal tasks
		let (sfu_evt_tx, sfu_evt_rx) = mpsc::unbounded_channel();

		let conn = MonitorConnection {
			inner: Arc::new(Inner {
				id: MonitorId::new(),
				sfu_evt_tx,
				listening_category: RwLock::new(MonitoringEventCategory::Global),
				connection_closed: Bag::default()
			})
		};

		let (tcp_read, tcp_write) = stream.into_split();

		tokio::spawn({
			let conn = conn.clone();
			async move {
				conn.wait_for_events(sfu_evt_rx, tcp_write).await;
			}
		});

		tokio::spawn({
			let conn = conn.clone();
			async move {
				conn.listen_to_monitor(tcp_read).await;
			}
		});

		conn
	}

	/// Returns true if that event is of interest for this monitor
	pub fn is_interested_in(&self, evt: &SFUEvent) -> bool {

		// Requires some manual maintenance ...
		match *self.inner.listening_category.read() {
		    MonitoringEventCategory::Global => {
				matches!(evt,
					SFUEvent::MonitorAccepted |
					SFUEvent::ServerStarted |
					SFUEvent::RoomOpened{..} |
					SFUEvent::RoomClosed {..}
				)
			},
		    MonitoringEventCategory::Room(_listening_room_id) => {
				matches!(evt,
					SFUEvent::ParticipantEntered {room_id:_listening_room_id, ..} |
					SFUEvent::ParticipantLeft {room_id:_listening_room_id, ..}
				)
			},
		}
	}

	pub fn id(&self) -> MonitorId {
		self.inner.id.clone()
	}

	pub fn on_connection_closed<F>(&self, callback: F) -> HandlerId
	where F: Fn() + Send + Sync + 'static {
		self.inner.connection_closed.add(Arc::new(callback))
	}

	// this is the actual API function to send events
	/// Send an event to the connected monitor through the network
	pub fn send(&self, evt: SFUEvent) {
		match self.inner.sfu_evt_tx.send(evt) {
			Ok(_) => (),
		 	Err(_) => {
				log::error!("{:?}'s event receiver was dropped but the monitor is still alive. This should never happen", self.id());
			}
		}
	}

	async fn listen_to_monitor(self, mut tcp_listener: OwnedReadHalf) {
		loop {
			let mut buf = [0; 1024];
			if let Err(e) = tcp_listener.read(&mut buf).await {
				log::error!("Error reading TcpStream: {e}");
			}
			match bincode::deserialize::<MonitorMessage>(&buf) {
				Ok(_message) => (),
				Err(_) => {
					info!("A remote monitor sent an invalid message!");

					self.send(SFUEvent::Error(SFUErrorReturn::UnreadableMessage));
				}
			}
		}
	}

	/// Waits for SFUEvents and forwards them through the TCP connection
	/// We could technically directly use the send_remote function but by using a channel we can do error handling inside,
	/// easing the send process for the dispatch
	async fn wait_for_events(self, mut sfu_evt_rx: UnboundedReceiver<SFUEvent>, mut tcp_writer: OwnedWriteHalf) {
		const ERRORS_BEFORE_CLOSING: i32 = 3;
		let mut num_send_errors = 0;

		while let Some(event) = sfu_evt_rx.recv().await {

			match MonitorConnection::send_remote(&mut tcp_writer, event).await {
				Ok(_) => {num_send_errors = 0},
				Err(e) => {
					debug!("Error sending to a monitor process: {e:?}");
					num_send_errors += 1;
					if num_send_errors == ERRORS_BEFORE_CLOSING {
						info!("{num_send_errors} errors in a row while sending to monitor {}. Closing connection", self.id());
						self.inner.connection_closed.call_simple();
						break;
					}
				}
			}

		}
	}

	// This is where we actually send the event through the network
	async fn send_remote(stream: &mut OwnedWriteHalf, event: SFUEvent) -> Result<(), Error> {
		match bincode::serialize(&event) {
			Ok(evt_bytes) => {
				stream.write(evt_bytes.as_slice()).await?;
				stream.flush().await?;
			},
			Err(e) => log::error!("Failed to serialize event. \
				Serialization of events sent from the SFU to a monitor shouldn't fail: {e}")
		};

		Ok(())
	}
}
