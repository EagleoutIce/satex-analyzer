# syntax=docker/dockerfile:1
# SaTeX in a container: `docker run --rm -v "$PWD:/work" ghcr.io/eagleoutice/satex lint -f main.tex`.
# Build: `docker build -t satex .`. TEX_PACKAGES picks the TeX Live part of the image (see README.md).

# At least 1.95 (if-let guards), whatever Cargo.toml's rust-version says.
ARG RUST_VERSION=1.95
ARG DEBIAN=trixie

FROM rust:${RUST_VERSION}-${DEBIAN} AS build
WORKDIR /src
# Unset uses every core. `--build-arg CARGO_BUILD_JOBS=4` builds on a shared machine.
ARG CARGO_BUILD_JOBS
COPY . .
# The registry and the target directory are cached between builds, so they are not in the layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bin satex \
 && strip target/release/satex \
 && cp target/release/satex /usr/local/bin/satex

FROM debian:${DEBIAN}-slim
# LaTeX, the recommended packages and their fonts, and latexmk for `satex compile`.
# `texlive-full` covers every package but adds about 5 GB.
ARG TEX_PACKAGES="texlive-latex-recommended texlive-fonts-recommended latexmk"
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates $TEX_PACKAGES \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/bin/satex /usr/local/bin/satex
# The interpreted kernel is built once here, not on every `docker run --rm`.
# Writable for every user, so `--user "$(id -u)"` can add package caches.
ENV SATEX_CACHE=/var/cache/satex
RUN satex cache build && chmod -R a+rwX "$SATEX_CACHE"

LABEL org.opencontainers.image.source="https://github.com/EagleoutIce/satex-analyzer" \
      org.opencontainers.image.description="SaTeX: static analyzer and language server for TeX and LaTeX" \
      org.opencontainers.image.licenses="MIT"
WORKDIR /work
# `satex lsp --host 0.0.0.0 --port 2087` serves the language server over TCP.
EXPOSE 2087
ENTRYPOINT ["satex"]
CMD ["--help"]
