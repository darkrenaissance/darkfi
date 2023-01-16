import itertools, time
from tabulate import tabulate

def find_ext_order(p, n):
    N = 1
    while True:
        pNx_order = p^N - 1

        # Does n divide the group order 𝔽_{p^N}^×?
        if pNx_order % n == 0:
            return N

        N += 1

def find_nth_root_unity(K, p, N, n):
    # It cannot be a quadratic residue if n is odd
    #assert n % 2 == 1

    # So there is an nth root of unity in p^N. Now we have to find it.
    pNx_order = p^N - 1

    ω = K.gens()[0]
    ω = ω^(pNx_order/n)
    assert ω^n == 1
    assert ω^(n - 1) != 1

    return ω

def vectorify(X, f, n):
    assert f.degree() < n
    fT = vector([f[i] for i in range(f.degree() + 1)] +
                # Zero padding
                [0 for _ in range(n - f.degree() - 1)])
    assert len(fT) == n
    # Just check decomposed polynomial is in the correct order
    assert sum([fT[i]*X^i for i in range(n)]) == f
    return fT

def dot(a, b):
    assert len(a) == len(b)
    return [a_i*b_i for a_i, b_i in zip(a, b)]

# ABC, DEF -> ADBECF
def alternate(list1, list2):
    return itertools.chain(*zip(list1, list2))

def calc_dft(n, ω_powers, f):
    m = len(f)
    if m == 1:
        return f
    g, h = vector(f[:m/2]), vector(f[m/2:])

    r = g + h
    s = dot(g - h, ω_powers)

    ω_powers = vector(ω_i for ω_i in ω_powers[::2])
    rT = calc_dft(n, ω_powers, r)
    sT = calc_dft(n, ω_powers, s)

    result = list(alternate(rT, sT))
    return result

def test1():
    p = 199
    #n = 16
    n = 8
    assert p.is_prime()
    N = find_ext_order(p, n)
    print(f"p = {p}")
    print(f"n = {n}")
    print(f"N = {N}")
    print(f"p^N = {p^N}")
    K.<a> = GF(p^N, repr="int")
    ω = find_nth_root_unity(K, p, N, n)
    print(f"ω = {ω}")
    print()

    L.<X> = K[]

    #f = 9*X^7 + 45*X^6 + 33*X^5 + 7*X^3 + X^2 + 110*X + 4
    f = 7*X^3 + X^2 + 110*X + 4
    assert f.degree() < n/2
    print(f"f = {f}")
    print()

    ω_powers = vector(ω^i for i in range(n/2))
    fT = vectorify(X, f, n)
    dft = calc_dft(n, ω_powers, fT)
    print()
    print(f"DFT(f) = {dft}")
    f_evals = [f(X=ω^i) for i in range(n)]
    print(f"f(ω^i) = {f_evals}")

def random_test():
    p = random_prime(1000)
    #n = 16
    n = 2^8
    assert p.is_prime()
    N = find_ext_order(p, n)
    print(f"p = {p}")
    print(f"n = {n}")
    print(f"N = {N}")
    print(f"p^N = {p^N}")
    K.<a> = GF(p^N, repr="int")
    ω = find_nth_root_unity(K, p, N, n)
    print(f"ω = {ω}")
    print()

    L.<X> = K[]

    #f = 9*X^7 + 45*X^6 + 33*X^5 + 7*X^3 + X^2 + 110*X + 4
    f = 0
    for i in range(n/2):
        f += ZZ.random_element(0, 200) * X^i
    assert f.degree() < n/2
    print(f"f = {f}")
    print()

    ω_powers = vector(ω^i for i in range(n/2))
    fT = vectorify(X, f, n)

    start = time.time()
    dft = calc_dft(n, ω_powers, fT)
    dft_duration = time.time() - start

    print(f"DFT time: {dft_duration}")

    start = time.time()
    f_evals = [f(X=ω^i) for i in range(n)]
    eval_duration = time.time() - start

    print(f"Eval time: {eval_duration}")

    #print()
    #print(f"DFT(f) = {dft}")
    #print()
    #print(f"f(ω^i) = {f_evals}")
    assert dft == f_evals

    return dft_duration, eval_duration

def timing_info():
    table = []
    total_dft, total_eval = 0, 0
    success = 0
    for i in range(20):
        print(f"Trial: {i}")
        try:
            dft, eval = random_test()
        except AssertionError:
            table.append((i, "Error", "Error"))
            continue
        table.append((i, dft, eval))
        total_dft += dft
        total_eval += eval
        success += 1
    avg_dft = total_dft / success
    avg_eval = total_eval / success
    table.append(("", "", ""))
    table.append(("Average:", avg_dft, avg_eval))
    print(tabulate(table, headers=["#", "DFT", "Naive"]))

#test1()
#timing_info()
#random_test()

