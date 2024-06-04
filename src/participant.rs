mod error;
pub use error::{Error, Result};

use crate::Room;
use warp::ws::WebSocket;
use crate::websocket::WsMessageKind;
use futures_util::{SinkExt, StreamExt};

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
