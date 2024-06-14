use confroom_server::monitoring::{SFUEvent, SFU_PORT};
use std::io::Read;
use std::time::Duration;
use std::net::TcpStream;
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

	let sfu_addr = SocketAddrV4::new(sfu_addr, SFU_PORT);

	let mut stream = TcpStream::connect_timeout(&sfu_addr.into(), CONNECT_TIMEOUT).unwrap();

	loop {
		let mut buf = [0; 1024];
		match stream.read(&mut buf) {
			Ok(bytes_read) => {
				match bincode::deserialize::<SFUEvent>(&buf[..bytes_read]) {
					Ok(evt) => match evt {
						SFUEvent::MonitorAccepted => println!("Received monitor accepted !"),
						_ => ()
					},
					Err(e) => eprintln!("{e}")
				}
			}
			Err(e) => eprintln!("{e}")
		}
	}
}
