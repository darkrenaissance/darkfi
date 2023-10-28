import os

os.environ['PYTHONHASHSEED'] = '0'
random.seed(0)
class Transcript(object):
    def __init__(self, label):
        self.buffer = [label]
        self.count = 0

    def append_message(self, label, message):
        self.buffer += [label, message]

    def challenge_bytes(self, label):
        buf = str(self.buffer)
        res = hash(buf)
        return res
