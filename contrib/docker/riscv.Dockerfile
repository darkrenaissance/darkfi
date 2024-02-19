# Usage(on repo root folder):
#   docker build . --pull --no-cache --shm-size=196m -t darkfi:riscv -f ./contrib/docker/riscv.Dockerfile -o riscv-bins
# Optionally, add arguments like: --build-arg BINS=darkirc
# If you want to keep building using the same builder, remove --no-cache option.

# Arguments configuration
ARG DEBIAN_VER=latest
ARG SQL_VER=3420000
ARG RUST_VER=nightly
#TODO: only darkirc has been tested, have to check rest binaries to add deps and ports.
ARG BINS=darkirc
# When changing riscv target, don't forget to also update the linker
ARG RISCV_TARGET=riscv64gc-unknown-linux-gnu
ARG RISCV_LINKER=riscv64-linux-gnu-gcc

# Environment setup
FROM debian:${DEBIAN_VER} as builder

## Arguments import
ARG SQL_VER
ARG RUST_VER
ARG RISCV_TARGET
ARG RISCV_LINKER
ARG BINS

## Install system dependencies
RUN apt-get update
RUN apt-get install -y make gcc pkg-config gcc-riscv64-linux-gnu wget

## Build sqlite3 for Risc-V
RUN wget https://www.sqlite.org/2023/sqlite-autoconf-${SQL_VER}.tar.gz && \
    tar xvfz sqlite-autoconf-${SQL_VER}.tar.gz && \
    rm sqlite-autoconf-${SQL_VER}.tar.gz && \
    mv sqlite-autoconf-${SQL_VER} sqlite3 && \
    cd sqlite3 && \
    ./configure --host=riscv64-linux-gnu CC=/bin/${RISCV_LINKER} && \
    make

## Rust installation
RUN wget -O install-rustup.sh https://sh.rustup.rs && \
    sh install-rustup.sh -yq --default-toolchain none && \
    rm install-rustup.sh
ENV PATH "${PATH}:/root/.cargo/bin/"
RUN rustup default ${RUST_VER}
RUN rustup target add ${RISCV_TARGET}

# Build binaries
FROM builder as rust_builder

WORKDIR /opt/darkfi
COPY . /opt/darkfi

# Risc-V support is highly experimental so we have to add some hack patches,
# at the end of Cargo.toml where [patch.crates-io] exists.
RUN echo 'ring = {git="https://github.com/aggstam/ring"} \n\
rustls = {git="https://github.com/aggstam/rustls", branch="risc-v"} \n\
rcgen = {git="https://github.com/aggstam/rcgen"} \n\
' >> Cargo.toml
RUN sed -i Cargo.toml -e "s|0.11.3|0.11.1|g"
# Add hacked dependencies into each crate that uses them
RUN sed -i bin/darkirc/Cargo.toml \
    -e "s|\[dependencies\]|\[dependencies\]\nring = \"0.16.20\"|g"
RUN cargo update

ENV RUSTFLAGS="-C linker=/bin/${RISCV_LINKER} -L/sqlite3/.libs/"
ENV TARGET_PRFX="--target=" RUST_TARGET="${RISCV_TARGET}"
RUN make ${BINS} &&  mkdir compiled-bins && \
    (if [ -e zkas ]; then cp -a zkas compiled-bins/; fi;) && \
    (if [ -e darkfid ]; then cp -a darkfid compiled-bins/; fi;) && \
    (if [ -e faucetd ]; then cp -a faucetd compiled-bins/; fi;) && \
    (if [ -e darkirc ]; then cp -a darkirc compiled-bins/; fi;) && \
    (if [ -e "genev-cli" ]; then cp -a genev-cli compiled-bins/; fi;) && \
    (if [ -e genevd ]; then cp -a genevd compiled-bins/; fi;) && \
    (if [ -e lilith ]; then cp -a lilith compiled-bins/; fi;) && \
    (if [ -e "tau-cli" ]; then cp -a tau-cli compiled-bins/; fi;) && \
    (if [ -e taud ]; then cp -a taud compiled-bins/; fi;) && \
    (if [ -e vanityaddr ]; then cp -a vanityaddr compiled-bins/; fi;)

# Export binaries from rust builder
FROM scratch
COPY --from=rust_builder /opt/darkfi/compiled-bins/* ./
