FROM rust:1 AS builder

COPY src /app/src
COPY Cargo.* /app
WORKDIR /app
RUN cargo build --release

FROM gcr.io/distroless/cc

COPY --from=builder /app/target/release/downloader /usr/local/bin/downloader
CMD ["/usr/local/bin/downloader"]
