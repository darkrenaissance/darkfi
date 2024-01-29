# Darkfi - Nixos impure flake

## Installation (impure)

This flake sticks to the initial design and uses Makefiles maintainedby the
DarkFi core team.

Using nix builders would make the package evaluation pure, but would although
lead to duplicate and unevitably to an out of date flake.

Make this flake pure

### Prerequisites

The flake buildPhases need network access that are by default disabled by nix.

You need to first *_relax_ nix build sandbox parameter to allow impure builds.

Either in your nixos configuration:

```nix
# configuration.nix
nix.settings.sandbox = "relaxed"
```

or exceptionnaly via command-line as an argument of an installation command.

```sh
nix-env -i github:darkrenaissance/darki?dir=contrib/nix --no-sandbox
```

## Usage

Import it in your flake inputs.

```sh
inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    darkfi.url = "github:darkrenaissance/darki?dir=contrib/nix";
};
```

Add the packages to your environment or to a specific users.

```nix
environment.systemPackages = with pkgs; [
    inputs.darkfi.packages.${system}.default
];
```
