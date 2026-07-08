FROM rust:slim-bookworm AS builder

# Install all common C-compilation tools and dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev build-essential cmake && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

# Copy the entire workspace into the container
COPY . .

# Build the main server binary
WORKDIR /usr/src/app/backend
RUN cargo build --release -p astra-server

# Create the minimal runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/app/backend/target/release/astra-server /usr/local/bin/

# Set default environment variables
ENV ASTRA_HOST=0.0.0.0
ENV ASTRA_PORT=8080
ENV ASTRA_DATA_DIR=/app/data

EXPOSE 8080
RUN mkdir -p /app/data

CMD ["astra-server"]
