use crate::monitor_dispatch::Error;
use confroom_server::monitoring::{MonitorMessage, MonitoringEventCategory, SFUErrorReturn, SFUEvent};
use confroom_server::uuids::MonitorId;
use log::{debug, info};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, tcp::OwnedReadHalf, tcp::OwnedWriteHalf};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::mpsc;

pub enum MonitorToDispatchMessage {
	/// sent when a monitor connection closed or was dropped
	Close(MonitorId),
	/// a utility type when a monitor needs to send back a message to it's associated process
	SendMeThisMessage(MonitorId, SFUEvent)
}

#[derive(Clone)]
pub struct Monitor {
	pub id: MonitorId,
	pub event_sender: UnboundedSender<SFUEvent>,
	listening_category: MonitoringEventCategory,
}

impl Monitor {
	pub fn new(stream: TcpStream, message_sender: UnboundedSender<MonitorToDispatchMessage>) -> Self {

		let id = MonitorId::new();

		let (tcp_read, tcp_write) = stream.into_split();

		let (event_sender, event_receiver) = mpsc::unbounded_channel();

		let another_message_sender = message_sender.clone();
		tokio::spawn(async move {
			Monitor::wait_for_events(event_receiver, tcp_write, id.clone(), another_message_sender).await;
		});

		tokio::spawn(async move {
			Monitor::listen_to_monitor(tcp_read, id.clone(), message_sender).await;
		});

		Monitor {
			id,
			event_sender,
			listening_category: MonitoringEventCategory::Global,
		}
	}

	/// Returns true if that event is of interest for this monitor
	pub fn is_interested(&self, evt: &SFUEvent) -> bool {

		// Requires some manual maintenance ...
		match self.listening_category {
		    MonitoringEventCategory::Global => {
				matches!(evt,
					SFUEvent::MonitorAccepted |
					SFUEvent::ServerStarted |
					SFUEvent::RoomCreated{..} |
					SFUEvent::RoomDestroyed {..}
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

	async fn listen_to_monitor(mut listener: OwnedReadHalf, id: MonitorId, message_sender: UnboundedSender<MonitorToDispatchMessage>) {
		loop {
			let mut buf = [0; 1024];
			if let Err(e) = listener.read(&mut buf).await {
				log::error!("Error reading TcpStream: {e}");
			}
			match bincode::deserialize::<MonitorMessage>(&buf) {
				Ok(_message) => (),
				Err(_) => {
					info!("A remote monitor sent an invalid message!");

					Monitor::send_to_dispatch(
						&message_sender,
						// damn!!
						MonitorToDispatchMessage::SendMeThisMessage(
							id.clone(),
							SFUEvent::Error(SFUErrorReturn::UnreadableMessage)
						)
					);
				}
			}
		}
	}

	/// Waits for SFUEvents and forwards them through the TCP connection
	async fn wait_for_events(mut receiver: UnboundedReceiver<SFUEvent>, mut stream: OwnedWriteHalf, id: MonitorId, message_sender: UnboundedSender<MonitorToDispatchMessage>) {
		const ERRORS_BEFORE_CLOSING: i32 = 3;
		let mut num_send_errors = 0;

		while let Some(evt) = receiver.recv().await {

			match Monitor::send_remote(&mut stream, evt).await {
				Ok(_) => {num_send_errors = 0},
				Err(e) => {
					debug!("Error sending to a monitor process: {e:?}");
					num_send_errors += 1;
					if num_send_errors == ERRORS_BEFORE_CLOSING {
						info!("{num_send_errors} errors in a row while sending to monitor {id}. Closing connection");
						Monitor::send_to_dispatch(&message_sender, MonitorToDispatchMessage::Close(id.clone()));
						break;
					}
				}
			}

		}
	}

	/// Send an event to this monitor through the network
	pub fn send(&self, evt: SFUEvent) {
		match self.event_sender.send(evt) {
			Ok(_) => (),
		 	Err(_) => {
				log::error!("{:?}'s event receiver was dropped but the monitor is still alive. This should never happen", self.id);
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

	fn send_to_dispatch(message_sender: &UnboundedSender<MonitorToDispatchMessage>, msg: MonitorToDispatchMessage) {
		if let Err(_) = message_sender.send(msg) {
			log::error!("Monitor messaging channel was closed but a monitor still exists");
		}
	}
}
