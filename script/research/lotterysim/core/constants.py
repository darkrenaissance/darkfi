from decimal import Decimal as Num

# number approximation terms
N_TERM = 2
# analogue controller enum
CONTROLLER_TYPE_ANALOGUE = -1
# discrete controller enum
CONTROLLER_TYPE_DISCRETE = 0
# takahashi controller enum
CONTROLLER_TYPE_TAKAHASHI = 1
# initial distribution of tokens (random value for sake of experimentation)
ERC20DRK = 2.1*10**7
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
EPOCH_LENGTH = Num(10)
# one month in slots
ONE_MONTH = Num(60*60*24*30/SLOT)
# one year in slots
ONE_YEAR = Num(365.25*24*60*60/SLOT)
# vesting issuance period
VESTING_PERIOD = ONE_MONTH
# stakeholder assumes  APR target
TARGET_APR = Num(0.12)
# primary controller assumes accuracy target
PRIMARY_REWARD_TARGET = 0.35 # staked ratio
# secondary controller assumes certain frequency of leaders per slot
SECONDARY_LEAD_TARGET = 1 #number of lead per slot
# maximum transaction size
MAX_BLOCK_SIZE = 1000
# maximum transaction computational cost
MAX_BLOCK_CC = 10
# fee controller computational capacity target
FEE_TARGET = MAX_BLOCK_CC
# max fee base value
FEE_MAX = 1
# min fee base value
FEE_MIN = 0.0001
# negligible value added to denominator to avoid invalid division by zero
EPSILON = 1
# window of accuracy calculation
ACC_WINDOW = int(EPOCH_LENGTH)
# headstart airdrop period
HEADSTART_AIRDROP = 0
# threshold of randomly slashing stakeholder
SLASHING_RATIO = 0.0001
# number of nodes
NODES = 1000
# headstart value
BASE_L = NODES**-1*L*0.01
# decimal high precision.
L_HP = Num(L)
F_MIN_HP = Num(F_MIN)
F_MAX_HP = Num(F_MAX)
EPSILON_HP = Num(EPSILON)
REWARD_MIN_HP = Num(REWARD_MIN)
REWARD_MAX_HP = Num(REWARD_MAX)
BASE_L_HP = Num(BASE_L)
