use serde::{Deserialize, Serialize};

use crate::uuids::{ParticipantId, RoomId};

pub const SFU_PORT: u16 = 12346;

/// The kind of events a Monitor subscribes to
#[derive(Serialize, Deserialize, Clone)]
pub enum MonitoringEventCategory {
	Global,
	Room(RoomId)
}

/// Message sent by a monitor to the server
/// If something goes wrong, the SFU will send back a descriptive SFUEvent::Error message
#[derive(Serialize, Deserialize, Clone)]
pub enum MonitorMessage {
	Greeting(String),
	SwitchCategory(MonitoringEventCategory)
}

/// Error messages sent back by the SFU when something goes wrong after a Monitor's request
#[derive(Serialize, Deserialize, Clone)]
pub enum SFUErrorReturn {
	/// Sent back when the monitor sends data that it failed to interpret as a valid MonitorMessage
	UnreadableMessage
}

/// Events sent by the SFU to a Monitor process
#[derive(Serialize, Deserialize, Clone)]
pub enum SFUEvent {
	/// This should be broadcasted only to the concerned monitor, add MonitorId?
	MonitorAccepted,
	/// This is kind of a pointless event because it's unlikely for a monitor to be accepted before the server starts
	ServerStarted,
	ServerClosed,
	RoomOpened { id: RoomId },
	RoomClosed { id: RoomId },
	ParticipantEntered {
		room_id: RoomId,
		participant_id: ParticipantId
	},
	ParticipantLeft {
		room_id: RoomId,
		participant_id: ParticipantId
	},
	Error(SFUErrorReturn)
}
