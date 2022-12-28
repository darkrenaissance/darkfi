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

/// Localnet addresses
pub const LOCALNET: [&str; 5] = ["localhost", "0.0.0.0", "[::]", "127.0.0.1", "[::1]"];

/// Illegal IPv6 addresses
pub const IP6_PRIV_RANGES: [&str; 2] = ["fc00::/7", "fec0::/10"];

/// Illegal IPv4 addresses
pub const IP4_PRIV_RANGES: [&str; 47] = [
    "0.0.0.0/8",
    "10.0.0.0/8",
    "127.0.0.0/8",
    "224.0.0.0/8",
    "225.0.0.0/8",
    "226.0.0.0/8",
    "227.0.0.0/8",
    "228.0.0.0/8",
    "229.0.0.0/8",
    "230.0.0.0/8",
    "231.0.0.0/8",
    "232.0.0.0/8",
    "233.0.0.0/8",
    "234.0.0.0/8",
    "235.0.0.0/8",
    "236.0.0.0/8",
    "237.0.0.0/8",
    "238.0.0.0/8",
    "239.0.0.0/8",
    "240.0.0.0/8",
    "241.0.0.0/8",
    "242.0.0.0/8",
    "243.0.0.0/8",
    "244.0.0.0/8",
    "245.0.0.0/8",
    "246.0.0.0/8",
    "247.0.0.0/8",
    "248.0.0.0/8",
    "249.0.0.0/8",
    "250.0.0.0/8",
    "251.0.0.0/8",
    "252.0.0.0/8",
    "253.0.0.0/8",
    "254.0.0.0/8",
    "255.0.0.0/8",
    "100.64.0.0/10",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.0.0.0/24",
    "192.0.2.0/24",
    "192.88.99.0/24",
    "192.168.0.0/16",
    "198.18.0.0/15",
    "198.51.100.0/24",
    "203.0.113.0/24",
    "233.252.0.0/24",
    "255.255.255.255/32",
];
