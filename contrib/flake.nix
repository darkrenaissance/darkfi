# Nix flake
#
# To use it, install Nix & Nix flakes (see https://nixos.wiki/wiki/Flakes)
# Then use these commands to get a development shell, build and run binaries:
# $ cd contrib
# $ nix develop
# $ nix build '.#darkfi-ircd'
# $ nix run '.#darkfi-ircd'

{
  description = "DarkFi";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-22.11";
  inputs.utils.url = "github:numtide/flake-utils";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";

  outputs = { self, nixpkgs, utils, rust-overlay }:
    utils.lib.eachDefaultSystem (system:
      let
        overlays = [
          (import rust-overlay)
        ];
        pkgs = import nixpkgs rec {
          inherit system overlays;
        };
        rust-bin = pkgs.rust-bin.stable."1.66.0".default.override {
          extensions = [ "rust-src" ];
        };
        buildRustPackage = (pkgs.makeRustPlatform {
          cargo = rust-bin;
          rustc = rust-bin;
        }).buildRustPackage;
        myNativeBuildInputs = with pkgs; [
          pkg-config
          gnumake
          cmake
          clang
          libclang
          llvm
        ];
        myBuildInputs = with pkgs; [
          expat
          fontconfig
          freetype
          openssl
        ];
        myBuildRustPackage = attrs:
          buildRustPackage ({
            version = "0.3.0";
            src = ../.;
            cargoLock = {
              lockFile = ../Cargo.lock;
              outputHashes = {
                "term_grid-0.2.0" = "sha256-wl4ZlFjY34xPuMpXdX1a/g9YUomKLIYp2U8r9/goti0=";
                "halo2_proofs-0.2.0" = "sha256-WkDg1SgGPH9KyEAgwyl+OXrPk+w9Rd+8ZC+bbGx0Yek=";
                #"dashu-0.2.0" = "sha256-gD1eb0N6sxEm6vpaQWqdodRGPvAlsPcV2Jxp63VQZZ4=";
              };
            };
            nativeBuildInputs = myNativeBuildInputs;
            buildInputs = myBuildInputs;
            #RUST_BACKTRACE=1;
            #RUST_LOG="trace";
          } // attrs);
      in rec {
        packages = rec {
          darkfi-drk = myBuildRustPackage rec {
            pname = "darkfi-drk";
            buildAndTestSubdir = "./bin/drk";
          };
          darkfi-darkfid = myBuildRustPackage rec {
            pname = "darkfi-darkfid";
            buildAndTestSubdir = "./bin/darkfid";
          };
          darkfi-dnetview = myBuildRustPackage rec {
            pname = "darkfi-dnetview";
            buildAndTestSubdir = "./bin/dnetview";
          };
          darkfi-ircd = myBuildRustPackage rec {
            pname = "darkfi-ircd";
            buildAndTestSubdir = "./bin/ircd";
          };
          darkfi-tau = myBuildRustPackage rec {
            pname = "darkfi-tau";
            buildAndTestSubdir = "./bin/tau";
          };
          darkfi-taud = myBuildRustPackage rec {
            pname = "darkfi-taud";
            buildAndTestSubdir = "./bin/taud";
          };
          darkfi-zkas = myBuildRustPackage rec {
            pname = "darkfi-zkas";
            buildAndTestSubdir = "./bin/zkas";
          };
          default = darkfi-drk;
        };
        defaultPackage = packages.default; # compat

        apps = rec {
          darkfi-drk = utils.lib.mkApp {
            drv = packages.darkfi-drk;
            exePath = "/bin/drk";
          };
          darkfi-darkfid = utils.lib.mkApp {
            drv = packages.darkfi-darkfid;
            exePath = "/bin/darkfid";
          };
          darkfi-dnetview = utils.lib.mkApp {
            drv = packages.darkfi-dnetview;
            exePath = "/bin/dnetview";
          };
          darkfi-ircd = utils.lib.mkApp {
            drv = packages.darkfi-ircd;
            exePath = "/bin/ircd";
          };
          darkfi-tau = utils.lib.mkApp {
            drv = packages.darkfi-tau;
            exePath = "/bin/tau";
          };
          darkfi-taud = utils.lib.mkApp {
            drv = packages.darkfi-taud;
            exePath = "/bin/taud";
          };
          darkfi-zkas = utils.lib.mkApp {
            drv = packages.darkfi-zkas;
            exePath = "/bin/zkas";
          };
          default = darkfi-drk;
        };
      });
}
