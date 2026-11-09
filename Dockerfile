# syntax=docker/dockerfile:1.7
FROM rust:1.99-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libfontconfig1-dev libxkbcommon-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/src/target \
    --mount=type=secret,id=proxy_ca,required=false \
    if [ -s /run/secrets/proxy_ca ]; then \
      cat /etc/ssl/certs/ca-certificates.crt /run/secrets/proxy_ca > /tmp/build-ca.pem; \
      export CARGO_HTTP_CAINFO=/tmp/build-ca.pem; \
    fi; \
    cargo build --release --locked -p vrcx-0-web && cp target/release/vrcx-0-web /usr/local/bin/vrcx-0-web

FROM node:26-bookworm-slim AS frontend
WORKDIR /src
COPY package.json package-lock.json ./
RUN --mount=type=secret,id=proxy_ca,required=false \
    if [ -s /run/secrets/proxy_ca ]; then export NODE_EXTRA_CA_CERTS=/run/secrets/proxy_ca; fi; \
    npm ci --ignore-scripts --no-audit --no-fund
COPY . .
RUN npx vite build --mode web
# The license generator reads cargo metadata and cached crate license files.
COPY --from=build /usr/local/cargo /usr/local/cargo
COPY --from=build /usr/local/rustup /usr/local/rustup
ENV PATH="/usr/local/cargo/bin:${PATH}" RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo
RUN --mount=type=secret,id=proxy_ca,required=false \
    if [ -s /run/secrets/proxy_ca ]; then \
      cat /etc/ssl/certs/ca-certificates.crt /run/secrets/proxy_ca > /tmp/build-ca.pem; \
      export CARGO_HTTP_CAINFO=/tmp/build-ca.pem; \
    fi; \
    npm run build:licenses

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libfontconfig1 libxkbcommon0 curl util-linux && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 1000 vrcx && useradd --uid 1000 --gid 1000 --home-dir /data --no-create-home vrcx \
    && install -d -o 1000 -g 1000 -m 0750 /data
WORKDIR /app
COPY --from=build /usr/local/bin/vrcx-0-web /usr/local/bin/vrcx-0-web
COPY --from=frontend /src/dist /app/dist
COPY LICENSE NOTICE /app/
COPY deploy/entrypoint.sh /usr/local/bin/vrcx-web-entrypoint
RUN chmod 0755 /usr/local/bin/vrcx-web-entrypoint
ENV VRCX_WEB_LISTEN=0.0.0.0:8080 VRCX_WEB_STATIC_DIR=/app/dist \
    XDG_CONFIG_HOME=/data/profile/.config XDG_CACHE_HOME=/data/profile/.cache
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["/usr/local/bin/vrcx-web-entrypoint"]
CMD ["vrcx-0-web", "--data-dir", "/data/profile"]
