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

from decimal import Decimal as Num

# number approximation terms
N_TERM = 5
# analogue controller enum
CONTROLLER_TYPE_ANALOGUE = -1
# discrete controller enum
CONTROLLER_TYPE_DISCRETE = 0
# takahashi controller enum
CONTROLLER_TYPE_TAKAHASHI = 1
# initial distribution of tokens (random value for sake of experimentation)
ERC20DRK = 10000
# initial distribution
PREMINT = ERC20DRK
# group base/order
L = 28948022309329048855892746252171976963363056481941560715954676764349967630337.0
# secondary finalization controller minimal clipped value
F_MIN = 0.0001
# secondary finalization controller maximal clipped value
F_MAX = 0.9999
# primary reward controller minimal clipped value
REWARD_MIN = 1
# primary reward controller maximal clipped value
REWARD_MAX = 1000
# slot length in seconds
SLOT = 90
# epoch length in slots
EPOCH_LENGTH = 10
# one month in slots
ONE_MONTH = 60*60*24*30/SLOT
# one year in slots
ONE_YEAR = 365.25*24*60*60/SLOT
# vesting issuance period
VESTING_PERIOD = ONE_MONTH
# stakeholder assumes  APR target
TARGET_APR = 0.15
# primary controller assumes accuracy target
PRIMARY_REWARD_TARGET = 0.35 # staked ratio
# secondary controller assumes certain frequency of leaders per slot
SECONDARY_LEAD_TARGET = 1 #number of lead per slot
# maximum transaction size
MAX_BLOCK_SIZE = 100
# maximum transaction computational cost
MAX_BLOCK_CC = 10
# fee controller computational capacity target
FEE_TARGET = MAX_BLOCK_CC
# max fee base value
FEE_MAX = 1
# min fee base value
FEE_MIN = 0.00001
# negligible value added to denominator to avoid invalid division by zero
EPSILON = 1
# window of accuracy calculation
ACC_WINDOW = int(EPOCH_LENGTH)*10
# headstart airdrop period
HEADSTART_AIRDROP = 0
# threshold of randomly slashing stakeholder
SLASHING_RATIO = 0.000005
# number of nodes
NODES = 1000
# headstart value
BASE_L = NODES**-1*L
# decimal high precision.
L_HP = Num(L)
F_MIN_HP = Num(F_MIN)
F_MAX_HP = Num(F_MAX)
EPSILON_HP = Num(EPSILON)
REWARD_MIN_HP = Num(REWARD_MIN)
REWARD_MAX_HP = Num(REWARD_MAX)
BASE_L_HP = Num(BASE_L)
CC_DIFF_EPSILON=0.0001
MIL_SLOT = 1000
