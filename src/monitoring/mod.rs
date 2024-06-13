pub mod handshake;
use crate::uuids::{ParticipantId, RoomId};

pub const SFU_PORT: u16 = 12346;

/// The kind of events a Monitor subscribes to
pub enum MonitoringEventCategory {
	Global,
	Room(RoomId)
}

/// Events sent by the SFU to a Monitor process
pub enum SFUEvent {
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
