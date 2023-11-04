# Commitment

Darkfi contract uses computationally binding, perfectly hiding pedersen commitment function in both money, and consensus contracts.

cm = comm(m, r), m is data encrypted as curve field element, r is a random curve scalar blinding factor, `comm` is a computationally hiding, computationally binding commitment.

## Curve point commitment

Commitment to a curve point pt is tuple $(cm_x,cm_y)$, after conversion to affine coordinates of pt: $(pt_x, pt_y)$

$$cm_x = comm(pt_x, r_x)$$
$$cm_y = comm(pt_y, r_x)$$
