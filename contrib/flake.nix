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

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
  inputs.utils.url = "github:numtide/flake-utils";
  inputs.crane.url = "github:ipetkov/crane";
  inputs.crane.inputs.nixpkgs.follows = "nixpkgs";

  outputs = {
    self,
    nixpkgs,
    utils,
    crane,
  }:
    utils.lib.eachDefaultSystem (system: let
      craneLib = crane.lib.${system};
      pkgs = import nixpkgs {
        inherit system;
      };
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
        craneLib.buildPackage ({
            src = ../.;
            nativeBuildInputs = myNativeBuildInputs;
            buildInputs = myBuildInputs;
            #RUST_BACKTRACE=1;
            #RUST_LOG="trace";
          }
          // attrs);
    in rec {
      packages = rec {
        darkfi-drk = myBuildRustPackage {
          pname = "darkfi-drk";
          cargoExtraArgs = "--package=drk";
          buildAndTestSubdir = "./bin/drk";
        };
        darkfi-darkfid = myBuildRustPackage {
          pname = "darkfi-darkfid";
          cargoExtraArgs = "--package=darkfid";
          buildAndTestSubdir = "./bin/darkfid";
        };
        darkfi-dnetview = myBuildRustPackage {
          pname = "darkfi-dnetview";
          cargoExtraArgs = "--package=dnetview";
          buildAndTestSubdir = "./bin/dnetview";
        };
        darkfi-ircd = myBuildRustPackage {
          pname = "darkfi-ircd";
          cargoExtraArgs = "--package=ircd";
          buildAndTestSubdir = "./bin/ircd";
        };
        darkfi-tau = myBuildRustPackage {
          pname = "darkfi-tau";
          cargoExtraArgs = "--package=tau";
          buildAndTestSubdir = "./bin/tau";
        };
        darkfi-taud = myBuildRustPackage {
          pname = "darkfi-taud";
          cargoExtraArgs = "--package=taud";
          buildAndTestSubdir = "./bin/taud";
        };
        darkfi-zkas = myBuildRustPackage {
          pname = "darkfi-zkas";
          cargoExtraArgs = "--package=zkas";
          buildAndTestSubdir = "./bin/zkas";
        };
        darkfi-vanityaddr = myBuildRustPackage {
          pname = "darkfi-vanityaddr";
          cargoExtraArgs = "--package=vanityaddr";
          buildAndTestSubdir = "./bin/vanityadddr";
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
