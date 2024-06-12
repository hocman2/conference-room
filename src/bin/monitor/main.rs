use confroom_server::monitoring_handshake::*;
use confroom_server::MONITORING_SFU_PORT;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use std::time::Duration;
use tokio::net::TcpStream;
use std::net::{Ipv4Addr, SocketAddrV4};
use clap::Parser;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// CLI Args for this brother
#[derive(Parser, Debug)]
struct Args {
	#[arg(short='r', long="remote", value_name="IPV4 Address")]
	/// The SFU server's IPv4 address, port unspecified
	sfu_addr: Option<Ipv4Addr>,
}

async fn initiate_handshake(sfu_addr: SocketAddrV4) -> Result<TcpStream, HandshakeError> {
	// This needs better error handling
	let mut sfu_stream = TcpStream::from_std(
		std::net::TcpStream::connect_timeout(&sfu_addr.into(), CONNECT_TIMEOUT)?
	)?;

	let msg_bytes = bincode::serialize(
		&MonitorHandshakeMessage::Greeting { greet: "Greetings" }
	)?;

	sfu_stream.write(&msg_bytes).await?;

	println!("Send greetings");

	let mut response = [0; 1024];
	let read_bytes = sfu_stream.read(&mut response).await?;
	let response = bincode::deserialize::<SFUHandshakeMessage>(&response[0..read_bytes])?;

	if let SFUHandshakeMessage::Greeting { ssl_mode } = response {
		if ssl_mode {
			unimplemented!("Secure mode unimplemented yet");
		}
	} else {
		return Err("Expected greetings message but received something else".to_string().into());
	}

	Ok(sfu_stream)
}

#[tokio::main]
async fn main() {
	let args = Args::parse();

	let sfu_addr = match args.sfu_addr {
		Some(addr) => addr,
		None => {
			println!("No address specified, using localhost");
			Ipv4Addr::LOCALHOST
		}
	};

	let sfu_addr = SocketAddrV4::new(sfu_addr, MONITORING_SFU_PORT);
	let mut sfu_stream = match initiate_handshake(sfu_addr).await {
		Ok(stream) => stream,
		Err(e) => panic!("{e}")
	};

	loop {
		let mut buff = [0;1024];

		let bytes_read = match sfu_stream.read(&mut buff).await {
			Ok(n) => n,
			Err(e) => {
				eprintln!("Error reading from TCP Stream: {e}");
				break;
			}
		};

		let msg = String::from_utf8_lossy(&buff[0..bytes_read]);
		println!("{msg}");
	}
}
