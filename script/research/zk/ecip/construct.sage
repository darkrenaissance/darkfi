load("div.sage")

def construct(points):
    divs = []
    for i in range(0, len(points), 2):
        # Odd last remainder element
        if i + 1 == len(divs):
            P = points[i]
            L = div_line(P, -P)
            Q = points[i]
            divs.append((Q, L))
            break

        P1, P2 = points[i], points[i + 1]
        L = div_line(P1, P2)
        Q = P1 + P2
        divs.append((Q, L))

    # Now apply reduction algorithm repeatedly
    while len(divs) != 1:
        divs2 = []

        if len(divs) % 2 == 1:
            divs2.append(divs[0])
            divs = divs[1:]

        for i in range(0, len(divs), 2):
            assert i + 1 < len(divs)
            Q1, L1 = divs[i]
            Q2, L2 = divs[i + 1]
            ℓ = div_line(Q1, Q2)
            L = ℓ + L1 + L2 - div_line(Q1, -Q1) - div_line(Q2, -Q2)
            Q = Q1 + Q2
            divs2.append((Q, L))

        divs = divs2

    assert len(divs) == 1
    return divs[0][1]

