use confroom_server::monitoring::{MonitoringEventCategory, SFUEvent};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::{io::AsyncWriteExt, net::TcpStream, sync::mpsc::UnboundedSender};
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct Monitor {
	listening_category: MonitoringEventCategory,
	pub sender: UnboundedSender<SFUEvent>,
}

impl Monitor {
	pub fn new(stream: TcpStream) -> Self {

		let (sender, receiver) = mpsc::unbounded_channel();
		tokio::spawn(async move {
			Monitor::wait_for_events(receiver, stream).await;
		});

		Monitor {
			sender,
			listening_category: MonitoringEventCategory::Global
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

	async fn wait_for_events(mut receiver: UnboundedReceiver<SFUEvent>, mut stream: TcpStream) {
		while let Some(evt) = receiver.recv().await {
			Monitor::send_remote(&mut stream, evt).await;
		}
	}

	/// Send the event through the network
	async fn send_remote(stream: &mut TcpStream, event: SFUEvent) {
		match bincode::serialize(&event) {
			Ok(evt_bytes) => {
				stream.write(evt_bytes.as_slice()).await;
				stream.flush().await;
			},
			Err(e) => eprintln!("Failed to serialize event: {e}")
		}
	}
}
