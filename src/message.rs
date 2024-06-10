use crate::{participant::{ParticipantId, TransportOptions}, room::RoomId, websocket::WsMessageKind};
use mediasoup::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub enum ClientMessage {
	Init {
		rtp_capabilities: RtpCapabilities,
	},
	ConnectProducerTransport {
		dtls_parameters: DtlsParameters
	},
	ConnectConsumerTransport {
		dtls_parameters: DtlsParameters
	},
	Produce {
		kind: MediaKind,
		rtp_parameters: RtpParameters
	},
	Consume {
		producer_id: ProducerId,
	},
	ConsumerResume(ConsumerId),
}

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
	ConnectedConsumerTransport,
	ConnectedProducerTransport,
	ProducerAdded { participant_id: ParticipantId, producer_id: ProducerId },
	ProducerRemoved{ participant_id: ParticipantId, producer_id: ProducerId },
	Produced(ProducerId),
	Consumed(ConsumerId),
	Warning(String),
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
