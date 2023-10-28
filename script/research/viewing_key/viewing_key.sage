from poseidon import poseidon_hash


def xor(text, key):
    ciphertext = ""
    for i in range(len(text)):
        ciphertext += chr(ord(text[i]) ^^ ord(key[i % len(key)]))

    return ciphertext


p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
Fp = GF(p)
Fq = GF(q)
Ep = EllipticCurve(Fp, (0, 5))
Ep.set_order(q)

nfk_x = 0x25e7aa169ca8198d2e375571faf4c9cf5e7eb192ccb5db9bd36f6aa7e447ca75
nfk_y = 0x155c1f851b1a3384880473442008ff755fe0a49ec1c1b4332db8dce21ae001cc
G = Ep([nfk_x, nfk_y])

# Alice's view key pair
a = Fq.random_element()
A = a * G

# Alice's spend key pair
b = Fq.random_element()
B = b * G

# Each output in a transaction has its own public key
# r is a "transaction secret key, unique to the transaction
# and known only to the sender.
r = Fq.random_element()
R = r * G

# The public key for the output is calculated as such:
rA = r * A
rA_x, rA_y = rA.xy()
P = Fq(int(poseidon_hash([rA_x, rA_y]))) * G + B

# Output value
value = "10000"
# Sender encrypts it
value_enc = xor(value, str(rA))

# A recipient scanning for txs will look at R and calculate
# for themselves what an output would look like if it was
# destined for them:
aR = a * R
aR_x, aR_y = aR.xy()
P_ = Fq(int(poseidon_hash([aR_x, aR_y]))) * G + B
assert P == P_

# Recipient decrypts ciphertext
value_ = xor(value_enc, str(aR))
assert value == value_

# The secret key for spending the output is: H(aR) + b
# And outputs can only be spent by providing a signature
# for the output.
