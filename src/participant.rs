mod result;
pub use result::Result;
use serde::Deserialize;
use uuid::Uuid;

use crate::Room;
use warp::ws::WebSocket;
use crate::websocket::WsMessageKind;
use futures_util::{SinkExt, StreamExt};

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

pub struct ParticipantConnection;
impl ParticipantConnection {
	pub fn new(room: Room) -> Result<Self> {
		Ok(ParticipantConnection)
	}

	pub async fn run(&mut self, websocket: WebSocket) {
		let (mut ws_tx, mut ws_rx) = websocket.split();

		while let Some(msg) = ws_rx.next().await {
			let msg = match msg {
				Ok(msg) => WsMessageKind::from(msg),
				Err(e) => {
					eprintln!("websocket error: {e}");
					break;
				}
			};

			match msg {
				_ => todo!()
			}
		}
	}
}
