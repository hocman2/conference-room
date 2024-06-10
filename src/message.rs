use crate::{participant::{ParticipantId, TransportOptions}, room::RoomId, websocket::WsMessageKind};
use mediasoup::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(tag = "action")]
pub enum ClientMessage {
	#[serde(rename_all="camelCase")]
	Init {
		rtp_capabilities: RtpCapabilities,
	},
	ConnectProducerTransport {
		dtls_parameters: DtlsParameters
	},
	#[serde(rename_all="camelCase")]
	ConnectConsumerTransport {
		dtls_parameters: DtlsParameters
	},
	#[serde(rename_all="camelCase")]
	Produce {
		kind: MediaKind,
		rtp_parameters: RtpParameters
	},
	#[serde(rename_all="camelCase")]
	Consume {
		producer_id: ProducerId,
	},
	#[serde(rename_all="camelCase")]
	ConsumerResume{id: ConsumerId},
}

/// Internal server messages to facilitate interactions between tasks
/// These won't be sent to the client
pub enum Internal {
	Close
}

/// Message types intended to be sent to the client
#[derive(Serialize)]
#[serde(tag = "action")]
pub enum ServerMessage {
	#[serde(rename_all="camelCase")]
	Init {
		room_id: RoomId,
		router_rtp_capabilities: RtpCapabilitiesFinalized,
		consumer_transport_options: TransportOptions,
		producer_transport_options: TransportOptions,
	},
	ConnectedConsumerTransport,
	ConnectedProducerTransport,
	#[serde(rename_all="camelCase")]
	ProducerAdded { participant_id: ParticipantId, producer_id: ProducerId },
	#[serde(rename_all="camelCase")]
	ProducerRemoved{ participant_id: ParticipantId, producer_id: ProducerId },
	#[serde(rename_all="camelCase")]
	Produced{id: ProducerId},
	#[serde(rename_all="camelCase")]
	Consumed{id: ConsumerId},
	#[serde(rename_all="camelCase")]
	Warning{message: String},
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
