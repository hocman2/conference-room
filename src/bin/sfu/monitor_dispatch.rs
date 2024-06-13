use std::sync::Arc;

use tokio::{net::TcpStream, sync::mpsc::UnboundedReceiver};
use confroom_server::monitoring::{MonitoringEventCategory, SFUEvent};

pub struct Monitor {
	stream: TcpStream,
	listening_category: MonitoringEventCategory,
}

#[derive(Default)]
pub struct MonitorDispatch {
	monitors: Vec<Monitor>
}

impl Monitor {
	/// Returns true if that event is of interest for this monitor
	pub fn is_interesting(&self, evt: SFUEvent) -> bool {

		// Requires some manual maintenance ...
		match self.listening_category {
		    MonitoringEventCategory::Global => {
				matches!(evt,
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

pub async fn run_monitor_dispatch(evt_receiver: UnboundedReceiver<SFUEvent>) {
	let dispatch = MonitorDispatch::default();
}
