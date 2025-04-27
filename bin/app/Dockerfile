FROM docker.io/archlinux

RUN pacman -Syu --noconfirm
RUN pacman -S --noconfirm jdk17-openjdk unzip wget cmake openssl pkgconf gcc git zip

# github override HOME, so here we are
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH 

RUN curl -o rustup.sh --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs
RUN chmod +x rustup.sh
RUN ./rustup.sh -y
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
    wget -q https://dl.google.com/android/repository/commandlinetools-linux-13114758_latest.zip && \
    unzip -q commandlinetools-linux-13114758_latest.zip && \
    rm commandlinetools-linux-13114758_latest.zip && \
    mv cmdline-tools latest && \
    mkdir cmdline-tools/ && \
    mv latest cmdline-tools/ && \
    chown -R root:root /opt
RUN mkdir -p ~/.android && touch ~/.android/repositories.cfg
RUN yes | ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager "platform-tools" | grep -v = || true
RUN yes | ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager "platforms;android-36" | grep -v = || true
RUN yes | ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager "build-tools;36.0.0-rc5"  | grep -v = || true
RUN ${ANDROID_HOME}/tools/bin/sdkmanager --update | grep -v = || true

# Install Android NDK
RUN cd /usr/local && \
    wget -q http://dl.google.com/android/repository/android-ndk-r25-linux.zip && \
    unzip -q android-ndk-r25-linux.zip && \
    rm android-ndk-r25-linux.zip
ENV NDK_HOME /usr/local/android-ndk-r25

# Copy contents to container. Should only use this on a clean directory
WORKDIR /root/
RUN git clone https://github.com/narodnik/cargo-quad-apk cargo-apk
#RUN git clone https://github.com/not-fl3/cargo-quad-apk cargo-apk
# For deterministic builds, we want a deterministic toolchain
#RUN cd /root/cargo-apk && git checkout f3b865610b79a2b1b9d2b90600c36390a9e19569

# Install binary
RUN cargo install --path /root/cargo-apk

# Add build-tools to PATH, for apksigner
ENV PATH="/opt/android-sdk-linux/build-tools/36.0.0-rc5/:${PATH}"

# Lets cache packages for faster builds
RUN git clone --depth=1 https://codeberg.org/darkrenaissance/darkfi
COPY Cargo.lock darkfi/bin/app/
WORKDIR /root/darkfi/bin/app/
RUN cargo fetch
RUN rm -fr /root/darkfi/

