from finite_fields.modp import IntegersModP

q = 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001
modq = IntegersModP(q)

a = modq(-1)
d = -(modq(10240)/modq(10241))
params = (a, d)

def is_jubjub(params, x, y):
    a, d = params

    return a * x**2 + y**2 == 1 + d * x**2 * y**2

def add(params, point_1, point_2):
    # From here: https://z.cash/technology/jubjub/

    a, d = params

    x1, y1 = point_1
    x2, y2 = point_2

    x3 = (x1 * y2 + y1 * x2) / (1 + d * x1 * x2 * y1 * y2)
    y3 = (y1 * y2 + x1 * x2) / (1 - d * x1 * x2 * y1 * y2)

    return (x3, y3)

x = 0x15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e
y = 0x015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891

x3, y3 = add(params, (x, y), (x, y))
print(hex(x3.n), hex(y3.n))

print(is_jubjub(params, x, y))
print(is_jubjub(params, x3, y3))

