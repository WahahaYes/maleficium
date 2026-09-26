# syntax=docker/dockerfile:1
# Reproducible Linux x86_64 release build: .deb + AppImage.
#
#   docker build --output type=local,dest=../maleficium-release .
#
# The artifacts (*.deb, *.AppImage) land flat in the --output directory.
# Needs network during the build (npm, crates.io, Tectonic release assets,
# SyncTeX source). See docs/BUILDING.md.
#
# Ubuntu 24.04 rather than a newer base on purpose: the binaries link against
# the build host's glibc, so an older base runs on more distributions.

FROM ubuntu:24.04@sha256:008173c23f95b170204355c12626cb5a965d779a7e1283b09e9cffbb1bf33ca3 AS build

ARG NODE_VERSION=22.23.2
ARG NODE_SHA256=d60acfe00a2932254bb0ad20e01b0d74397a0875595de719654b214f4b03f307
ARG RUSTUP_VERSION=1.29.1
ARG RUSTUP_SHA256=dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71
ARG RUST_VERSION=1.98.1

ENV DEBIAN_FRONTEND=noninteractive
# Tauri: webkit2gtk-4.1 and friends. AppImage: file. Sidecars: curl, git,
# python3, gcc + static zlib (SyncTeX), libgraphite2 (Tectonic smoke test).
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl file git pkg-config python3 wget xz-utils \
        libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
        zlib1g-dev libgraphite2-3 \
    && rm -rf /var/lib/apt/lists/*

RUN curl -sSfL -o /tmp/node.tar.xz \
        "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
    && echo "${NODE_SHA256}  /tmp/node.tar.xz" | sha256sum -c - \
    && tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1 \
    && rm /tmp/node.tar.xz

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -sSfL -o /tmp/rustup-init \
        "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/x86_64-unknown-linux-gnu/rustup-init" \
    && echo "${RUSTUP_SHA256}  /tmp/rustup-init" | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}" \
    && rm /tmp/rustup-init

WORKDIR /build

# Dependencies before sources, so a source edit reuses these layers.
COPY package.json package-lock.json ./
RUN npm ci

COPY scripts/fetch-sidecars.sh scripts/
COPY scripts/synctex-shim scripts/synctex-shim
RUN sh scripts/fetch-sidecars.sh

COPY . .

# linuxdeploy/appimagetool are AppImages themselves; there is no FUSE in a
# build container, so let them extract and run instead of mounting.
ENV APPIMAGE_EXTRACT_AND_RUN=1
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/src-tauri/target \
    npm run tauri build -- --bundles deb,appimage \
    && mkdir -p /out \
    && cp src-tauri/target/release/bundle/deb/*.deb src-tauri/target/release/bundle/appimage/*.AppImage /out/

FROM scratch AS artifacts
COPY --from=build /out/ /
