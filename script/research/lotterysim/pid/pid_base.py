from core.utils import *
import os

'''
base discrete/takahashi PID controller
'''
class BasePID:
    def __init__(self, target, clip_min, clip_max, controller_type, kp=0, ki=0, kd=0, dt=1, Kc=0, Ti=0, Td=0, Ts=0, debug=False, type='base', swap_error_fn=False):
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
        self.output_hist = [0]
        self.error_hist = [0, 0]
        self.debug=debug
        self.clip_min = clip_min
        self.clip_max = clip_max
        self.controller_type = CONTROLLER_TYPE_DISCRETE
        self.swap_error_fn = swap_error_fn
        self.type = type

    def continuous_pid(self, feedback):
        ret =  (self.Kp * self.proportional(feedback)) + (self.Ki * self.integral(feedback)) + (self.Kd * self.derivative(feedback))
        self.feedback_hist+=[feedback]
        self.prev_feedback=feedback
        return ret

    def discrete_pid(self, feedback, debug=True):
        k1 = self.Kp + self.Ki + self.Kd
        k2 = -1 * self.Kp - 2 * self.Kd
        k3 = self.Kd
        err = self.proportional(feedback)
        ret = self.output_hist[-1] + k1 * err + k2 * self.error_hist[-1] + k3 * self.error_hist[-2]
        self.error_hist+=[err]
        self.feedback_hist+=[feedback]
        return ret

    def zero_feedback_hist(self):
        count = 0
        length = len(self.feedback_hist)
        for i in range(0,length):
            if self.feedback_hist[length-(i+1)]==0:
                count+=1
            else:
                return count
        return count

    def takahashi(self, feedback, debug=True):
        err = self.proportional(feedback)
        ret = self.output_hist[-1] + self.Kc * (self.feedback_hist[-1] - feedback + self.Ts * err/ self.Ti +  self.Td / self.Ts * (2*self.feedback_hist[-1] - feedback  - self.feedback_hist[-2]))
        self.error_hist+=[err]
        self.feedback_hist+=[feedback]
        return ret

    def pid_clipped(self, feedback, debug=True):
        pid_value = None
        if self.controller_type == CONTROLLER_TYPE_TAKAHASHI:
            pid_value = self.takahashi(feedback, debug)
        elif self.controller_type == CONTROLLER_TYPE_DISCRETE:
            pid_value = self.discrete_pid(feedback, debug)
        else:
            pid_value = self.continuous_pid(feedback)

        print('[{}-{}]'.format(self.clip_min, self.clip_max))
        if pid_value <= self.clip_min:
            pid_value = self.clip_min
        if pid_value >= self.clip_max:
            pid_value =  self.clip_max

        if self.integral(feedback) == 0 and len(self.feedback_hist) >=3 and self.feedback_hist[-1] == 0 and self.feedback_hist[-2] == 0 and self.feedback_hist[-3] == 0:
            pid_value = 0.9**self.zero_feedback_hist()

        self.output_hist+=[pid_value]
        return pid_value

    def error(self, feedback):
        if self.swap_error_fn:
            # maintain positive proportional gains
            return self.target - feedback
        else:
            return feedback - self.target

    def proportional(self,  feedback):
        return self.error(feedback)

    def integral(self, feedback):
        return sum(self.feedback_hist[-10:]) + feedback

    def derivative(self, feedback):
        return (self.error(self.prev_feedback) - self.error(feedback)) / self.T

    def write_feedback(self, feedback_hist_file):
        if len(self.feedback_hist)==0:
            return
        buf = ''
        buf+=str(self.feedback_hist[0])
        buf+=','
        for i in self.feedback_hist[1:]:
            buf+=str(i)+','
        with open(feedback_hist_file, "w+") as f:
            f.write(buf)

    def write_fval(self, output_hist_file):
        if len(self.output_hist)==0:
            return
        buf = ''
        buf+=str(self.output_hist[0])
        buf+=','
        for i in self.output_hist[1:]:
            buf+=str(i)+','
        with open(output_hist_file, "w+") as f:
            f.write(buf)

    def write(self, feedback_hist_file='_feedback.hist', output_hist_file='_output.hist'):
        self.write_feedback('log' + os.sep + self.type+feedback_hist_file)
        self.write_fval('log'+ os.sep + self.type+output_hist_file)

    def acc(self):
        return sum(np.array(self.feedback_hist)==1)/float(len(self.feedback_hist))

    def acc_percentage(self):
        return self.acc() * 100
