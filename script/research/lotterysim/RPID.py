from utils import *

REWARD_MIN = 0
REWARD_MAX = 1000

class RPID:
    def __init__(self, kp=0, ki=0, kd=0, dt=1, target=80, Kc=0, Ti=0, Td=0, Ts=0, debug=False):
        self.Kp = kp # discrete pid kp
        self.Ki = ki # discrete pid ki
        self.Kd = kd # discrete pid kd
        self.T = dt # discrete pid frequency time.
        self.Ti = Ti # takahashi ti
        self.Td = Td # takahashi td
        self.Ts = Ts # takahashi ts
        self.Kc = Kc # takahashi kc
        self.target = target # pid set point, target
        self.prev_feedback = 0
        self.feedback_hist = [0, 0]
        self.f_hist = [0]
        self.error_hist = [0, 0]
        self.debug=debug

    def discrete_pid(self, feedback, debug=True):
        k1 = self.Kp + self.Ki + self.Kd
        k2 = -1 * self.Kp - 2 * self.Kd
        k3 = self.Kd
        err = self.proportional(feedback)
        ret = self.f_hist[-1] + k1 * err + k2 * self.error_hist[-1] + k3 * self.error_hist[-2]
        self.error_hist+=[err]
        self.feedback_hist+=[feedback]
        return ret

    def takahashi(self, feedback, debug=True):
        err = self.proportional(feedback)
        ret = self.f_hist[-1] + self.Kc * (self.feedback_hist[-1] - feedback + self.Ts * err/ self.Ti +  self.Td / self.Ts * (2*self.feedback_hist[-1] - feedback  - self.feedback_hist[-2]))
        self.error_hist+=[err]
        self.feedback_hist+=[feedback]
        return ret

    def pid_clipped(self, feedback, controller=CONTROLLER_TYPE_DISCRETE, debug=True):
        pid_value = None
        if controller == CONTROLLER_TYPE_TAKAHASHI:
            pid_value = self.takahashi(feedback, debug)
        elif controller == CONTROLLER_TYPE_DISCRETE:
            pid_value = self.discrete_pid(feedback, debug)
        else:
            pid_value = self.pid(feedback)

        if pid_value <= REWARD_MIN:
            pid_value = REWARD_MIN # should be corresponding to lowest APY
        if pid_value >= REWARD_MAX:
            pid_value =  REWARD_MAX # should be corresponding to target APY

        self.f_hist+=[pid_value]
        return pid_value

    def error(self, feedback):
        return self.target - feedback

    def proportional(self,  feedback):
        return self.error(feedback)

    def integral(self, feedback):
        return sum(self.feedback_hist[-10:]) + feedback

    def derivative(self, feedback):
        return (self.error(self.prev_feedback) - self.error(feedback)) / self.T

    def write_feedback(self, lead_hist_file):
        if len(self.feedback_hist)==0:
            return
        buf = ''
        buf+=str(self.feedback_hist[0])
        buf+=','
        for i in self.feedback_hist[1:]:
            buf+=str(i)+','
        with open(lead_hist_file, "w+") as f:
            f.write(buf)

    def write_fval(self, f_hist_file):
        if len(self.f_hist)==0:
            return
        buf = ''
        buf+=str(self.f_hist[0])
        buf+=','
        for i in self.f_hist[1:]:
            buf+=str(i)+','
        with open(f_hist_file, "w+") as f:
            f.write(buf)

    def write(self, lead_hist_file='leads.hist', f_hist_file='f.hist'):
        self.write_feedback(lead_hist_file)
        self.write_fval(f_hist_file)

    def acc(self):
        return sum(np.array(self.feedback_hist)==1)/float(len(self.feedback_hist))
