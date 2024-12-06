FROM docker.io/archlinux

RUN pacman -Syu --noconfirm
RUN pacman -S --noconfirm jdk8-openjdk unzip wget cmake openssl pkgconf gcc git

# github override HOME, so here we are
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH 

RUN curl -o rustup.sh --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs
RUN chmod +x rustup.sh
RUN ./rustup.sh -y --default-toolchain nightly-2024-12-05

#RUN rustup toolchain install nightly
#RUN rustup default nightly
RUN rustc --version

RUN rustup target add armv7-linux-androideabi
RUN rustup target add aarch64-linux-android
RUN rustup target add i686-linux-android
RUN rustup target add x86_64-linux-android

# Install Android SDK
ENV ANDROID_HOME /opt/android-sdk-linux
ENV JAVA_HOME /usr/lib/jvm/default
RUN mkdir ${ANDROID_HOME} && \
    cd ${ANDROID_HOME} && \
    wget -q https://dl.google.com/android/repository/sdk-tools-linux-4333796.zip && \
    unzip -q sdk-tools-linux-4333796.zip && \
    rm sdk-tools-linux-4333796.zip && \
    chown -R root:root /opt
RUN mkdir -p ~/.android && touch ~/.android/repositories.cfg
RUN yes | ${ANDROID_HOME}/tools/bin/sdkmanager "platform-tools" | grep -v = || true
RUN yes | ${ANDROID_HOME}/tools/bin/sdkmanager "platforms;android-31" | grep -v = || true
RUN yes | ${ANDROID_HOME}/tools/bin/sdkmanager "build-tools;31.0.0"  | grep -v = || true
RUN ${ANDROID_HOME}/tools/bin/sdkmanager --update | grep -v = || true

# Install Android NDK
RUN cd /usr/local && \
    wget -q http://dl.google.com/android/repository/android-ndk-r25-linux.zip && \
    unzip -q android-ndk-r25-linux.zip && \
    rm android-ndk-r25-linux.zip
ENV NDK_HOME /usr/local/android-ndk-r25

# Copy contents to container. Should only use this on a clean directory
WORKDIR /root/
RUN git clone https://github.com/not-fl3/cargo-quad-apk cargo-apk
# For deterministic builds, we want a deterministic toolchain
RUN cd /root/cargo-apk && git checkout 8962f6888e748e201f6ac2ced411669a3f939e07

# ArmV7a
#ENV CC ${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi31-clang
#ENV CXX ${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi31-clang++
#ENV AR ${NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar
#ENV CXXSTDLIB c++
# Arm64V8a
# X86

# Install binary
RUN cargo install --path /root/cargo-apk

# Add build-tools to PATH, for apksigner
ENV PATH="/opt/android-sdk-linux/build-tools/31.0.0/:${PATH}"
