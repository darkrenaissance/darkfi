class Logger(object):
    def __init__(self, obj):
        self.obj = obj
    def info(self, payload):
        print(f"[{self.obj}]: {payload}")
    def warn(self, payload):
        print(f"[{self.obj}]: {payload}")
    def error(self, pyaload):
        print(f"[{self.obj}]: {payload}")
        exit()
