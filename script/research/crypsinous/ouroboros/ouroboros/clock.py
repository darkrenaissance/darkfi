'''
synchronized NTP clock
'''

import ntplib
import time
import math

class SynchedNTPClock(object):

    def __init__(self, epoch_length, slot_length=60, ntp_server='europe.pool.ntp.org'):
        #TODO how long should be the slot length
        self.epoch_length=epoch_length # how many slots in a block
        self.slot_length=slot_length
        self.ntp_server = ntp_server
        self.ntp_client = ntplib.NTPClient()
        #TODO validate the server
        # when was darkfi birthday? as seconds since the epoch 
        self.darkfi_epoch=time.mktime(time.strptime("2022-01-01", "%Y-%m-%d"))
        self.offline_cnt=0

    def __repr__(self):
        return 'darkfi time: '+ time.ctime(self.darkfi_time) + ', current synched time: ' + ctime(self.synched_time)

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
        stime = state.tx_time
        return stime

    @property
    def darkfi_time(self):
        return self.synched_time - self.darkfi_epoch

    @property
    def offline_time(self):
        self.offline_cnt+=1
        return self.offline_cnt

    @property
    def slot(self):
        return math.floor(self.offline_time/self.slot_length)

    @property
    def epoch(self):
        return math.floor(self.slot/self.epoch_length)
    
    @property
    def epoch_slot(self):
        return self.slot%self.epoch_length
