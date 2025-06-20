# ---- Stage 1: Build the application ----
# Use the official Rust image as a build environment.
FROM rust:1.87-slim-bookworm as builder

# FIX: Add `curl` to the list of installed packages.
# This is required by the utoipa-swagger-ui crate's build script.
RUN apt-get update && apt-get install -y build-essential pkg-config libssl-dev curl

# Set the working directory
WORKDIR /usr/src/app

# Copy dependencies first to leverage Docker layer caching.
COPY Cargo.toml Cargo.lock ./
# Create a dummy src/main.rs to build only dependencies.
RUN mkdir src && \
    echo "fn main() {println!(\"if you see this, the build broke\")}" > src/main.rs && \
    cargo build --release && \
    rm -f src/main.rs target/release/deps/tiny_bank_server*

# Now copy the full source code.
COPY . .

# Build the application for release.
RUN cargo build --release

# ---- Stage 2: Create the final, small runtime image ----
# Use a minimal base image for a small and secure final container.
FROM debian:bookworm-slim

# Install runtime dependencies (like SSL certificates).
RUN apt-get update && apt-get install -y ca-certificates

# Copy the compiled binary from the builder stage.
COPY --from=builder /usr/src/app/target/release/tiny-bank-server /usr/local/bin/tiny-bank-server

# Copy the configuration file.
COPY config/default.toml /app/config/default.toml

# Set the working directory for the runtime container.
WORKDIR /app

# Expose the port the server listens on.
EXPOSE 3000

# Set the command to run when the container starts.
CMD ["tiny-bank-server"]
