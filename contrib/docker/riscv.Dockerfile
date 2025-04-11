# This script uses QEMU emulation, so before execution pull the corresponding
# images:
#   docker run --rm --privileged multiarch/qemu-user-static --reset -p yes

# Usage(on repo root folder):
#   docker build . --platform=linux/riscv64 --pull --no-cache --shm-size=196m -t darkfi:riscv -f ./contrib/docker/riscv2.Dockerfile -o riscv-bins
# Optionally, add arguments like: --build-arg BINS=darkirc
# If you want to keep building using the same builder, remove --no-cache option.

# Arguments configuration
ARG DEBIAN_VER=sid
ARG RUST_VER=nightly-2025-04-10
#TODO: only darkirc has been tested, have to check rest binaries to add deps and ports.
ARG BINS=darkirc

# Environment setup
FROM --platform=$TARGETPLATFORM riscv64/debian:${DEBIAN_VER} as builder

## Arguments import
ARG RUST_VER
ARG BINS

## Install system dependencies
RUN apt-get update
RUN apt-get install -y git cmake make gcc g++ pkg-config \
    libasound2-dev libclang-dev libssl-dev libsqlcipher-dev \
    libsqlite3-dev wabt wget

## Rust installation
RUN wget -O install-rustup.sh https://sh.rustup.rs && \
    sh install-rustup.sh -yq --default-toolchain none && \
    rm install-rustup.sh
ENV PATH "${PATH}:/root/.cargo/bin/"
RUN rustup default ${RUST_VER}
RUN rustup target add wasm32-unknown-unknown --toolchain ${RUST_VER}

# Build binaries
FROM builder as rust_builder

WORKDIR /opt/darkfi
COPY . /opt/darkfi

# Cleanup existing binaries
RUN rm -rf zkas bin/zkas/zkas darkfid bin/darkfid/darkfid \
    darkirc bin/darkirc/darkirc lilith bin/lilith/lilith \
    taud bin/tau/taud/taud vanityaddr bin/vanityaddr/vanityaddr

# Risc-V support is highly experimental so we have to add some hack patches
# at Cargo.toml where [patch.crates-io] exists.
RUN sed -i Cargo.toml -e '335iring = {git="https://github.com/aggstam/ring"} \n\
rustls = {git="https://github.com/aggstam/rustls", branch="risc-v"} \n\
rcgen = {git="https://github.com/aggstam/rcgen"} \n'

# Add hacked dependencies into each crate that uses them
RUN sed -i bin/darkirc/Cargo.toml \
    -e "s|\[dependencies\]|\[dependencies\]\nring = \"0.16.20\"|g"
RUN cargo +${RUST_VER} update

RUN sed -i rust-toolchain.toml -e "s|nightly|${RUST_VER}|g"
RUN make CARGO="cargo +${RUST_VER}" ${BINS} && \
    mkdir compiled-bins && \
    (if [ -e zkas ]; then cp -a zkas compiled-bins/; fi;) && \
    (if [ -e darkfid ]; then cp -a darkfid compiled-bins/; fi;) && \
    (if [ -e darkirc ]; then cp -a darkirc compiled-bins/; fi;) && \
    (if [ -e lilith ]; then cp -a lilith compiled-bins/; fi;) && \
    (if [ -e taud ]; then cp -a taud compiled-bins/; fi;) && \
    (if [ -e vanityaddr ]; then cp -a vanityaddr compiled-bins/; fi;)

# Export binaries from rust builder
FROM scratch
COPY --from=rust_builder /opt/darkfi/compiled-bins/* ./
