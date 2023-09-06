/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

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
