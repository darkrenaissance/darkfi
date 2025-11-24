#!/usr/bin/env python3
# Copyright (c) 2014-2023, The Monero Project
#
# All rights reserved.
#
# Redistribution and use in source and binary forms, with or without modification, are
# permitted provided that the following conditions are met:
#
# 1. Redistributions of source code must retain the above copyright notice, this list of
#    conditions and the following disclaimer.
#
# 2. Redistributions in binary form must reproduce the above copyright notice, this list
#    of conditions and the following disclaimer in the documentation and/or other
#    materials provided with the distribution.
#
# 3. Neither the name of the copyright holder nor the names of its contributors may be
#    used to endorse or promote products derived from this software without specific
#    prior written permission.
#
# THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY
# EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
# MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL
# THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
# SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
# PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
# INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT,
# STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF
# THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
#
# Parts of this file are originally copyright (c) 2012-2013 The Cryptonote developers
import random

DIFFICULTY_TARGET = 120
DIFFICULTY_WINDOW = 720
DIFFICULTY_LAG = 15
DIFFICULTY_CUT = 60

def difficulty():
    times = []
    diffs = []
    while True:
        if len(times) <= 1:
            diff = 1
        else:
            begin = max(len(times) - DIFFICULTY_WINDOW - DIFFICULTY_LAG, 0)
            end = min(begin + DIFFICULTY_WINDOW, len(times))
            length = end - begin
            assert length >= 2
            if length <= DIFFICULTY_WINDOW - 2 * DIFFICULTY_CUT:
                cut_begin = 0
                cut_end = length
            else:
                excess = length - (DIFFICULTY_WINDOW - 2 * DIFFICULTY_CUT)
                cut_begin = (excess + 1) // 2
                cut_end = length - excess // 2
            assert cut_begin + 2 <= cut_end
            wnd = times[begin:end]
            wnd.sort()
            dtime = wnd[cut_end - 1] - wnd[cut_begin]
            dtime = max(dtime, 1)
            ddiff = sum(diffs[begin + cut_begin + 1:begin + cut_end])
            diff = (ddiff * DIFFICULTY_TARGET + dtime - 1) // dtime
        times.append((yield diff))
        diffs.append(diff)


random.seed(1)
time = 1000
gen = difficulty()
diff = next(gen)
for i in range(100000):
    power = 100 if i < 10000 else 100000000 if i < 500 else 1000000000000 if i < 1000 else 1000000000000000 if i < 2000 else 10000000000000000000 if i < 4000 else 1000000000000000000000000
    time += random.randint(-diff // power - 10, 3 * diff // power + 10)
    print(time, diff)
    diff = gen.send(time)
