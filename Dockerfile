FROM rust:1.92.0-slim AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations

RUN cargo build --release

FROM debian:bookworm-slim AS runtime

WORKDIR /app

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

COPY --from=builder /app/target/release/webshelf /app/webshelf
COPY migrations ./migrations
COPY config.toml.example ./config.toml.example

EXPOSE 3000

ENV RUST_LOG=info
ENV DATABASE_URL=postgres://postgres:password@postgres:5432/webshelf
ENV REDIS_URL=redis://redis:6379

CMD ["/app/webshelf"]
