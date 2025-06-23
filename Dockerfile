# ---- Stage 1: Planner ----
# This stage calculates the dependency tree using cargo-chef.
FROM rust:1.87-slim-bookworm as planner
WORKDIR /app
RUN cargo install cargo-chef
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---- Stage 2: Cacher ----
# This stage builds and caches only the dependencies.
# This layer will only be rebuilt if Cargo.toml or Cargo.lock changes.
FROM rust:1.87-slim-bookworm as cacher
WORKDIR /app
RUN cargo install cargo-chef
# Install build dependencies needed by some crates.
RUN apt-get update && apt-get install -y build-essential pkg-config libssl-dev curl
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies
RUN cargo chef cook --release --recipe-path recipe.json

# ---- Stage 3: Builder ----
# This stage builds the actual application source code.
# It uses the cached dependencies from the previous stage.
FROM rust:1.87-slim-bookworm as builder
WORKDIR /app
# FIX: Install the linker dependencies in the builder stage as well.
RUN apt-get update && apt-get install -y build-essential pkg-config libssl-dev
COPY . .
# Copy the cached dependencies.
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
# Build the application. This will be very fast if only src/ changes.
RUN cargo build --release

# ---- Stage 4: Runtime ----
# This is the final, minimal image for production.
FROM debian:bookworm-slim as runtime

# Install only the necessary runtime dependencies.
# `ca-certificates` is needed for making secure HTTPS calls.
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy the compiled binary and the configuration file.
# The migrations are now embedded in the binary, so we don't need to copy them.
COPY --from=builder /app/target/release/tiny-bank-server /usr/local/bin/
COPY config/default.toml /app/config/default.toml

WORKDIR /app

# Expose the application port.
EXPOSE 3000

# The command to run when the container starts.
# The application now handles its own migrations.
CMD ["tiny-bank-server"]
