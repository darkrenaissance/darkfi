'''
synchronized NTP clock
'''

import ntplib
from time import ctime
import math

class SynchedNTPClock(object):

    def __init__(self, slot_length=60, ntp_server='europe.pool.ntp.org'):
        #TODO how long should be the slot length
        self.slot_length=slot_length
        self.ntp_server = ntp_server
        self.ntp_client = ntplib.NTPClient()
        #TODO validate the server
        # when was darkfi birthday? as seconds since the epoch 
        self.darkfi_epoch=0
        self.offline_cnt=0
    def __repr__(self):
        return 'darkfi time: '+ ctime(self.darkfi_time) + ', current synched time: ' + ctime(self.synched_time)

    def __get_time_stat(self):
        response=None
        success=False
        while not success:
            try:
                response = self.ntp_client.request(self.ntp_server, version=3)
                success=True
            #except ntplib.NTPException as e:
            except:
                pass
                 #print("connection failed: {}".format(e.what()))
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
    def offline_time(self):
        self.offline_cnt+=1
        return self.offline_cnt

    @property
    def slot(self):
        return math.floor(self.darkfi_time/self.slot_length)
