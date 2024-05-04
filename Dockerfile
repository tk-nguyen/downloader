FROM rust:1 AS builder

WORKDIR /app
COPY src Cargo.toml Cargo.lock ./
RUN cargo build --release

FROM gcr.io/distroless/cc

COPY --from=builder /app/target/release/downloader /usr/local/bin/downloader
CMD ["/usr/local/bin/downloader"]
