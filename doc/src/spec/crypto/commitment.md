# commitment

darkfi contract uses computationally binding, perfectly hiding pedersen commitment function in both money, and consensus contracts.

cm = comm(m, r), m is data encrypted as curve field element, r a random curve scalar is blinding factor, is a computationally hiding, computationally binding commitment.

## curve point commitment
commitment to a curve point pt after convertion to affine coordinates $pt = (pt_x, pt_y)$
$$cm_x, cm_y = comm(pt) = comm(pt_x, r_x), comm(pt_y, r_y)$$
