# =============================================================================
# Stage 1: Builder — Compile a fully static Rust binary using the MUSL target
# =============================================================================
# Using rust:1.77-slim-bookworm gives us a recent Rust toolchain on a slim
# Debian Bookworm base. We install musl-tools so we can link against musl libc
# and produce a truly static binary (no dynamic glibc dependency at runtime).
FROM rust:1.77-slim-bookworm AS builder

WORKDIR /app

# Install musl-tools which provides the linker and C library needed for the
# x86_64-unknown-linux-musl target, then add that target via rustup.
RUN apt-get update && \
    apt-get install -y musl-tools && \
    rm -rf /var/lib/apt/lists/* && \
    rustup target add x86_64-unknown-linux-musl

# Copy dependency manifests first so Docker can cache the dependency layer.
# This avoids re-downloading and recompiling crates when only source code changes.
COPY Cargo.toml Cargo.lock ./

# Create a dummy source file so cargo can resolve and build dependencies
# without needing the real application code yet. This is the key caching trick:
# the dependencies are compiled once and cached in the Docker layer.
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies only. The --release flag ensures we get optimized deps,
# and the musl target produces a static binary.
RUN cargo build --release --target x86_64-unknown-linux-musl

# Now copy the real source code, overwriting the dummy main.rs.
COPY src ./src

# Touch main.rs to invalidate its cached fingerprint so cargo rebuilds the
# application binary (but reuses the already-compiled dependencies).
RUN touch src/main.rs

# Build the final release binary, statically linked via musl.
RUN cargo build --release --target x86_64-unknown-linux-musl

# =============================================================================
# Stage 2: Runtime — Minimal Alpine image with the static binary
# =============================================================================
# Alpine is ideal for static Rust binaries: it's tiny (~5 MB) and since our
# binary is fully statically linked we don't need any system libraries.
FROM alpine:3.19

# Install CA certificates so the application can make HTTPS/TLS connections
# to external services (ClickHouse, MQTT broker, etc.).
RUN apk add --no-cache ca-certificates

WORKDIR /app

# Copy the statically-linked binary from the builder stage.
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/shuidui-backend /app/shuidui-backend

# Copy configuration files that the application reads at startup.
COPY config/ ./config/

# Expose the API port.
EXPOSE 8080

# Default environment variables — these can be overridden at runtime via
# docker run -e or docker-compose environment directives.
ENV CLICKHOUSE_URL=http://clickhouse:8123
ENV CLICKHOUSE_DB=shuidui
ENV MQTT_BROKER=mosquitto
ENV MQTT_PORT=1883
ENV API_PORT=8080
ENV RUST_LOG=info

CMD ["/app/shuidui-backend"]
