FROM rust:latest AS builder

WORKDIR /app
COPY . .
RUN cargo build --release 

FROM gcr.io/distroless/cc:latest
#FROM gcr.io/distroless/cc:debug
#FROM debian:bookworm-slim
COPY --from=builder /app/target/release/cephylas /
CMD ["/cephylas"]

