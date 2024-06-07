mod result;

use std::net::{IpAddr, Ipv4Addr};

pub use result::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{error::SendError, UnboundedReceiver, UnboundedSender};
use uuid::Uuid;
use mediasoup::prelude::*;
use warp::ws::WebSocket;
use warp::filters::ws;

use futures_util::{stream::{SplitSink, SplitStream}, StreamExt, SinkExt};

use crate::room::RoomId;
use crate::websocket::WsMessageKind;
use crate::Room;
use crate::message::*;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Deserialize, Copy)]
pub struct ParticipantId(Uuid);
impl ParticipantId {
	fn new() -> Self {
		ParticipantId(Uuid::new_v4())
	}
}

impl std::fmt::Display for ParticipantId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(&self.0, f)
	}
}

#[derive(Serialize)]
pub struct TransportOptions {
	id: TransportId,
	dtls_parameters: DtlsParameters,
	ice_parameters: IceParameters,
	ice_candidates: Vec<IceCandidate>,
}

struct Transports {
	consumer: WebRtcTransport,
	producer: WebRtcTransport,
}

pub struct ParticipantConnection {
	id: ParticipantId,
	transports: Transports,
	room: Room,
}

impl ParticipantConnection {
	pub async fn new(room: Room) -> Result<Self> {

		let router = room.router();

		let transport_opts = WebRtcTransportOptions::new(WebRtcTransportListenInfos::new(ListenInfo {
			protocol: Protocol::Udp,
			ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
			announced_address: None,
			port: None,
			port_range: None,
			flags: None,
			send_buffer_size: None,
			recv_buffer_size: None
		}));

		let consumer = router.create_webrtc_transport(
			transport_opts.clone()
		)
		.await
		.map_err(|e| format!("Failed to create consumer transport: {e}"))?;

		let producer = router.create_webrtc_transport(
			transport_opts.clone()
		)
		.await
		.map_err(|e| format!("Failed to create producer transport: {e}"))?;

		Ok(ParticipantConnection {
			id: ParticipantId::new(),
			transports: Transports {
				consumer,
				producer
			},
			room
		})
	}

	pub async fn run(&mut self, websocket: WebSocket) {
		println!("New participant {:?} in room {:?}", self.id, self.room.id());

		let (ws_tx, ws_rx) = websocket.split();
		let (ch_tx, ch_rx) = mpsc::unbounded_channel::<Message>();

		// The task that'll handle web socket messages
		{
			let ch_tx = ch_tx.clone();
			let participant_id = self.id.clone();
			tokio::spawn(async move {
				if let Err(e) = ParticipantConnection::receive_ws_messages(ws_rx, ch_tx, participant_id).await {
					eprintln!("Error sending message through the channel: {e}");
				}
			});
		}

		// Send a server ready message to the client
		{
			let ch_tx = ch_tx.clone();
			let room_id = self.room.id().clone();
			let router = self.room.router();
			let transports = &self.transports;

			ParticipantConnection::send_server_init(
				room_id,
				router,
				transports,
			 	ch_tx
			);
		}

		// This is what blocks the "run" function
		// It receives various internal messages and handles them
		self.handle_channel_endpoint(ws_tx, ch_rx).await;
	}

	fn send_server_init(room_id: RoomId, router: &Router, transports: &Transports, ch_tx: UnboundedSender<Message>) {

		let server_init = ServerMessage::Init {
			room_id,
			router_rtp_capabilities: router.rtp_capabilities().clone(),
			consumer_transport_options: TransportOptions {
				id: transports.consumer.id().clone(),
				dtls_parameters: transports.consumer.dtls_parameters().clone(),
				ice_candidates: transports.consumer.ice_candidates().clone(),
				ice_parameters: transports.consumer.ice_parameters().clone()
			},
			producer_transport_options: TransportOptions {
				id: transports.producer.id().clone(),
				dtls_parameters: transports.producer.dtls_parameters().clone(),
				ice_candidates: transports.producer.ice_candidates().clone(),
				ice_parameters: transports.producer.ice_parameters().clone()
			},
		};

		let _ = ch_tx.send(server_init.into());
	}

	async fn receive_ws_messages(
		mut ws_rx: SplitStream<WebSocket>,
		ch_tx: UnboundedSender<Message>,
		participant_id: ParticipantId,
	) -> std::result::Result<(), SendError<Message>> {
		let mut ws_error_counter = 0;
		const MAX_ERRORS: u8 = 3;

		while let Some(msg) = ws_rx.next().await {
			let msg = match msg {
				Ok(msg) => WsMessageKind::try_from(msg).unwrap(),
				Err(e) => {
					ws_error_counter += 1;

					if ws_error_counter <= MAX_ERRORS {
						eprintln!("websocket error: {e}. {ws_error_counter}/{MAX_ERRORS} errors. Keeping connection alive");
						continue;
					} else {
						eprintln!("websocket error: {e}. {ws_error_counter}/{MAX_ERRORS} errors. Closing connection");
						ch_tx.send(Internal::Close.into())?;
						break;
					}
				}
			};

			match msg {
				WsMessageKind::Ping(data) => {
					ch_tx.send(WsMessageKind::Pong(data).into())?;
				},
				WsMessageKind::Pong(_) => println!("Received pong from participant {:?}", participant_id),
				WsMessageKind::Text(text) => todo!("Convert received text into ClientMessage struct (yet to create)"),
				WsMessageKind::Binary(bin) => {
					// Not a critical error, just warn the client
					println!("Received binary message: {:?}. The server does not handle binary messages", bin);
					ch_tx.send(WsMessageKind::Text(
						"Received binary data. Binary messages are not handled by the server. Please provide data in an expected JSON format".into())
					.into())?;
				},
				WsMessageKind::Close(_) => {
					ch_tx.send(Internal::Close.into())?;
				},
				_ => todo!()
			}
		}

		Ok(())
	}

	async fn handle_channel_endpoint(&mut self, mut ws_tx: SplitSink<WebSocket, ws::Message>, mut ch_rx: UnboundedReceiver<Message>) {
		let mut send_error_counter = 0;
		const MAX_ERRORS: u8 = 3;

		while let Some(msg) = ch_rx.recv().await {
			let result = match msg {
			    Message::WebSocket(ws_msg) => match ws_msg {
			        WsMessageKind::Ping(bytes) => ws_tx.send(ws::Message::ping(bytes)).await,
			        WsMessageKind::Pong(bytes) => ws_tx.send(ws::Message::pong(bytes)).await,
					_ => unimplemented!("Requested the server to send an unimplemented websocket message kind")
			    },
				Message::Internal(int_msg) => match int_msg {
					Internal::Close => {
						todo!("Must send some event to warn the room that this participant leaves here");
						return; /* bye bye */
					}
				},
				Message::Server(srv_msg) => {
					match serde_json::to_string(&srv_msg) {
						Ok(json_msg) => ws_tx.send(ws::Message::text(json_msg)).await,
						Err(e) => panic!("A server message couldn't be converted to JSON. This should never happen. Error: {:?}", e)
					}
				}
			};

			if let Err(e) = result {
				send_error_counter += 1;
				if send_error_counter < MAX_ERRORS {
					eprintln!("Error sending message: {e}\n{send_error_counter}/{MAX_ERRORS}; Keeping connection alive");
					continue;
				} else {
					eprintln!("Error sending message: {e}\n{send_error_counter}/{MAX_ERRORS}; Closing connection");
					break;
				}
			}
		}
	}
}
