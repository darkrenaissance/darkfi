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

config = {
    # IRC server host
    "host": "127.0.0.1",

    # IRC server port
    "port": 6667,

    # IRC nickname
    "nick": "meetbot",
    "channels": [
        {
            "name": "#foo",
            "secret": None,
        },
        {
            "name": "#secret_channel",
            # TODO: This is useless right now, but it would be nice
            # to add a CAP in ircd to give all incoming PRIVMSG to be
            # able to check them.
            "secret": "HNEKcUmwsspdaL9b8sFn45b8Rf3bzv1LdYS1JVNvkPGL",
        },
    ],
}
