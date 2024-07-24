# Usage(on repo root folder):
#   docker build . --pull --no-cache --shm-size=196m -t darkfi:static -f ./contrib/docker/static.Dockerfile -o static-bins
# Optionally, add arguments like: --build-arg BINS=darkirc
# If you want to keep building using the same builder, remove --no-cache option.

# Arguments configuration
ARG ALPINE_VER=edge
ARG SQLCIPHER_VER=4.5.5
ARG RUST_VER=nightly
#TODO: only darkirc, taud and lilith has been tested, have to check rest binaries to add deps.
ARG BINS="darkirc taud lilith"

# Environment setup
FROM alpine:${ALPINE_VER} as builder

## Arguments import
ARG SQLCIPHER_VER
ARG RUST_VER
ARG BINS

## Install system dependencies
RUN apk update
RUN apk add rustup git musl-dev make gcc openssl-dev openssl-libs-static tcl-dev zlib-static

## Setup SQLCipher
RUN wget -O sqlcipher.tar.gz https://github.com/sqlcipher/sqlcipher/archive/refs/tags/v${SQLCIPHER_VER}.tar.gz && \
    tar xf sqlcipher.tar.gz && \
    rm sqlcipher.tar.gz && \
    mv sqlcipher-${SQLCIPHER_VER} sqlcipher && \
    cd sqlcipher && \
    ./configure --prefix=/usr/local --disable-shared --enable-static --enable-cross-thread-connections --enable-releasemode && \
    make -j$(nproc) && \
    make install

## Configure Rust
RUN rustup-init --default-toolchain none -y
ENV PATH "${PATH}:/root/.cargo/bin/"
RUN rustup default ${RUST_VER}
RUN rustup target add wasm32-unknown-unknown

# Build binaries
FROM builder as rust_builder

WORKDIR /opt/darkfi
COPY . /opt/darkfi

RUN sed -e 's,^#RUSTFLAGS ,RUSTFLAGS ,' -i Makefile
RUN make clean && make ${BINS} &&  mkdir compiled-bins && \
    (if [ -e zkas ]; then cp -a zkas compiled-bins/; fi;) && \
    (if [ -e darkfid ]; then cp -a darkfid compiled-bins/; fi;) && \
    (if [ -e faucetd ]; then cp -a faucetd compiled-bins/; fi;) && \
    (if [ -e darkirc ]; then cp -a darkirc compiled-bins/; fi;) && \
    (if [ -e "genev-cli" ]; then cp -a genev-cli compiled-bins/; fi;) && \
    (if [ -e genevd ]; then cp -a genevd compiled-bins/; fi;) && \
    (if [ -e lilith ]; then cp -a lilith compiled-bins/; fi;) && \
    (if [ -e taud ]; then cp -a taud compiled-bins/; fi;) && \
    (if [ -e vanityaddr ]; then cp -a vanityaddr compiled-bins/; fi;)

# Export binaries from rust builder
FROM scratch
COPY --from=rust_builder /opt/darkfi/compiled-bins/* ./
