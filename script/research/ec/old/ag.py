import numpy as np

add_tuple = lambda a, b: tuple(a_i + b_i for a_i, b_i in zip(a, b))

def shift(a, pos):
    shift_x, shift_y = pos
    c = np.zeros(add_tuple(a.shape, (shift_y, shift_x)), dtype=int)
    c[shift_y:,shift_x:] = a
    return c

def max_shape(shape_a, shape_b):
    a_n, a_m = shape_a
    b_n, b_m = shape_b
    return (max(a_n, b_n), max(a_m, b_m))

def add_shape(shape_a, shape_b):
    a_n, a_m = shape_a
    b_n, b_m = shape_b
    return (a_n + b_n, a_m + b_m)

a = np.array([
    [1, 2, 3],
    [7, 8, 9]
])
print(shift(a, (2, 1)))
