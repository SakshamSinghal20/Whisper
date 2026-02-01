FROM rust:1.75-slim as builder

WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libzmq3-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml ./
COPY whisper-core ./whisper-core
COPY whisper-server ./whisper-server
COPY whisper-client ./whisper-client

# Build release
RUN cargo build --release --bin whisper-server

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libzmq5 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/whisper-server /usr/local/bin/whisper-server

EXPOSE 3000

CMD ["whisper-server"]
