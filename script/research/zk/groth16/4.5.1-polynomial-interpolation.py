import numpy as np

def lagrange(points):
    result = np.poly1d([0])
    for i, (x_i, y_i) in enumerate(points):
        poly = np.poly1d([y_i])
        for j, (x_j, y_j) in enumerate(points):
            if i == j:
                continue
            poly *= np.poly1d([1, -x_j]) / (x_i - x_j)
        #print(poly)
        #print(poly(1), poly(2), poly(3))
        result += poly
    return result

left = lagrange([
    (1, 2), (2, 2), (3, 6)
])
print(left)

right = lagrange([
    (1, 1), (2, 3), (3, 2)
])
print(right)

out = lagrange([
    (1, 2), (2, 6), (3, 12)
])
print(out)

