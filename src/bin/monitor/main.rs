use confroom_server::MONITORING_SFU_PORT;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::net::{Ipv4Addr, SocketAddrV4};

use clap::Parser;

/// CLI Args for this brother
#[derive(Parser, Debug)]
struct Args {
	#[arg(short='r', long="remote", value_name="IPV4 Address")]
	/// The SFU server's IPv4 address, port unspecified
	sfu_addr: Option<Ipv4Addr>,
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
	let mut sfu_stream = TcpStream::connect(sfu_addr).await.unwrap();

	// Send an empty packet to the server so it knows it's being monitored
	// In the future, we should send some identification information here so the server doesn't accept anyone
	sfu_stream.write("".as_bytes()).await.unwrap();

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
