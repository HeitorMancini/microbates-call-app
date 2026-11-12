# syntax=docker/dockerfile:1
# Imagem do token-server (build a partir da raiz do repositório):
#   docker build -f crates/token-server/Dockerfile -t sheeshcord-token-server .
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p token-server \
    && cp target/release/token-server /usr/local/bin/token-server

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /usr/local/bin/token-server /token-server
ENV TOKEN_SERVER_BIND=0.0.0.0:7890 \
    RUST_LOG=info,tower_http=info
EXPOSE 7890
USER nonroot
ENTRYPOINT ["/token-server"]
