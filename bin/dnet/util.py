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
        with open('config.toml') as f:
            cfg = toml.load(f)
        with open(path, 'w') as f:
            toml.dump(cfg, f)
        print(f"Config file created in {path}. Please review it and try again.")
        sys.exit(0)
        
