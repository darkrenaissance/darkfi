def countZeros(x):
    total_bits = 32
    res = 0
    count = 0
    while ((x & (1 << (total_bits - 1))) == 0) and count < 32:
        x = (x << 1)
        res += 1
        count += 1
    return res
