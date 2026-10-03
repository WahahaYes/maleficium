# syntax=docker/dockerfile:1
# Local Linux x86_64 release build: .deb + AppImage, the same packages CI's
# ubuntu-24.04 job builds. Only an environment: the build itself is
# scripts/package.sh, as on every CI runner.
#
#   docker build --output type=local,dest=../maleficium-release .
#
# The artifacts (*.deb, *.AppImage) land flat in the --output directory.
# Needs network during the build (npm, crates.io, Tectonic release assets,
# SyncTeX source). See docs/BUILDING.md.
#
# Ubuntu 24.04, the oldest base the packages can support: the binaries link
# against the build host's glibc, and the pinned Linux Tectonic (a glibc
# build) already needs glibc 2.39. Going older means a musl Tectonic first.

FROM ubuntu:24.04@sha256:008173c23f95b170204355c12626cb5a965d779a7e1283b09e9cffbb1bf33ca3 AS build

# Node's checksum is per version: bump it together with .nvmrc (a stale pair
# fails the check below). Rust's version comes from rust-toolchain.toml.
ARG NODE_SHA256=d60acfe00a2932254bb0ad20e01b0d74397a0875595de719654b214f4b03f307
ARG RUSTUP_VERSION=1.29.1
ARG RUSTUP_SHA256=dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71

ENV DEBIAN_FRONTEND=noninteractive
# The same package list CI's ubuntu-24.04 job installs.
COPY scripts/linux-deps.txt /tmp/
RUN apt-get update && xargs -a /tmp/linux-deps.txt apt-get install -y --no-install-recommends \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY .nvmrc rust-toolchain.toml ./
RUN NODE_VERSION=$(cat .nvmrc) && curl -sSfL -o /tmp/node.tar.xz \
        "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
    && echo "${NODE_SHA256}  /tmp/node.tar.xz" | sha256sum -c - \
    && tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1 \
    && rm /tmp/node.tar.xz

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -sSfL -o /tmp/rustup-init \
        "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/x86_64-unknown-linux-gnu/rustup-init" \
    && echo "${RUSTUP_SHA256}  /tmp/rustup-init" | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain none \
    && rm /tmp/rustup-init \
    && rustup toolchain install

# Dependencies before sources, so a source edit reuses these layers.
COPY package.json package-lock.json ./
RUN npm ci

# The engine builds from source here, on its own pinned nightly.
COPY scripts/fetch-sidecars.sh scripts/
COPY scripts/synctex-shim scripts/synctex-shim
COPY src-tauri/engine src-tauri/engine
RUN cd src-tauri/engine && rustup toolchain install
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/src-tauri/engine/target \
    sh scripts/fetch-sidecars.sh

COPY . .

# The target dir is a cache mount; package.sh clears its stale bundles.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/src-tauri/target \
    sh scripts/package.sh --out /out

FROM scratch AS artifacts
COPY --from=build /out/ /
