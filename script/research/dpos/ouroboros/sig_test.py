from ouroboros.utils import *
from ouroboros.block import Block, EmptyBlock

passwd = 'passwd'
empty = EmptyBlock()
block = Block(empty, 'data', 1)

sk, pk = generate_sig_keys(passwd)
message = 'msg'

signature = sign_message(passwd, sk, message)
assert verify_signature(pk, message, signature)