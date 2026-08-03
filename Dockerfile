# Build stage
FROM rust:1.93-alpine AS builder

# musl-libc statically links; needed for a standalone binary
RUN apk add --no-cache musl-dev build-base
WORKDIR /build

# Cache dependencies
COPY rust/Cargo.toml rust/Cargo.lock ./
COPY rust/crates ./crates

# Build
RUN cargo build --release --bin ddns-rs

# Runtime stage
FROM alpine
LABEL name=ddns-rs
LABEL url=https://github.com/jeessy2/ddns-rs
RUN apk add --no-cache curl grep

WORKDIR /app
COPY --from=builder /build/target/release/ddns-rs /app/
ENV TZ=Asia/Shanghai
EXPOSE 9876
ENTRYPOINT ["/app/ddns-rs"]
CMD ["-l", ":9876", "-f", "300"]
