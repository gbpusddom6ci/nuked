# syntax=docker/dockerfile:1

FROM rust:1.91-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/nuked /app/nuked

EXPOSE 3000

CMD ["./nuked"]
