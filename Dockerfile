# syntax=docker/dockerfile:1

FROM debian:bookworm-slim AS downloader

ARG SIMULATION_INSTALLER_URL=https://github.com/Dost-2-0-FK/simulation/releases/latest/download/simulation-installer.sh

ADD --chmod=0755 ${SIMULATION_INSTALLER_URL} /tmp/simulation-installer.sh

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl xz-utils \
    && /tmp/simulation-installer.sh

FROM debian:bookworm-slim

WORKDIR /app
COPY --from=downloader /root/.cargo/bin/simulation /usr/local/bin/simulation
COPY simulation.toml /app/simulation.toml
COPY docker-simulation-entrypoint.sh /usr/local/bin/simulation-entrypoint

RUN apt-get update \
    && apt-get install -y --no-install-recommends bash ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && sed -i 's|bind_address = "127.0.0.1"|bind_address = "0.0.0.0"|' /app/simulation.toml \
    && sed -i 's|uri = "mongodb://localhost:27017"|uri = "mongodb://mongodb:27017"|' /app/simulation.toml \
    && sed -i 's|credit_exchange_url = ".*"|credit_exchange_url = "http://credit-exchanger:18080"|' /app/simulation.toml \
    && chmod +x /usr/local/bin/simulation-entrypoint

EXPOSE 8080

ENTRYPOINT ["simulation-entrypoint"]
