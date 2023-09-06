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

from tabulate import tabulate

headers = [
    "row_index", "first_name", "last_name", "..."
]

table = [
    ["46", "john", "doe", "..."],
    ["...", "", "", ""]
]

print(tabulate(table, headers, tablefmt="github"))
print()

# Foos

headers = [
    "foo_index", "foo_name"
]

table = [
    [1, "john doe"],
    [2, "alison bob"],
    ["...", ""],
]

print(tabulate(table, headers, tablefmt="github"))
print()

# Foo-Bars

headers = [
    "foo_index", "bar_index"
]

table = [
    [1, 73],
    [1, 74],
    ["...", ""],
]

print(tabulate(table, headers, tablefmt="github"))
print()

# Bars

headers = [
    "bar_index", "bar_x"
]

table = [
    [73, 110],
    [74, 4],
    ["...", ""],
]

print(tabulate(table, headers, tablefmt="github"))
print()

##### DAO State

headers = [
    "dao_tree_index", "proposal_tree_index"
]
table = [
    [301, 406]
]
print(tabulate(table, headers, tablefmt="github"))
print()

# DAO bullas

headers = [
    "dao_bulla"
]
table = [
    ["0xabea9132b05a70803a4e85094fd0e1800777fbef"],
    ["0x7c4de4aa5068376033aef8e3df766aff3080e045"]
]
print(tabulate(table, headers, tablefmt="github"))
print()

# DAO roots

headers = [
    "dao_roots"
]
table = [
    ["0xd6dfd811e06267b25472753c4e57c0b28652bfb8"],
    ["0x5f78fbab81f9892bbe379d88c8a224774411b0a9"]
]
print(tabulate(table, headers, tablefmt="github"))
print()

# proposal roots

headers = [
    "proposal_roots"
]
table = [
    ["0x1430118732f564ec474c4998d94521661143df23"],
    ["0x87611ca3403a3878dfef0da2a786e209abfc1eff"]
]
print(tabulate(table, headers, tablefmt="github"))
print()

# proposal votes

headers = [
    "proposal_votes_index", "yes_votes_commit", "all_votes_commit"
]
table = [
    [72, "xxx", "yyy"]
]
print(tabulate(table, headers, tablefmt="github"))
print()

# vote nullifiers

headers = [
    "proposal_votes_index", "nullifier"
]
table = [
    [72, "aaa"],
    [72, "bbb"],
    [72, "ccc"],
]
print(tabulate(table, headers, tablefmt="github"))
print()

# Base -> ProposalVotes index (proposal_votes)

headers = [
    "base", "proposal_votes_index"
]
table = [
    ["0xa20bfb25ab13a77cc9b50aec28a0b826cee20f88892d087ec1cbc1cbda635d6e", 72],
]
print(tabulate(table, headers, tablefmt="github"))
print()

