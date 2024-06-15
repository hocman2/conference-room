#[derive(Debug)]
pub enum Error {
	NoDispatchRunning,
	TcpError(tokio::io::Error)
}

impl From<tokio::io::Error> for Error {
	fn from(value: tokio::io::Error) -> Self {
		Error::TcpError(value)
	}
}
