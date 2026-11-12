# Static musl build of pulply in a distroless image (CA certificates, /tmp, non-root user).
#
#   podman build -t pulply -f Containerfile .
#   podman run --rm -it -v ~/.config/pulply:/home/nonroot/.config/pulply:ro pulply --context prod
#   podman run --rm -p 8484:8484 -v ~/.config/pulply:/home/nonroot/.config/pulply:ro pulply serve --listen 0.0.0.0:8484

FROM docker.io/library/node:22-alpine AS web
WORKDIR /web
COPY crates/pulply/web/package.json crates/pulply/web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY crates/pulply/web/ ./
RUN npm run build

FROM docker.io/library/rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
COPY --from=web /web/dist crates/pulply/web/dist
# The interface is built by the `web` stage: build.rs must not look for npm here.
RUN PULPLY_SKIP_WEB=1 cargo build --release --locked -p pulply \
    && cp target/release/pulply /pulply

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /pulply /usr/local/bin/pulply
ENV HOME=/home/nonroot TERM=xterm-256color
ENTRYPOINT ["/usr/local/bin/pulply"]
