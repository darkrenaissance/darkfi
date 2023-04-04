from core.utils import *
from pid.pid_base import BasePID

'''
reward primary PID controller.
'''
class RPID(BasePID):
    def __init__(self, controller_type, kp=0, ki=0, kd=0, dt=1,  Kc=0, Ti=0, Td=0, Ts=0, debug=False):
        BasePID.__init__(self, REWARD_MIN, REWARD_MAX, PRIMARY_REWARD_TARGET, controller_type, kp=kp, ki=ki, kd=kd, dt=dt,  Kc=Kc, Ti=Ti, Td=Td, Ts=Ts, debug=debug, type='reward', swap_error_fn=True)


class PrimaryDiscretePID(RPID):
    def __init__(self,  kp, ki, kd):
        RPID.__init__(self, CONTROLLER_TYPE_DISCRETE, kp=kp, ki=ki, kd=kd)

class PrimaryTakahashiPID(RPID):
    def __init__(self, kc, ti, td, ts):
        RPID.__init__(self, CONTROLLER_TYPE_TAKAHASHI, Kc=kc, Ti=ti, Td=td, Ts=ts)

'''
lead secondary PID controller
'''
class LeadPID(BasePID):
    def __init__(self, controller_type, kp=0, ki=0, kd=0, dt=1, Kc=0, Ti=0, Td=0, Ts=0, debug=False):
        BasePID.__init__(self, F_MIN, F_MAX, SECONDARY_LEAD_TARGET, controller_type, kp=kp, ki=ki, kd=kd, dt=dt, Kc=Kc, Ti=Ti, Td=Td, Ts=Ts, debug=debug, type='f')

class SecondaryDiscretePID(LeadPID):
    def __init__(self, kp, ki, kd):
        LeadPID.__init__(self, CONTROLLER_TYPE_DISCRETE, kp=kp, ki=ki, kd=kd)

class SecondaryTakahashiPID(LeadPID):
    def __init__(self, kc, ti, td, ts):
        LeadPID.__init__(self, CONTROLLER_TYPE_TAKAHASHI, Kc=kc, Ti=ti, Td=td, Ts=ts)
