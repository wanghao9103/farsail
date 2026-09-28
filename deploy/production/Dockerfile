# syntax=docker/dockerfile:1
FROM rust:1.93.0-bookworm AS coordinator-build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/src/target \
    cargo build --locked --release -p farsail-coordinator && cp target/release/farsail-coordinator /coordinator

FROM rust:1.93.0-bookworm AS relay-build
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/tmp/relay-target \
    CARGO_TARGET_DIR=/tmp/relay-target cargo install --locked iroh-relay --version 1.2.0 --features server --root /relay

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
USER 10001:10001

FROM runtime AS coordinator
ARG REVISION=local
LABEL org.opencontainers.image.source="https://github.com/wanghao9103/farsail" org.opencontainers.image.revision=$REVISION
COPY --from=coordinator-build /coordinator /usr/local/bin/farsail-coordinator
ENTRYPOINT ["farsail-coordinator"]

FROM runtime AS relay
ARG REVISION=local
LABEL org.opencontainers.image.source="https://github.com/wanghao9103/farsail" org.opencontainers.image.revision=$REVISION
COPY --from=relay-build /relay/bin/iroh-relay /usr/local/bin/iroh-relay
ENTRYPOINT ["iroh-relay"]
