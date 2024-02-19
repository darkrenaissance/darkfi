darkirc
=======

If you're trying to join the chat, then for now, use the older ircd,
[Installation Guide](https://darkrenaissance.github.io/darkfi/misc/ircd/ircd.html).

DarkIRC is still pending a few more upgrades before we switch over.

## Android build

1. Install `android-ndk`
2. Compile `openssl` with the Android toolchain
3. Compile `sqlcipher` with the Android toolchain and the `openssl` lib
4. Compile `darkirc`

### OpenSSL

```
$ git clone https://github.com/openssl/openssl
$ cd openssl
$ export ANDROID_NDK_ROOT="/opt/android-ndk"
$ export PATH="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH"
$ ./Configure android-arm64 -D__ANDROID_API__=32
$ make -j$(nproc)
```

### SQLcipher

```
$ git clone https://github.com/sqlcipher/sqlcipher
$ cd sqlcipher
$ sed -e 's/strchrnul//' -i configure
$ export ANDROID_NDK_ROOT="/opt/android-ndk"
$ export PATH="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH"
$ CC=aarch64-linux-android32-clang \
  CPPFLAGS="-I$PWD/../openssl/include" \
  LDFLAGS="-L$PWD/../openssl" \
  ./configure \
      --host=aarch64-linux-android32 \
      --disable-shared \
      --enable-static \
      --enable-cross-thread-connections \
      --enable-releasemode \
      --disable-tcl
$ make -j$(nproc)
$ ./libtool --mode install install libsqlcipher.la $PWD
```

### DarkIRC

```
$ make darkirc.android64
```
