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
