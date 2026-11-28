# Build:  docker build -f crates/server/deploy/Dockerfile -t housewright-server .
# Run:    docker run -d -p 127.0.0.1:8787:8787 -v hw-data:/data housewright-server
FROM rust:1.92-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p housewright-server

FROM debian:bookworm-slim
RUN useradd --system --home /data housewright && mkdir /data && chown housewright /data
COPY --from=build /src/target/release/housewright-server /usr/local/bin/housewright-server
USER housewright
ENV HW_LISTEN=0.0.0.0:8787 HW_DATA_DIR=/data HW_BEHIND_PROXY=true
VOLUME /data
EXPOSE 8787
CMD ["housewright-server"]
