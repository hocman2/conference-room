use serde::{Deserialize, Serialize};

pub type HandshakeError = Box<dyn std::error::Error + Send + Sync>;

/// Messages sent by the Monitor process to handshake the SFU and confirm it's identity
#[derive(Serialize)]
pub enum MonitorHandshakeMessage {
	/// We may use this greeting message to further restrict access to the SFU by enforcing some specific data
	Greeting { greet: &'static str },
	Identification { key: String }
}

/// Messages sent back by the SFU when initiating a handshake procedure
#[derive(Deserialize)]
pub enum SFUHandshakeMessage {
	Greeting { ssl_mode: bool },
	Accepted
}
