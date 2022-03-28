{
  description = "Darkfi Dev Environment";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-21.11";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.mozilla = { url = "github:mozilla/nixpkgs-mozilla"; flake = false; };

  outputs =
    { self
    , nixpkgs
    , mozilla
    , flake-utils
    , ...
    } @inputs:
    let rustOverlay = final: prev:
          let rustChannel = prev.rustChannelOf {
            channel = "1.58.0";
            sha256 = "sha256-eQBpSmy9+oHfVyPs0Ea+GVZ0fvIatj6QVhNhYKOJ6Jk=";
          };
          in
          { inherit rustChannel;
            rustc = rustChannel.rust;
            cargo = rustChannel.rust;
          };
    in flake-utils.lib.eachDefaultSystem
      (system:
        let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (import "${mozilla}/rust-overlay.nix")
            rustOverlay
          ];
        };
        in {
          devShell = pkgs.mkShell {
            #RUST_BACKTRACE=1;
            #RUST_LOG="trace";
            buildInputs = with pkgs; [
              openssl
              gnumake
              openssl
              clang
              libclang
              pkg-config
              cmake
              llvm
              freetype
              fontconfig
              (rustChannel.rust.override { extensions = [ "rust-src" ]; })
            ];
            LIBCLANG_PATH="${pkgs.libclang.lib}/lib";
          };
        });
}
