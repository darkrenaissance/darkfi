# Discrete Fast Fourier Transform

Available code files:

* [fft2.sage](https://git.dark.fi/darkrenaissance/darkfi/src/master/script/research/zk/fft/fft2.sage):
  implementation using vandermonde matrices illustrating the theory section below.
* [fft3.sage](https://git.dark.fidarkrenaissance/darkfi/src/master/script/research/zk/fft/fft3.sage):
  simple example with $n = 4$ showing 3 steps of the algorithm.
* [fft4.sage](https://git.dark.fi/darkrenaissance/darkfi/src/master/script/research/zk/fft/fft4.sage):
  illustrates the full working algorithm.

## Theory

$$ f(x)g(x) ∈ 𝔽_{<2n}[x] $$
$$ fg = \sum_{i + j < 2n - 2} a_i b_j x^{i + j} $$
Complexity: $O(n^2)$

Suppose $ω ∈ 𝔽$ is an nth root of unity.

Recall: if $𝔽 = 𝔽_{p^k}$ then $∃N : 𝔽_{p^N}$
contains all nth roots of unity.

$$ \DFT_ω : 𝔽^n → 𝔽^n $$
$$ \DFT_ω(f) = (f(ω^0), f(ω^1), …, f(ω^{n - 1})) $$

$$ V_ω =
\begin{pmatrix}
1 & 1   & 1   & \cdots & 1 \\
1 & ω^1 & ω^2 & \cdots & ω^{n - 1} \\
1 & ω^2 & ω^4 & \cdots & ω^{2(n - 1)} \\
\vdots \\
1 & ω^{n - 1} & ω^{2(n - 1)} & \cdots & ω^{(n - 1)^2} \\
\end{pmatrix}
$$

$$ \DFT_ω(f) = V_ω · f^T $$
since vandermonde multiplication is simply evaluation of a polynomial.

### Lemma: $V_ω^{-1} = \frac{1}{n} V_{ω^{-1}}$

Use $1 + ω + ⋯ + ω^{n - 1}$ and compute $V_ω V_{ω^{-1}}$

Corollary: $\DFT_ω$ is invertible.

### Definitions

1. Convolution $f * g = fg \mod (x^n - 1)$
2. Pointwise product
   $$(a_0, …, a_{n - 1})·(b_0, …, b_{n - 1}) =
         (a_0 b_0, …, a_{n - 1} b_{n - 1}) ∈ 𝔽^n → 𝔽_{<n}[x]$$

### Theorem: $\DFT_ω(f*g) = \DFT_ω(f)·\DFT_ω(g)$

$$ fg = q'(x^n - 1) + f*g  $$
$$ ⇒ f*g = fg + q(x^n - 1) $$
$$ \deg fg ≤ 2n - 2        $$

$$
\begin{align}
(f*g)(ω^i) &= f(ω^i)g(ω^i) + q(ω^i)(ω^{in} - 1) \\
           &= f(ω^i)g(ω^i)
\end{align}
$$

### Result

$$ f, g ∈ 𝔽_{<n/2}[x] $$
$$ fg = f*g $$
$$ \DFT_ω(f*g) = \DFT_ω(f)·\DFT_ω(g) $$
$$ fg = \frac{1}{n} \DFT_{ω^{-1}} (\DFT_ω(f) · \DFT_ω(g)) $$

## Finite Field Extension Containing Nth Roots of Unity

$$ μ_N = ⟨ω⟩, |𝔽_{p^N}^×| = p^N - 1 $$
$$ \textrm{ord}(ω) = n | p^N - 1 $$
but $𝔽_{p^N}^×$ is cyclic.

For all $d | p^N - 1$, there exists $x ∈ 𝔽_{p^N}^×$
with ord$(x) = d$.

Finding $n | p^N - 1$ is sufficient for $ω ∈ 𝔽_{p^N}$
$$ n | p^N - 1 ⇔ \textrm{ord}(p) = (ℤ / nℤ)^× $$

## FFT Algorithm Recursive Compute

We recurse to a depth of $\log n$. Since each recursion uses $ω^i$, then
in the final step $ω^i = 1$, and we simply return $f^T$.

We only need to prove a single step of the algorithm produces the desired
result, and then the correctness is inductively proven.

$$
\begin{align}
f(X)    &= a_0 + a_1 X + a_2 X^2 + ⋯ + a_{n - 1} X^{n - 1} \\
        &= g(X) + X^{n/2} h(X)
\end{align}
$$

### Algorithm

Implementation of this algorithm is available in [fft4.sage](https://git.dark.fi/darkrenaissance/darkfi/src/master/script/research/zk/fft/fft4.sage).
Particularly the function called `calc_dft()`.

**function** DFT($n = 2^d, f(X)$)<br>
<span style="padding-left: 30px;">**if** $n = 1$ **then**</span><br>
<span style="padding-left: 60px;">    **return** $f(X)$</span><br>
<span style="padding-left: 30px;">**end**</span><br>
<span style="padding-left: 30px;">Write $f(X)$ as the sum of two polynomials with equal degree</span><br>
<span style="padding-left: 30px;">$f(X) = g(X) + X^{n/2} h(X)$</span><br>
<span style="padding-left: 30px;">Let $\mathbf{g}, \mathbf{h}$ be the vector representations of $g(X), h(X)$</span><br>
<span style="padding-left: 30px;"></span><br>
<span style="padding-left: 30px;">$\mathbf{r} = \mathbf{g} + \mathbf{h}$</span><br>
<span style="padding-left: 30px;">$\mathbf{s} = (\mathbf{g} - \mathbf{h})·(ω^0, …, ω^{n/2 - 1})$</span><br>
<span style="padding-left: 30px;">Let $r(X), s(X)$ be the polynomials represented by the vectors $\mathbf{r}, \mathbf{s}$</span><br>
<span style="padding-left: 30px;"></span><br>
<span style="padding-left: 30px;">Compute $(r(ω^0), …, r(ω^{n/2})) = \textrm{DFT}_{ω^2}(n/2, r(X))$</span><br>
<span style="padding-left: 30px;">Compute $(s(ω^0), …, s(ω^{n/2})) = \textrm{DFT}_{ω^2}(n/2, s(X))$</span><br>
<span style="padding-left: 30px;"></span><br>
<span style="padding-left: 30px;">**return** $(r(ω^0), s(ω^0), r(ω^2), s(ω^2), …, r(ω^{n/2}), s(ω^{n/2}))$</span><br>
end

Sage code:

```python
def calc_dft(ω_powers, f):
    m = len(f)
    if m == 1:
        return f
    g, h = vector(f[:m/2]), vector(f[m/2:])

    r = g + h
    s = dot(g - h, ω_powers)

    ω_powers = vector(ω_i for ω_i in ω_powers[::2])
    rT = calc_dft(ω_powers, r)
    sT = calc_dft(ω_powers, s)

    return list(alternate(rT, sT))
```

### Even Values

$$
\begin{align}
r(X)        &= g(X) + h(X) \\
\\
f(ω^{2i})   &= g(ω^{2i}) + (ω^{2i})^{n/2} h(ω^{2i}) \\
            &= g(ω^{2i}) +                h(ω^{2i}) \\
            &= (g + h)(ω^{2i}) \\
\end{align}
$$

So then we can now compute $DFT_ω(f)_{k=2i} = DFT_{ω^2}(r)$
for the even powers of $f(ω^{2i})$.

### Odd Values

For odd values $k = 2i + 1$

$$
\begin{align}
s(X)        &= (g(X) - h(X))·(ω^0, …, ω^{n/2 - 1}) \\
\\
f(X)          &= a_0 + a_1 X + a_2 X^2 + ⋯ + a_{n - 1} X^{n - 1} \\
              &= g(X) + X^{n/2} h(X) \\
f(ω^{2i + 1}) &= g(ω^{2i + 1}) + (ω^{2i + 1})^{n/2} h(ω^{2i + 1}) \\
\end{align}
$$
But observe that for any $n$th root of unity $ω^n = 1$ and $ω^{n/2} = -1$
$$ (ω^{2i + 1})^{n/2} = ω^{in} ω^{n/2} = ω^{n/2} = -1 $$
$$
\begin{align}
⇒ f(ω^{2i + 1}) &= g(ω^{2i + 1}) - h(ω^{2i + 1}) \\
                &= (g - h)(ω^{2i + 1})
\end{align}
$$

Let $\mathbf{s} = (\mathbf{g} - \mathbf{h})·(ω^0, …, ω^{n/2 - 1})$ be the
representation for $s(X)$. Then we can see that $s(ω^{2i}) = (g - h)(ω^{2i + 1})$
as desired.

So then we can now compute $DFT_ω(f)_{k=2i + 1} = DFT_{ω^2}(s)$
for the odd powers of $f(ω^{2i + 1})$.

## Example

Let $n = 8$
$$
\begin{align}
f(X)    &= (a_0 + a_1 X + a_2 X^2 + a_3 X^3) +     (a_4 X^4 + a_5 X^5 + a_6 X^6 + a_7 X^7) \\
        &= (a_0 + a_1 X + a_2 X^2 + a_3 X^3) + X^4 (a_4     + a_5 X   + a_6 X^2 + a_7 X^3) \\
        &= g(X) + X^{n/2} h(X) \\
g(X)    &=  a_0 + a_1 X + a_2 X^2 + a_3 X^3 \\
h(X)    &=  a_4 + a_5 X + a_6 X^2 + a_7 X^3 \\
\end{align}
$$
Now vectorize $g(X), h(X)$
$$
\begin{align}
\mathbf{g} &= (a_0, a_1, a_2, a_3) \\
\mathbf{h} &= (a_4, a_5, a_6, a_7) \\
\end{align}
$$
Compute reduced polynomials in vector form
$$
\begin{align}
\mathbf{r} &=  \mathbf{g} + \mathbf{h} \\
           &= (a_0 + a_4, a_1 + a_5, a_2 + a_6, a_3 + a_7) \\
\mathbf{s} &= (\mathbf{g} - \mathbf{h})·(1, ω, ω^2, ω^3) \\
           &= (a_0 - a_4, a_1 - a_5, a_2 - a_6, a_3 - a_7)·(1, ω, ω^2, ω^3) \\
           &= (a_0 - a_4, ω (a_1 - a_5), ω^2 (a_2 - a_6), ω^3 (a_3 - a_7)) \\
\end{align}
$$
Convert them to polynomials from the vectors. We also expand them out below
for completeness.
$$
\begin{align}
r(X)       &= r_0 + r_1 X + r_2 X^2 + r_3 X^3 \\
           &= (a_0 + a_4) + (a_1 + a_5) X + (a_2 + a_6) X^2 + (a_3 + a_7) X^3 \\
s(X)       &= s_0 + s_1 X + s_2 X^2 + s_3 X^3 \\
           &= (a_0 - a_4) + ω (a_1 - a_5) X + ω^2 (a_2 - a_6) X^2 + ω^3 (a_3 - a_7) X^3 \\
\end{align}
$$
Compute
$$ \textrm{DFT}_{ω^2}(4, r(X)), \textrm{DFT}_{ω^2}(4, s(X)) $$
The values returned will be
$$
(r(1), s(1), r(ω^2), s(ω^2), r(ω^4), s(ω^4), r(ω^6), s(ω^6))
=
(f(1), f(ω), f(ω^2), f(ω^3), f(ω^4), f(ω^5), f(ω^6), f(ω^7))
$$
Which is the output we return.

### Comparing Evaluations for $f(X)$ and $r(X), s(X)$

We can see the evaluations are correct by substituting in $ω^i$.

We expect that $s(X)$ on the domain $(1, ω^2, ω^4, ω^6)$ produces the values
$(f(1), f(ω^2), f(ω^4), f(ω^6))$, while $r(X)$ on the same domain produces
$(f(ω), f(ω^3), f(ω^5), f(ω^7))$.

#### Even Values

Let $k = 2i$, be an even number. Then note that $k$ is a multiple of 2, so
$4k$ is a multiple of $n ⇒ ω^{4k} = 1$,
$$
\begin{align}
r(X)       &= (a_0 + a_4) + (a_1 + a_5) X + (a_2 + a_6) X^2 + (a_3 + a_7) X^3 \\
r(ω^{2i})  &= (a_0 + a_4) + (a_1 + a_5) ω^{2i} + (a_2 + a_6) ω^{4i} + (a_3 + a_7) ω^{6i} \\
f(ω^k)     &= (a_0 + a_1 ω^k + a_2 ω^{2k} + a_3 ω^{3k}) + ω^{4k} (a_4     + a_5 ω^k   + a_6 ω^{2k} + a_7 ω^{3k}) \\
           &= (a_0 + a_1 ω^k + a_2 ω^{2k} + a_3 ω^{3k}) +        (a_4     + a_5 ω^k   + a_6 ω^{2k} + a_7 ω^{3k}) \\
           &= (a_0 + a_4) + (a_1 + a_5) ω^k + (a_2 + a_6) ω^{2k} + (a_3 + a_7) ω^{3k} \\
           &= f(ω^{2i}) \\
           &= (a_0 + a_4) + (a_1 + a_5) ω^{2i} + (a_2 + a_6) ω^{4i} + (a_3 + a_7) ω^{6i} \\
           &= r(ω^{2i})
\end{align}
$$

#### Odd Values

For $k = 2i + 1$ odd, we have a similar relation where $4k = 8i + 4$, so
$ω^{4k} = ω^4$. But observe that $ω^4 = -1$.
$$
\begin{align}
s(X)       &= (a_0 - a_4) + ω (a_1 - a_5) X + ω^2 (a_2 - a_6) X^2 + ω^3 (a_3 - a_7) X^3 \\
s(ω^{2i})  &= (a_0 - a_4) + (a_1 - a_5) ω^{2i + 1} + (a_2 - a_6) ω^{4i + 2} + (a_3 - a_7) ω^{6i + 3} \\
f(ω^k)     &= (a_0 + a_1 ω^k + a_2 ω^{2k} + a_3 ω^{3k}) + ω^{4k} (a_4     + a_5 ω^k   + a_6 ω^{2k} + a_7 ω^{3k}) \\
           &= (a_0 + a_1 ω^k + a_2 ω^{2k} + a_3 ω^{3k}) -        (a_4     + a_5 ω^k   + a_6 ω^{2k} + a_7 ω^{3k}) \\
           &= f(ω^{2i + 1}) \\
           &= (a_0 + a_1 ω^{2i + 1} + a_2 ω^{4i + 2} + a_3 ω^{6i + 3}) -  (a_4     + a_5 ω^{2i + 1}   + a_6 ω^{4i + 2} + a_7 ω^{6i + 3}) \\
           &= 
           (a_0  - a_4)
           + (a_1 - a_5) ω^{2i + 1}
           + (a_2 - a_6) ω^{4i + 2}
           + (a_3 - a_7) ω^{6i + 3} \\
           &= s(ω^{2i})
\end{align}
$$

