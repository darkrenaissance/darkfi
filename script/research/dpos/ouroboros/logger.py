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
