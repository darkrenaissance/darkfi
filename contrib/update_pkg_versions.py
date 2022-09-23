#!/usr/bin/env python3
# Update the version in the toplevel Cargo.toml for DarkFi, and then run this
# script to update all the other Cargo.toml files.
import subprocess

from os import chdir
from subprocess import PIPE

import tomlkit


def update_package_version(filename, version):
    with open(filename) as f:
        content = f.read()

    p = tomlkit.parse(content)
    p["package"]["version"] = version

    with open(filename, "w") as f:
        f.write(tomlkit.dumps(p))


def main():
    toplevel = subprocess.run(["git", "rev-parse", "--show-toplevel"],
                              capture_output=True)
    toplevel = toplevel.stdout.decode().strip()
    chdir(toplevel)

    with open("Cargo.toml") as f:
        content = f.read()

    p = tomlkit.parse(content)
    version = p["package"]["version"]

    find_output = subprocess.run(
        ["find", ".", "-type", "f", "-name", "Cargo.toml"], stdout=PIPE)
    files = [i.strip() for i in find_output.stdout.decode().split("\n")][:-1]

    for filename in files:
        update_package_version(filename, version)


if __name__ == "__main__":
    main()
