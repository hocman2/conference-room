extern crate confroom_server as server;

use std::collections::HashMap;

use confroom_server::monitoring::SFUEvent;
use confroom_server::uuids::ParticipantId;
use event_listener_primitives::HandlerId;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{error::SendError, UnboundedReceiver, UnboundedSender};
use mediasoup::prelude::*;
use warp::ws::WebSocket;
use warp::filters::ws;
use std::sync::Arc;
use parking_lot::Mutex;

use futures_util::{stream::{SplitSink, SplitStream}, StreamExt, SinkExt};

use server::websocket::WsMessageKind;
use crate::monitor_dispatch::MonitorDispatch;
use crate::room::Room;
use crate::message::*;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
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
	id: ParticipantId,
	transports: Transports,
	room: Room,
	client_rtp_capabilities: Mutex<Option<RtpCapabilities>>,
	consumers: Mutex<HashMap<ConsumerId, Consumer>>,
	producers: Mutex<Vec<Producer>>,
	attached_handlers: Mutex<Vec<HandlerId>>
}

#[derive(Clone)]
pub struct ParticipantConnection {
	inner: Arc<Inner>
}

impl ParticipantConnection {
	pub async fn new(room: Room) -> Result<Self, Error> {

		let router = room.router();
		let server = room.webrtc_server();

		let transport_opts = WebRtcTransportOptions::new_with_server(server.to_owned());

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
			inner: Arc::new(
				Inner {
					id: ParticipantId::new(),
					transports: Transports {
						consumer,
						producer
					},
					room,
					client_rtp_capabilities: Mutex::new(None),
					consumers: Mutex::new(HashMap::new()),
					producers: Mutex::new(Vec::new()),
					attached_handlers: Mutex::new(Vec::new()),
				}
			)
		})
	}

	pub async fn run(&self, websocket: WebSocket) {
		log::info!("New participant {} in room {}", self.inner.id, self.inner.room.id());
		let _ = MonitorDispatch::send_event(SFUEvent::ParticipantEntered {
			room_id: self.inner.room.id(),
			participant_id: self.inner.id.clone(),
		});

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
			self.init_connection(ch_tx);
		}

		// This is what blocks the "run" function
		// It receives various internal messages and handles them
		self.handle_channel_endpoint(ws_tx, ch_rx).await;
	}

	/// Prepares the connection and sends a ServerMessage::Init when done
	fn init_connection(&self, ch_tx: UnboundedSender<Message>) {
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

		if let Err(e) = ch_tx.send(server_init.into()) {
			eprintln!("Failed to send message through the channel: {e}");
		}

		let room = self.inner.room.clone();
		{
			let mut attached_handlers = self.inner.attached_handlers.lock();

			attached_handlers.push(room.on_producer_add({
				let ch_tx = ch_tx.clone();
				let own_id = self.inner.id.clone();

				move |participant_id, producer| {
					if *participant_id == own_id { return; }

					let result = ch_tx.send(ServerMessage::ProducerAdded {
						participant_id: participant_id.to_owned(),
						producer_id: producer.id().clone()
					}.into());

					if let Err(e) = result {
						eprintln!("Failed to send message through the channel: {e}");
					}
				}
			}));

			attached_handlers.push(room.on_producer_remove({
				let ch_tx = ch_tx.clone();
				let own_id = self.inner.id.clone();

				move |participant_id, producer_id| {
					if *participant_id == own_id { return; }

					let result = ch_tx.send(ServerMessage::ProducerRemoved {
						participant_id: participant_id.to_owned(),
						producer_id: producer_id.to_owned()
					}.into());

					if let Err(e) = result {
						eprintln!("Failed to send message through the channel: {e}");
					}
				}
			}));

			attached_handlers.push(room.on_fatal_error({
				let ch_tx = ch_tx.clone();
				move || {
					let _ = ch_tx.send(Internal::Close.into());
				}
			}));
		}

		for (participant_id, producer_id) in room.get_all_producers() {
			let result = ch_tx.send(ServerMessage::ProducerAdded {
				participant_id,
				producer_id,
			}.into());

			// This is getting repetitive ...
			if let Err(e) = result {
				eprintln!("Failed to send message through the channel: {e}");
			}
		}
	}

	async fn receive_ws_messages(
		&self,
		mut ws_rx: SplitStream<WebSocket>,
		ch_tx: UnboundedSender<Message>,
	) -> std::result::Result<(), SendError<Message>> {
		// Cut connection when there are too many errors
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
				WsMessageKind::Pong(_) => log::info!("Received pong from participant {}", self.inner.id),
				WsMessageKind::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
					Ok(client_msg) => self.handle_client_message(client_msg, ch_tx.clone()).await?,
					Err(e) => eprintln!("Failed to parse JSON into a valid ClientMessage: {e}")
				},
				WsMessageKind::Binary(bin) => {
					// Not a critical error, just warn the client
					log::warn!("Received binary message: {:?}. The server does not handle binary messages", bin);
					ch_tx.send(WsMessageKind::Text(
						"Received binary data. Binary messages are not handled by the server. Please provide data in an expected JSON format".into())
					.into())?;
				},
				WsMessageKind::Close(_) => {
					ch_tx.send(Internal::Close.into())?;
					break;
				},
			}
		}

		Ok(())
	}

	async fn handle_client_message(&self, msg: ClientMessage, ch_tx: UnboundedSender<Message>) -> std::result::Result<(), SendError<Message>> {
		match msg {
		    ClientMessage::Init { rtp_capabilities } => {
				self.inner.client_rtp_capabilities.lock().replace(rtp_capabilities);
				Ok(())
			},
		    ClientMessage::ConnectProducerTransport { dtls_parameters } => {
				let producer_transport = self.inner.transports.producer.clone();

				if let Err(e) = producer_transport.connect(WebRtcTransportRemoteParameters {
					dtls_parameters
				}).await {
					eprintln!("Failed to connect producer transport for {:?}: {e}", self.inner.id);
				}

				ch_tx.send(ServerMessage::ConnectedProducerTransport.into())
			},
		    ClientMessage::ConnectConsumerTransport { dtls_parameters } => {
				let consumer_transport = self.inner.transports.consumer.clone();

				if let Err(e) = consumer_transport.connect(WebRtcTransportRemoteParameters { dtls_parameters }).await {
					eprintln!("Failed to connect consumer transport for {:?}: {e}", self.inner.id)
				}

				ch_tx.send(ServerMessage::ConnectedConsumerTransport.into())
			},
		    ClientMessage::Produce { kind, rtp_parameters } => {
				let producer_transport = self.inner.transports.producer.clone();
				match producer_transport.produce(ProducerOptions::new(kind, rtp_parameters)).await {
					Ok(producer) => {
						log::info!("Created {:?} producer for {:?}", kind, self.inner.id);
						self.inner.room.add_producer(self.inner.id.clone(), producer.clone());
						ch_tx.send(ServerMessage::Produced{id: producer.id().clone()}.into())?;
						self.inner.producers.lock().push(producer);
						Ok(())
					},
					Err(e) => {
						eprintln!("Failed to produce {:?} for {:?}: {e}", kind, self.inner.id);
						ch_tx.send(ServerMessage::Warning{message: "Failed to produce, an unexpected error occured.".into()}.into())
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
								log::info!("{producer_id} is now being consumed by participant {}", self.inner.id);
								self.inner.consumers.lock().insert(consumer.id().clone(), consumer.clone());
								ch_tx.send(ServerMessage::Consumed{
									id: consumer.id().clone(),
									kind: consumer.kind().clone(),
									rtp_parameters: consumer.rtp_parameters().clone(),
									producer_id: consumer.producer_id().clone(),
								}.into())
							},
							Err(e) => {
								eprintln!("Failed to consume {producer_id} for participant {:?}: {e}", self.inner.id);
								ch_tx.send(ServerMessage::Warning{
									message: "Failed to consume producer, an unexpected error occured.".into()
								}.into())
							}
						}
					},
					None =>{
						ch_tx.send(ServerMessage::Warning{ message:
							"You must send your RTP capabilities through an Init message before being able to consume".into()
						}.into())
					}
				}
			},
			ClientMessage::ConsumerResume {id} => {
				let consumer_maybe = {
					let consumers = self.inner.consumers.lock();
					consumers.get(&id).map(|v| v.to_owned())
				};

				match consumer_maybe {
					Some(consumer) => {
						if let Err(e) =  consumer.resume().await {
							eprintln!("Failed to resume consumer {id} for {:?}: {e}", self.inner.id);
						} else {
							log::info!("Resumed consumer {id} for {:?}", self.inner.id);
						}

						Ok(())
					},
					None => {
						ch_tx.send(ServerMessage::Warning{message: "No consumer found for the provided id !".into()}.into())
					}
				}
			}
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
						// End connection
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
					log::warn!("Error sending message: {e}\n{send_error_counter}/{MAX_ERRORS}; Keeping connection alive");
					continue;
				} else {
					log::error!("Error sending message: {e}\n{send_error_counter}/{MAX_ERRORS}; Closing connection");
					break;
				}
			}
		}
	}
}

impl Drop for ParticipantConnection {
	fn drop(&mut self) {
		log::info!("Participant {} is leaving", self.inner.id);
		self.inner.room.remove_participant(&self.inner.id);

		let _ = MonitorDispatch::send_event(SFUEvent::ParticipantLeft {
			room_id: self.inner.room.id(),
			participant_id: self.inner.id
		});
	}
}
