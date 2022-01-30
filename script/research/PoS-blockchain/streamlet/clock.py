'''
synchronized clock
'''

import ntplib
from time import ctime
import math

class Clock(object):
    def __init__(self, epoch_length=180, ntp_server='europe.pool.ntp.org'):
        self.epoch_length=epoch_length #2 minutes
        self.ntp_server = ntp_server
        self.ntp_client = ntplib.NTPClient()
        #TODO validate the server
        # when was darkfi birthday? as seconds since the epoch 
        self.darkfi_epoch=0
        self.observers = []
    def __repr__(self):
        return 'darkfi time: '+ ctime(self.darkfi_time) + ', current synched time: ' + ctime(self.synched_time)

    def __get_time_stat(self):
        response=None
        success=True
        while not success:
            try:
                response = self.ntp_client.request(self.ntp_server, version=3)
                success=True
            except ntplib.NTPException as e:
                 print("connection failed: {}".format(e.what()))
        return response
    @property
    def synched_time(self):
        state = self.__get_time_stat()
        synched_time = state.tx_time
        return synched_time

    @property
    def darkfi_time(self):
        return self.synched_time - self.darkfi_epoch

    @property
    def epoch(self):   
        return math.floor(self.darkfi_time/self.epoch_length)

    def bind(self, callback):
        self.observers.append((callback))

    def background(self):
        current_epoch = self.epoch
        while True:
            if self.epoch !=current_epoch:
                current_epoch = self.epoch
                for obs in self.observers:
                    obs(current_epoch)