use uuid::Uuid;
use serde::{Deserialize, Serialize};

// Simple uuid representing a room's id
#[derive(Debug, PartialEq, Eq, Hash, Clone, Deserialize, Serialize, Copy)]
pub struct RoomId(Uuid);
impl RoomId {
	pub fn new() -> Self {
		RoomId(Uuid::new_v4())
	}
}
impl std::fmt::Display for RoomId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(&self.0, f)
	}
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, Copy)]
pub struct ParticipantId(Uuid);
impl ParticipantId {
	pub fn new() -> Self {
		ParticipantId(Uuid::new_v4())
	}
}

impl std::fmt::Display for ParticipantId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(&self.0, f)
	}
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Deserialize, Serialize, Copy)]
pub struct MonitorId(Uuid);
impl MonitorId {
	pub fn new() -> Self {
		MonitorId(Uuid::new_v4())
	}
}
impl std::fmt::Display for MonitorId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(&self.0, f)
	}
}
