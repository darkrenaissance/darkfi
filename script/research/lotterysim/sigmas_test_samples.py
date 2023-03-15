'''
generating a test case for sigmas in pallas field.

the output in csv format: f,Sigma,sigma1,sigma2 respectively for 2-term approximation.
'''

from lottery import *
import numpy as np

SEP=','

def calc_sigmas(f, Sigma):
    k=N_TERM
    x = (1-f)
    c = math.log(x)
    neg_c = -1*c
    sigmas = [(int((neg_c/Sigma)**i * (L/fact(i)))) for i in range(1, k+1)]

    return sigmas


sigmas = calc_sigmas(0.5, 1000)
assert(len(sigmas)==2)

with open("pallas_unittests.csv", 'w') as file:
    buf = ''
    for f in np.arange(0.01, 0.99, 10):
        for total_stake in np.arange(100, 1000, 10):
            sigmas = calc_sigmas(f, total_stake)
            line=str(f) + SEP + \
                str(total_stake) + SEP + \
                '{:x}'.format(sigmas[0]) + SEP + \
                '{:x}'.format(sigmas[1]) + '\n'
            buf+=line
    file.write(buf)
