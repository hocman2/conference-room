use crate::{participant::TransportOptions, room::RoomId, websocket::WsMessageKind};
use mediasoup::prelude::*;
use serde::Serialize;

/// Internal server messages to facilitate interactions between tasks
/// These won't be sent to the client
pub enum Internal {
	Close
}

/// Message types intended to be sent to the client
#[derive(Serialize)]
pub enum ServerMessage {
	Init {
		room_id: RoomId,
		router_rtp_capabilities: RtpCapabilitiesFinalized,
		consumer_transport_options: TransportOptions,
		producer_transport_options: TransportOptions,
	},
}

/// Different types of messages that can be sent by the server
pub enum Message {
	WebSocket(WsMessageKind),
	Internal(Internal),
	Server(ServerMessage),
}

impl Into<Message> for Internal {
	fn into(self) -> Message {
		Message::Internal(self)
	}
}

impl Into<Message> for WsMessageKind {
	fn into(self) -> Message {
		Message::WebSocket(self)
	}
}

impl Into<Message> for ServerMessage {
	fn into(self) -> Message {
		Message::Server(self)
	}
}
