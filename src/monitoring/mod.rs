pub mod handshake;
use serde::{Deserialize, Serialize};

use crate::uuids::{ParticipantId, RoomId};

pub const SFU_PORT: u16 = 12346;

/// The kind of events a Monitor subscribes to
#[derive(Clone)]
pub enum MonitoringEventCategory {
	Global,
	Room(RoomId)
}

/// Events sent by the SFU to a Monitor process
#[derive(Serialize, Deserialize, Clone)]
pub enum SFUEvent {
	/// This should be broadcasted only to the concerned monitor, add MonitorId?
	MonitorAccepted,
	/// This is kind of a pointless event because it's unlikely for a monitor to be accepted before the server starts
	ServerStarted,
	RoomCreated { id: RoomId },
	RoomDestroyed { id: RoomId },
	ParticipantEntered {
		room_id: RoomId,
		participant_id: ParticipantId
	},
	ParticipantLeft {
		room_id: RoomId,
		participant_id: ParticipantId
	},
}
