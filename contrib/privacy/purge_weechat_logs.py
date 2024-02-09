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
#
# This script is supposed to be executed with cron
# (https://en.wikipedia.org/wiki/Cron)
#
# This script also assumes that your weechat logfile mask is:
# /set logger.mask.irc "irc/$server/%Y-%m/%d-$channel.weechatlog"
#
# Once you're set up, remember to change dryrun=True to False to
# actually make the script remove files.
import os
import re
import sys
from datetime import datetime, timedelta
from os.path import join, isfile

# Full path to your weechat IRC log directory
LOG_BASE = "/home/user/.local/share/weechat/logs/irc"
pattern = re.compile(r'(\d{4}-\d{2})/(\d{2})-(.+).weechatlog$')

# Delete logs older than this many days
# In your crontab, you can set it to run every day at 2pm:
# 0 14 * * * /usr/bin/python3 /full/path/to/this/script.py
PURGE_END = datetime.now() - timedelta(days=7)

# Map of "server": ["#channel0", "#channel1", "user"] to purge.
PURGE_MAP = {
    "darkfi": ["#dev", "#math", "god"],
    "libera": ["##rust"],
}


def main(dryrun=True):
    to_delete = []

    for server, channels in PURGE_MAP.items():
        for dirpath, _dirnames, filenames in os.walk(join(LOG_BASE, server)):
            for filename in filenames:
                full_path = join(dirpath, filename)

                match = pattern.search(full_path)
                if match:
                    date_str = "-".join(match.groups()[:-1])
                    channel = match.groups()[-1]

                    if channel not in channels:
                        continue

                    file_date = datetime.strptime(date_str, "%Y-%m-%d")

                    if file_date < PURGE_END:
                        to_delete.append(full_path)

    if dryrun:
        for fname in to_delete:
            print(f"[DRYRUN] Deleting: {fname}")
        return 0

    for fname in to_delete:
        if isfile(fname):
            try:
                os.remove(fname)
            except:
                print(f"[WARNING] Failed to remove {fname}")
                continue

    return 0


if __name__ == "__main__":
    sys.exit(main())
