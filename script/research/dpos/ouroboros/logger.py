class Logger(object):
    def __init__(self, obj):
        self.obj = obj

    def info(self, payload):
        print(f"\t[{self.obj}]:\n{payload}\n")
    
    def warn(self, payload):
        print(f"\t[{self.obj}]:\n{payload}\n")
    
    def error(self, payload):
        print(f"\t[{self.obj}]:\n{payload}\n")
        exit()
