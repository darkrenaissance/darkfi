#!/usr/bin/env sage

"""
The following code is taken from the examples outlined in the article:

"R1CS and QAP - From Zero to Hero with Finite Fields & sagemath".
(https://risencrypto.github.io/R1CSQAP/). The Sage examples begin at the 
end of the section called "Conversion to Arithemetic Circuits & then R1CS".

Note that this example uses the same polynomial as the file qap.py
in this directory. 
"""

header = """
This script demonstrates the conversion of a set of R1CS matrices to QAP form.
For more information, read the code comments in the file for this script.
"""
print(header)

# Define the R1CS matrices over the finite field GF(41)
F41 = GF(41)
L = Matrix(F41, [
[0,1,0,0,0,0],
[0,0,0,1,0,0],
[0,1,0,0,1,0],
[5,0,0,0,0,1],]
)
R = Matrix(F41, [
[0,1,0,0,0,0],
[0,1,0,0,0,0],
[1,0,0,0,0,0],
[1,0,0,0,0,0,],])
O = Matrix(F41, [
[0,0,0,1,0,0],
[0,0,0,0,1,0],
[0,0,0,0,0,1],
[0,0,1,0,0,0,],])

# Print R1CS matrices
print(L)
print()
print(R)
print()
print(O)
print()

# Debug: this section should print 35*x^3 + 36*x^2 + 16*x + 36
# It represents the Lagrange interpolation for the first column of matrix L.
R41.<x> = PolynomialRing(F41)
points = [(1,0),(2,0),(3,0),(4,5)]
R41.lagrange_polynomial(points)


# Convert matrices L, R, and O to QAP form using Lagrange interpolation, i.e.
# repeat the above lines of code for every column of every matrix.
M = [L, R, O]
PolyM = []
for m in M:
	PolyList = []
	for i in range(m.ncols()):
		points = []
		for j in range(m.nrows()):
			points.append([j+1,m[j,i]])
		Poly = R41.lagrange_polynomial(points).coefficients(sparse=False)

		if(len(Poly) < m.nrows()):
			# if degree of the polynomial is less than 4
			# we add zeroes to represent the missed out terms
			dif = m.nrows() - len(Poly)
			for c in range(dif):
				Poly.append(0);
		PolyList.append(Poly)
	PolyM.append(Matrix(F41, PolyList))

# Print the new matrices containing QAP form polynomials.
# Each matrix should have 6 polynomials. This corresponds to the number of elements
# in the solution vector (which is 6 using the example from the article).
print(PolyM[0])
print()
print(PolyM[1])
print()
print(PolyM[2])
print()

# Solution vector (defined over the finite field). This is known only to the prover.
S = vector(F41,[1, 3, 35, 9, 27, 30]) 
# Create the L, Rx & Ox polynomials. Perform the dot product of the solution vector
# with each of the matrices obtained in the previous step.
Lx = R41(list(S*PolyM[0]))
Rx = R41(list(S*PolyM[1]))
Ox = R41(list(S*PolyM[2]))
print("Lx = " + str(Lx))
print("Rx = " + str(Rx))
print("Ox = " + str(Ox))

# Multiply polynomials Lx and Rx and subtract Ox. This is how we check the
# constraints of the circuit and corresponds to checking the vectors in
# the R1CS step.
# Note: the verifier does not know the polynomial T.
T = Lx*Rx - Ox
print("T(x) = ", end="")
print(T)

# Check Tx at x = 1, 2, 3, 4. The output of each should be 0.
print("T(1) = " + str(T(1)))
print("T(2) = " + str(T(2)))
print("T(3) = " + str(T(3)))
print("T(4) = " + str(T(4)))

# This polynomial is known to both the prover and verifier
Z = R41((x-1)*(x-2)*(x-3)*(x-4))
H = T.quo_rem(Z)
print("Quotient of Z/T = ", end="")
print(H[0])
# The remainder here must be 0, indicating that Z exactly divides T.
print("Remainder of Z/T = ", end="")
print(H[1])
