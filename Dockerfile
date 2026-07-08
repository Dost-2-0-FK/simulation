# syntax=docker/dockerfile:1

ARG RUST_VERSION=1

FROM rust:${RUST_VERSION}-bullseye AS builder

ENV CARGO_BUILD_JOBS=1

WORKDIR /build/simulation
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# The application currently binds localhost:8080 in source. For Docker port
# publishing the simulation server must listen on all container interfaces.
RUN sed -i 's|\.bind(("127\.0\.0\.1", 8080))?|.bind(("0.0.0.0", 8080))?|' src/main.rs \
    && cargo build --release --jobs 1

FROM debian:bullseye-slim

COPY --from=builder /build/simulation/target/release/simulation /usr/local/bin/simulation

WORKDIR /app
COPY simulation.toml /app/simulation.toml
COPY docker-simulation-entrypoint.sh /usr/local/bin/simulation-entrypoint

RUN apt-get update \
    && apt-get install -y --no-install-recommends bash ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && sed -i 's|uri = "mongodb://localhost:27017"|uri = "mongodb://mongodb:27017"|' /app/simulation.toml \
    && sed -i 's|credit_exchange_url = ".*"|credit_exchange_url = "http://credit-exchanger:18080"|' /app/simulation.toml \
    && chmod +x /usr/local/bin/simulation-entrypoint

EXPOSE 8080

ENTRYPOINT ["simulation-entrypoint"]
