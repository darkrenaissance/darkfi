R.<x, y, a, b, K> = QQ[]
a

S1 = R.quotient(x - a)

f = (x - a) + b^2
assert f(x=a) == b^2

W1 = b
# This expression is 0 at x = 0
# => (x - a)^1 is a factor
assert (W1^2 - f)(x=a) == 0
# Therefore 0 in the quotient ring
assert S1(W1^2) == S1(f)

W_prev = W1
n = 1

S2 = R.quotient((x - a)^(n + 1))
W_next = W_prev + K*(x - a)^n
# The term k^2*(x - a)^(2*n) disappears in the quotient ring
assert S2(K^2*(x - a)^(2*n)) == 0
assert S2(W_next^2 - f) == S2(W_prev^2 - f + 2*K*(x - a)^n*W_prev)

# Remember from the last step that (x - a)^n is a factor of
# W_prev^2 - f
P = (W_prev^2 - f) / (x - a)^n
k = -P/2

W_next = W_prev + k*(x - a)^n
assert S2(W_next^2) == S2(W_prev^2 - W_prev^3 + f*W_prev)
#assert S(W_next^2 - f) == 0

