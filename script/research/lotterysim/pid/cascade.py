/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

from core.utils import *
from pid.pid_base import BasePID

'''
reward primary PID controller.
'''
class RPID(BasePID):
    def __init__(self, controller_type, kp=0, ki=0, kd=0, dt=1,  Kc=0, Ti=0, Td=0, Ts=0, debug=False):
        BasePID.__init__(self, PRIMARY_REWARD_TARGET, REWARD_MIN, REWARD_MAX, controller_type, kp=kp, ki=ki, kd=kd, dt=dt,  Kc=Kc, Ti=Ti, Td=Td, Ts=Ts, debug=debug, type='reward', swap_error_fn=True)

class FeePID(BasePID):
    def __init__(self, kp=0, ki=0, kd=0, dt=1, Kc=0, Ti=0, Td=0, Ts=0, debug=False):
        BasePID.__init__(self, MAX_BLOCK_CC, FEE_MIN, FEE_MAX,  CONTROLLER_TYPE_DISCRETE, kp=kp, ki=ki, kd=kd, dt=dt, Kc=Kc, Ti=Ti, Td=Td, Ts=Ts, debug=debug)


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
        BasePID.__init__(self, SECONDARY_LEAD_TARGET, F_MIN, F_MAX,  controller_type, kp=kp, ki=ki, kd=kd, dt=dt, Kc=Kc, Ti=Ti, Td=Td, Ts=Ts, debug=debug, type='f')

class SecondaryDiscretePID(LeadPID):
    def __init__(self, kp, ki, kd):
        LeadPID.__init__(self, CONTROLLER_TYPE_DISCRETE, kp=kp, ki=ki, kd=kd)

class SecondaryTakahashiPID(LeadPID):
    def __init__(self, kc, ti, td, ts):
        LeadPID.__init__(self, CONTROLLER_TYPE_TAKAHASHI, Kc=kc, Ti=ti, Td=td, Ts=ts)
