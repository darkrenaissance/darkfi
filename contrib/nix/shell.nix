{
  pkgs ? import <nixpkgs> {},
  ...
}:
pkgs.mkShell rec {
  nativeBuildInputs = with pkgs; [
    # Nightly toolchains
    llvmPackages.bintools
    rustup
    pkg-config
    sqlcipher
    gnumake
    cmake
    clang
    libclang
    llvm
    git
    openssl
    cacert
    wabt
  ];

  ## Manage toolchain with rustup instead of nix-store
  ## https://nixos.wiki/wiki/Rust
  RUSTC_VERSION = pkgs.lib.readFile ../../rust-toolchain.toml;
  LIBCLANG_PATH = pkgs.lib.makeLibraryPath [pkgs.llvmPackages_latest.libclang.lib];
  shellHook = ''
    export PATH=$PATH:''${CARGO_HOME:-~/.cargo}/bin
    export PATH=$PATH:''${RUSTUP_HOME:-~/.rustup}/toolchains/$RUSTC_VERSION-x86_64-unknown-linux-gnu/bin/
  '';
  # Add precompiled library to rustc search path
  RUSTFLAGS = builtins.map (a: ''-L ${a}/lib'') [
    # add libraries here (e.g. pkgs.libvmi)
  ];
  # Add glibc, clang, glib and other headers to bindgen search path
  BINDGEN_EXTRA_CLANG_ARGS =
    # Includes with normal include path
    (builtins.map (a: ''-I"${a}/include"'') [
      # add dev libraries here (e.g. pkgs.libvmi.dev)
      pkgs.glibc.dev
    ])
    # Includes with special directory paths
    ++ [
      ''-I"${pkgs.llvmPackages_latest.libclang.lib}/lib/clang/${pkgs.llvmPackages_latest.libclang.version}/include"''
      ''-I"${pkgs.glib.dev}/include/glib-2.0"''
      ''-I${pkgs.glib.out}/lib/glib-2.0/include/''
    ];
}
