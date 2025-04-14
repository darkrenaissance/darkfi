FROM docker.io/ubuntu
ENV DEBIAN_FRONTEND noninteractive

RUN apt-get update
RUN apt install -yq openjdk-21-jre-headless openjdk-21-jdk-headless
RUN apt install -yq wget unzip cmake file
# For vendored openssl
RUN apt-get update && apt-get install -y build-essential checkinstall zlib1g-dev

RUN cd /tmp/ && \
    wget -O install-rustup.sh https://sh.rustup.rs && \
    sh install-rustup.sh -yq --default-toolchain none && \
    rm install-rustup.sh
ENV PATH "${PATH}:/root/.cargo/bin/"
RUN rustup default stable
RUN rustup target add aarch64-linux-android
#RUN rustup target add armv7-linux-androideabi
#RUN rustup target add i686-linux-android
#RUN rustup target add x86_64-linux-android

# Install Android SDK
ENV ANDROID_HOME /opt/android-sdk/
RUN mkdir ${ANDROID_HOME} && \
    cd ${ANDROID_HOME} && \
    wget -O cmdline-tools.zip -q https://dl.google.com/android/repository/commandlinetools-linux-10406996_latest.zip && \
    unzip cmdline-tools.zip && \
    rm cmdline-tools.zip
# Required by SDKManager
RUN cd ${ANDROID_HOME}/cmdline-tools/ && \
    mkdir latest && \
    mv bin lib latest
RUN yes | ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager --licenses
RUN ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager "platform-tools"
RUN ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager "platforms;android-34"
RUN ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager "ndk;25.2.9519653"
RUN ${ANDROID_HOME}/cmdline-tools/latest/bin/sdkmanager "build-tools;34.0.0"

RUN echo '[target.aarch64-linux-android] \n\
ar = "/opt/android-sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar" \n\
linker = "/opt/android-sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android33-clang" \n\
' > /root/.cargo/config.toml
# wtf cargo
ENV RANLIB /opt/android-sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ranlib

# Needed by the ring dependency
ENV TARGET_AR /opt/android-sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar
ENV TARGET_CC /opt/android-sdk/ndk/25.2.9519653/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android33-clang

# Make sqlcipher
# Needed for sqlcipher amalgamation
#RUN apt install -yq tclsh libssl-dev
#RUN cd /tmp/ && \
#    wget -O sqlcipher.zip https://github.com/sqlcipher/sqlcipher/archive/refs/tags/v4.5.6.zip && \
#    unzip sqlcipher.zip && \
#    rm sqlcipher.zip && \
#    mv sqlcipher* sqlcipher && \
#    cd sqlcipher && \
#    ./configure && \
#    make sqlite3.c && \
#    mkdir build && \
#    mv *.c *.h build/ && \
#    mkdir jni && \
#    echo '\
#APP_ABI := arm64-v8a \n\
#APP_CPPFLAGS += -fexceptions -frtti \n\
#APP_STL := c++_shared' > jni/Application.mk && \
#    echo '\
#LOCAL_PATH := $(call my-dir) \n\
#include $(CLEAR_VARS) \n\
#LOCAL_MODULE            := sqlcipher-a \n\
#LOCAL_MODULE_FILENAME   := libsqlcipher \n\
#LOCAL_SRC_FILES         := ../build/sqlite3.c \n\
#LOCAL_C_INCLUDES        := ../build \n\
#LOCAL_EXPORT_C_INCLUDES := ../build \n\
#LOCAL_CFLAGS            := -DSQLITE_THREADSAFE=1 \n\
#include $(BUILD_STATIC_LIBRARY)' > jni/Android.mk && \
#    /opt/android-sdk/ndk/25.2.9519653/ndk-build
#ENV RUSTFLAGS "-L/tmp/sqlcipher/obj/local/arm64-v8a/"

# Make directory for user code
RUN mkdir /root/src
WORKDIR /root/src/bin/darkirc/

