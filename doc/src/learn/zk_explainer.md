# Zero-knowledge explainer

We start with this algorithm as an example:

```python
def foo(w, a, b):
    if w:
        return a * b
    else:
        return a + b
```

ZK code consists of lines of constraints. It has no concept of
branching conditionals or loops.

So our first task is to flatten (convert) the above code to a linear
equation that can be evaluated in ZK.

Consider an interesting fact. For any value $\mathbb{x}$, then
$(1 - w) = 0$ if and only if $x = 1$.

In our code above $w$ is a binary value. It's value is either $1$
or $0$. We make use of this fact by the following:

1. $w = 1$ when $w = 1$
2. $(1 - w) = 1$ when $w = 0$. If $w = 1$ then the expression is $0$.

So we can rewrite `foo(w, a, b)` as the mathematical function

$$f(w, a, b) = w(ab) + (1 - w)(a + b)$$

We now can convert this expression to a constraint system.

ZK statements take the form of:
$$(c_{l,1} \cdot v_{l,1} + c_{a,2} \cdot v_{l,2} + \dots) \times (c_{b,1} \cdot v_{r,1} + c_{b,2} \cdot v_{r,2} + \dots) = (c_{o,1} \cdot v_{o,1} + c_{o,2} v_{o,2} + \dots)$$

More succinctly as:
$$\sum_{i = 1}^n c_{l,i} \cdot v_{l,i} \times \sum_{i = 1}^n c_{r,i} v_{r, i} = \sum_{i = 1}^n c_{o, i} v_{o, i}$$

These statements are converted into polynomials of the form:
$$L(x) \times R(x) - O(x) = t(x)h(x)$$

$t(x)$ is the target polynomial and in our case will be
$(x - 1)(x - 2)(x - 3)$. $h(x)$ is the cofactor polynomial. The
statement says that the polynomial $L(x) \times R(x) - O(x)$ has roots
(is equal to zero) at the points when $x \in {1, 2, 3}$.

Earlier we wrote our mathematical statement which we will now convert
to constraints.

$$f(w, a, b) = w(ab) + (1 - w)(a + b)$$

Rearranging the equation, we note that:

$$ v = w(ab) + (1 - w)(a + b) $$
$$   = w(ab) + a + b - w(a + b) $$

Swapping and rearranging, our final statement becomes
$w(ab - a - b) = v - a - b$. Represented in ZK as:

$$ ab = m $$
$$ w(m - a - b) = v - a - b $$
$$ w^2 = w $$

The last line is a boolean constraint that $w$ is either $0$ or $1$ by
enforcing that $w(w - 1) = 0$ (re-arranged this is $w \cdot w = w$).

| Line      | L(x)               | R(x)                                        | O(x)                                        |
|-----------|--------------------|---------------------------------------------|---------------------------------------------|
| 1         | $(1\cdot a)$       | $(1 \cdot b)$                               | $(1 \cdot m)$                               |
| 2         | $(1 \cdot w)$      | $(1 \cdot m + (-1) \cdot a + (-1) \cdot b)$ | $(1 \cdot v + (-1) \cdot a + (-1) \cdot b)$ |
| 3         | $(1 \cdot w)$      | $(1 \cdot w)$                               | $(1 \cdot w)$                               |

Because of how the polynomials are created during the setup phase, you
must supply them with the correct variables that satisfy these
constraints, so that $L(1) \times R(1) - O(1) = 0$ (line 1),
$L(2) \times R(2) - O(2) = 0$ (line 2) and
$L(3) \times R(3) - O(3) = 0$ (line 3).

Each one of $L(x)$, $R(x)$ and $O(x)$ is supplied a list of
(constant coefficient, variable value) pairs.

In bellman library, the constant is a fixed value of type `Scalar`.
The variable is a type called `Variable`. These are the values fed
into `lc0` (the 'left' polynomial), `lc1` (the 'right' polynomial),
and `lc2` (the 'out' polynomial).

In our example we had a function $f(w, a, b)$ where for example $f(1, 4, 2) = 8$.
The verifier does not know the variables $w = 1$, $a = 4$
and $b = 2$ which are *allocated* by the prover as *variables*. However
the verifier does know the coefficients (which are of the `Scalar`
type) shown in the table above. In our example they only either $1$
or $-1$, but can also be other constant values.

```rust
pub struct LinearCombination<Scalar: PrimeField>(Vec<(Variable, Scalar)>);
```

It is important to note that each one of the left, right and out
registers is simply a list of tuples of (constant coefficient,
variable value).

When we wish to add a constant value, we use the variable called
`~one` (which is always the first automatically allocated variable in
bellman at index 0). Therefore we end up adding our constant $c$ to
the `LinearCombination` as `(c, ~one)`.

Any other non-constant value, we wish to add to our constraint system
*must* be allocated as a variable. Then the variable is added to the
`LinearCombination`. So in our example, we will allocate
$w, a, b, m, v$, getting back `Variable` objects which we then add to
the left lc, right lc or output lc.
