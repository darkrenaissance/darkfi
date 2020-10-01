from finite_fields.modp import IntegersModP

q = 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001
modq = IntegersModP(q)

print("0x%x" % modq(2).inverse().n)
inv2 = 0x39f6d3a994cebea4199cec0404d0ec02a9ded2017fff2dff7fffffff80000001
assert modq(2).inverse().n == inv2
print((2 * inv2) % q)

