# --- Build Stage ---
FROM docker.io/library/rust:1.85-slim-bookworm AS builder

# Install protobuf compiler (protoc) needed for Tonic/gRPC compilation
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the build configuration and source code
COPY Cargo.toml ./
COPY build.rs ./
COPY proto/ ./proto/
COPY src/ ./src/

# Compile the release binary
RUN cargo build --release

# --- Run Stage ---
FROM docker.io/library/debian:bookworm-slim

# Install basic runtime dependencies (CA certificates)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/release/scim /app/scim

# Copy the default configuration
COPY config/config.toml /app/config/config.toml

# Expose the HTTP REST port (8080) and the gRPC control port (50051)
EXPOSE 8080
EXPOSE 50051

# Configure container execution
ENTRYPOINT ["/app/scim"]
CMD ["server", "--config", "config/config.toml"]
