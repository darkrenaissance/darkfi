class Logger(object):
    def __init__(self, obj):
        self.obj = obj

    def info(self, payload):
        print(f"\t[{self.obj}]:\n{payload}")
    
    def warn(self, payload):
        print(f"\t[{self.obj}]:\n{payload}")
    
    def error(self, pyaload):
        print(f"\t[{self.obj}]:\n{payload}")
        exit()