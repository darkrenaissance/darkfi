

## Files

* Dockerfile.*Emulation : not updated, used for ARM target build on x64 host via emulation, see https://docs.docker.com/build/building/multi-platform/

* Dockerfile.<linux_os> : should work on aarch64 and x86_64 hosts

* riscv.Dockerfile : experimental RISC-V support, see https://en.wikipedia.org/wiki/RISC-V

## Params

**DOCKER_BUILDKIT=0** : old docker build, will be deprecated
DOCKER_BUILDKIT=0 docker build . --pull --shm-size=196m -t darkfi:debian -f ./contrib/docker/Dockerfile.debian

**--build-arg OS_VER=fedora:37** : define OS version

**--build-arg RUST_VER=nightly** : define Rust version

**--build-arg BINS="darkfid  darkfid2"** : specify what binaries to build

**--build-arg DONT_EXEC_TESTS=1** : do not execute tests when building image

