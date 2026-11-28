# The Housewright add-on image: housewright-server and housewright-bridge.
# Build from the repository root (the Rust sources must be in the context):
#   docker build -f integrations/home-assistant/addon/housewright/Dockerfile \
#     --build-arg BUILD_FROM=ghcr.io/home-assistant/amd64-base-debian:bookworm \
#     -t housewright-addon .
ARG BUILD_FROM=ghcr.io/home-assistant/amd64-base-debian:bookworm

FROM rust:1.92-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p housewright-server -p housewright-bridge

FROM ${BUILD_FROM}
RUN apt-get update \
    && apt-get install -y --no-install-recommends jq ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/housewright-server /usr/local/bin/housewright-server
COPY --from=build /src/target/release/housewright-bridge /usr/local/bin/housewright-bridge
COPY integrations/home-assistant/addon/housewright/run.sh /run.sh
RUN chmod a+x /run.sh
LABEL io.hass.type="addon" io.hass.version="0.1.0"
CMD ["/run.sh"]
