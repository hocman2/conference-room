use warp::filters::ws::Message;

// Convenience enum for pattern matching a warp Message representing a websocket message
pub enum WsMessageKind {
	Ping,
	Pong,
	Text(String),
	Binary(Vec<u8>),
	Close(Option<(u16, String)>),
	// Consider an error if this is the match case
	Unexpected(Vec<u8>)
}

impl From<Message> for WsMessageKind {
	fn from(value: Message) -> Self {
		if value.is_ping() 			{ WsMessageKind::Ping }
		else if value.is_pong() 	{ WsMessageKind::Pong }
		else if value.is_text()		{ WsMessageKind::Text(String::from_utf8_lossy(value.as_bytes()).into()) }
		else if value.is_binary()	{ WsMessageKind::Binary(value.into_bytes()) }
		else if value.is_close()	{ WsMessageKind::Close( value.close_frame().map(|t| (t.0, String::from(t.1))) )}
		// Shouldn't happen
		else { WsMessageKind::Unexpected(value.into_bytes()) }
	}
}
