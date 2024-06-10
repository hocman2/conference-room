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
use std::sync::Arc;
use parking_lot::Mutex;

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

struct Inner {
	transports: Transports,
	room: Room,
	client_rtp_capabilities: Mutex<Option<RtpCapabilities>>,
}

#[derive(Clone)]
pub struct ParticipantConnection {
	id: ParticipantId,
	inner: Arc<Inner>
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
			inner: Arc::new(
				Inner {
					transports: Transports {
						consumer,
						producer
					},
					room,
					client_rtp_capabilities: Mutex::new(None),
				}
			)
		})
	}

	pub async fn run(&self, websocket: WebSocket) {
		println!("New participant {:?} in room {:?}", self.id, self.inner.room.id());

		let (ws_tx, ws_rx) = websocket.split();
		let (ch_tx, ch_rx) = mpsc::unbounded_channel::<Message>();

		// The task that'll handle web socket messages
		{
			let ch_tx = ch_tx.clone();
			let conn = self.clone();
			tokio::spawn(async move {
				if let Err(e) = conn.receive_ws_messages(ws_rx, ch_tx).await {
					eprintln!("Error sending message through the channel: {e}");
				}
			});
		}

		// Send a server ready message to the client
		{
			let ch_tx = ch_tx.clone();

			self.send_server_init(ch_tx);
		}

		// This is what blocks the "run" function
		// It receives various internal messages and handles them
		self.handle_channel_endpoint(ws_tx, ch_rx).await;
	}

	fn send_server_init(&self, ch_tx: UnboundedSender<Message>) {

		let (room_id, router, transports) = (
			self.inner.room.id().clone(),
			self.inner.room.router(),
			&self.inner.transports
		);

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
		&self,
		mut ws_rx: SplitStream<WebSocket>,
		ch_tx: UnboundedSender<Message>,
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
				WsMessageKind::Pong(_) => println!("Received pong from participant {:?}", self.id),
				WsMessageKind::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
					Ok(client_msg) => self.handle_client_message(client_msg).await,
					Err(e) => eprintln!("Failed to parse JSON into a valid ClientMessage: {e}")
				},
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
				_ => unimplemented!()
			}
		}

		Ok(())
	}

	async fn handle_client_message(&self, msg: ClientMessage) {
		match msg {
		    ClientMessage::Init { rtp_capabilities } => {
				self.inner.client_rtp_capabilities.lock().replace(rtp_capabilities);
			},
		    ClientMessage::ConnectProducerTransport { dtls_parameters } => {
				let producer_transport = self.inner.transports.producer.clone();

				if let Err(e) = producer_transport.connect(WebRtcTransportRemoteParameters {
					dtls_parameters
				}).await {
					eprintln!("Failed to connect producer transport for {:?}: {e}", self.id);
				}

				todo!("Send back a connected producer transport message to the client")
			},
		    ClientMessage::ConnectConsumerTransport { dtls_parameters } => {
				let consumer_transport = self.inner.transports.consumer.clone();

				if let Err(e) = consumer_transport.connect(WebRtcTransportRemoteParameters {
					dtls_parameters
				}).await {
					eprintln!("Failed to connect consumer transport for {:?}: {e}", self.id)
				}

				todo!("Send back a connected consumer transport message to the client");
			},
		    ClientMessage::Produce { kind, rtp_parameters } => {
				let producer_transport = self.inner.transports.producer.clone();
				match producer_transport.produce(ProducerOptions::new(kind, rtp_parameters)).await {
					Ok(producer) => {
						println!("Created {:?} producer for {:?}", kind, self.id);
						todo!("Save producer so it doesn't disappear");
						todo!("Warn the existing participants that a new producer was added")
					},
					Err(e) => {
						eprintln!("Failed to produce {:?} for {:?}: {e}", kind, self.id);
						todo!("Warn the client ? maybe");
					}
				}
			},
		    ClientMessage::Consume { producer_id } => {
				let consumer_transport = self.inner.transports.consumer.clone();
				let client_rtp_capabilities = self.inner.client_rtp_capabilities.lock().clone();
				match client_rtp_capabilities {
					Some(rtp_capabilities) => {
						match consumer_transport.consume(ConsumerOptions::new(producer_id, rtp_capabilities)).await {
							Ok(consumer) => {
								println!("{producer_id} is now being consumed by participant {:?}", self.id);
								todo!("Save consumer so it doesn't disappear");
								todo!("Maybe something else");
							},
							Err(e) => {
								eprintln!("Failed to consume {producer_id} for participant {:?}: {e}", self.id);
								todo!("warn the client maybe");
							}
						}
					},
					None =>{
						println!("Client tried to consume but didn't send their RTP capabilities first.");
						todo!("Send a warning back to the client explaining why it failed");
					}
				}

			},
		}
	}

	async fn handle_channel_endpoint(&self, mut ws_tx: SplitSink<WebSocket, ws::Message>, mut ch_rx: UnboundedReceiver<Message>) {
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
