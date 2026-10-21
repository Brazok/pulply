# Static musl build of pulply in a distroless image (CA certificates, /tmp, non-root user).
#
#   podman build -t pulply -f Containerfile .
#   podman run --rm -it -v ~/.config/pulply:/home/nonroot/.config/pulply:ro pulply --context prod

FROM docker.io/library/rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p pulply \
    && cp target/release/pulply /pulply

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /pulply /usr/local/bin/pulply
ENV HOME=/home/nonroot TERM=xterm-256color
ENTRYPOINT ["/usr/local/bin/pulply"]
