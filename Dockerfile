FROM --platform=linux/amd64 ubuntu
RUN apt-get -y update && apt-get install -y build-essential curl python3-pip
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
WORKDIR /app
COPY ./src /app/src/
COPY ./Cargo.toml ./Cargo.lock /app/
CMD ["/root/.cargo/bin/cargo","build","--bin","sfu", "--release"]
