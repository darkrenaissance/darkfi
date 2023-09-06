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

import time
import os
os.system("color")

class Logger(object):
    def __init__(self, obj, genesis_time=time.time()):
        self.obj = obj
        self.genesis=genesis_time

    @property
    def diff(self):
        cur = time.time()
        d = cur - self.genesis
        return round(d,1)

    def info(self, payload):
        print("\033[32m", f"[{self.diff}] - [{type(self.obj).__name__}] {self.obj}:\n\t{payload}\n", "\033[0m")
    
    def highlight(self, payload):
        print("\033[35m", f"[{self.diff}] - [{type(self.obj).__name__}] {self.obj}:\n\t{payload}\n", "\033[0m")
    
    def warn(self, payload):
        print("\033[33m", f"[{self.diff}] - [{type(self.obj).__name__}] {self.obj}:\n\t{payload}\n", "\033[0m")
    
    def error(self, payload):
        print("\033[31m", f"[{self.diff}] - [{type(self.obj).__name__}] {self.obj}:\n\t{payload}\n", "\033[0m")
        exit()
