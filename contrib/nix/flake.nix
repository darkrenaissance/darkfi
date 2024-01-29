# Nix flake
{
  description = "DarkFi - Anonimous blockchain primitives";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = inputs:
    with inputs;
      flake-utils.lib.eachDefaultSystem (
        system: let
          pkgs = nixpkgs.legacyPackages.${system};
        in rec {
          packages.default = pkgs.callPackage ./default.nix {};
          devShells.default = pkgs.callPackage ./shell.nix {};
        }
      );
}
