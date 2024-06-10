use warp::filters::ws::Message;

// Convenience enum for pattern matching a warp Message representing a websocket message
pub enum WsMessageKind {
	Ping(Vec<u8>),
	Pong(Vec<u8>),
	Text(String),
	Binary(Vec<u8>),
	Close(Option<(u16, String)>),
}

impl TryFrom<Message> for WsMessageKind {
	type Error = Box<dyn std::error::Error + Send + Sync>;
	fn try_from(value: Message) -> Result<Self, Self::Error> {
		if value.is_ping() 			{ Ok(WsMessageKind::Ping(value.into_bytes())) }
		else if value.is_pong() 	{ Ok(WsMessageKind::Pong(value.into_bytes())) }
		else if value.is_text()		{ Ok(WsMessageKind::Text(String::from_utf8_lossy(value.as_bytes()).into())) }
		else if value.is_binary()	{ Ok(WsMessageKind::Binary(value.into_bytes())) }
		else if value.is_close()	{ Ok(WsMessageKind::Close( value.close_frame().map(|t| (t.0, String::from(t.1))))) }
		else { Err("Failed to convert warp::filters::ws::Message into a WsMessageKind".into()) }
	}
}
