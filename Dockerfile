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
RUN apt-get update && apt-get install -y build-essential pkg-config libssl-dev
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies
RUN cargo chef cook --release --recipe-path recipe.json

# ---- Stage 3: Builder ----
# This stage builds the actual application source code.
# It uses the cached dependencies from the previous stage.
FROM rust:1.87-slim-bookworm as builder
WORKDIR /app
COPY . .
# Copy the cached dependencies.
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
# Build the application. This will be very fast if only src/ changes.
RUN cargo build --release

# ---- Stage 4: Runtime ----
# This is the final, minimal image for production.
FROM debian:bookworm-slim as runtime

# Install runtime dependencies.
# We add `ca-certificates` for HTTPS calls and `curl` for debugging if needed.
# We also add the `sqlx-cli` to run migrations.
WORKDIR /app
RUN apt-get update && \
    apt-get install -y ca-certificates curl && \
    rm -rf /var/lib/apt/lists/* && \
    curl -L https://github.com/launchbadge/sqlx/releases/download/v0.7.4/sqlx-v0.7.4-x86_64-unknown-linux-musl.tar.gz | tar -xz && \
    mv sqlx-v0.7.4-x86_64-unknown-linux-musl/sqlx /usr/local/bin/

# Copy the compiled binary and required files.
COPY --from=builder /app/target/release/tiny-bank-server /usr/local/bin/
COPY config/default.toml /app/config/default.toml
COPY migrations /app/migrations

# Expose the application port.
EXPOSE 3000

# The command to run when the container starts.
# It first runs database migrations and then starts the server.
CMD ["/bin/sh", "-c", "sqlx migrate run && tiny-bank-server"]
