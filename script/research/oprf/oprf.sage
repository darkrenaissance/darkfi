# Oblivious Pseudo-Random function

# Constructing an OPRF on elliptic curves is possible if the curve is a
# prime-order group.
Fp = GF(0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001)

input_block = Fp(42)

# Alice generates a random blinding factor r, and multiplies her input block
# with this r
r = Fp.random_element()
alpha = input_block * r

# She sends alpha over to Bob, who has the key k and wants to keep it secret,
# so Bob calculates beta from alpha and the key
#k = Fp.random_element()
k = Fp(69420)
beta = alpha * k

# And then sends back beta to Alice, who can then unblind the result, by
# multiplying beta with 1/r
output_block = beta * (1 / r)

# If we expand the last calculation we can see how r is eliminated by 1/r
# output_block = input_block * r * k * (1/r)
# And by cancelling out r and 1/r we get:
#output_block = input_block * k
print(output_block)
