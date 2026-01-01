# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2026 Dyne.org foundation
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, either version 3 of the
# License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.

import os
import sys
import toml
import platform

def get_os():
   if sys.platform.startswith('java'):
      os_name = platform.java_ver()[3][0]
      if os_name.startswith('Windows'): 
          system = 'win32'
      elif os_name.startswith('Mac'):
          system = 'macOS'
      else: 
          system = 'linux'
   else:
        system = sys.platform
   return system

def user_config_dir(appname, system):
   if system == "win32":
       path = windows_dir(appname)
   elif system == 'macOS':
       path = os.path.expanduser('~/Library/Preferences/')
       path = os.path.join(path, appname)
   else:
       path = os.getenv('XDG_CONFIG_HOME', os.path.expanduser("~/.config"))
       path = os.path.join(path, appname)
   return path

def windows_dir(appname):
   appauthor = appname
   const = "CSIDL_APPDATA"
   path = os.path.normpath(_get_win_folder(const))
   path = os.path.join(path, appname)
   return path

def spawn_config(path):
    file_exists = os.path.exists(path)
    if file_exists:
        with open(path) as f:
            cfg = toml.load(f)
            return cfg
    else:
        with open('dnet_config.toml') as f:
            cfg = toml.load(f)
        with open(path, 'w') as f:
            toml.dump(cfg, f)
        print(f"Config file created in {path}. Please review it and try again.")
        sys.exit(0)
        
