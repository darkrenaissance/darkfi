---
title: Halo 1 Arithmetization Polynomials Expansion
author: Amir Taaki
header-includes: |
    - \newcommand{\vec}[1]{\mathbf{#1}}
---

Halo 1 constraints consist of multiplication constraints of the form
$$\vec{a_i} \cdot \vec{b_i} = \vec{c_i}$$
and linear addition constraints of the form
$$\left(\sum_{i = 1}^N \vec{a_i} \cdot (\vec{u_q})_i\right) + \left(\sum_{i = 1}^N \vec{b_i} \cdot (\vec{v_q})_i\right) + \left(\sum_{i = 1}^N \vec{c_i} \cdot (\vec{w_q})_i\right) = \vec{k_q}$$

We embed all these constraints using different powers of $Y$ so they are linearly independent.

$$\sum_{i = 1}^N \vec{a_i} \cdot Y^N u_i(Y) + \sum_{i = 1}^N \vec{b_i} \cdot Y^N v_i(Y) + \sum_{i = 1}^N \vec{c_i} \cdot (Y^N w_i(Y) - Y^i - Y^{-i}) + \sum_{i = 1}^N \vec{a_i b_i} \cdot (Y^i + Y^{-i}) - Y^N k(Y) = 0 $$

where we define the polynomials

$$u_i(Y) = \sum_{q = 1}^Q Y^q (\vec{u}_q)_i \qquad v_i(Y) = \sum_{q = 1}^Q Y^q (\vec{v}_q)_i$$
$$w_i(Y) = \sum_{q = 1}^Q Y^q (\vec{w}_q)_i \qquad k(Y) = \sum_{q = 1}^Q Y^q \vec{k}_q$$

\begin{alignat*}{2}
r(X, Y) &= && \sum_{i = 1}^N \vec{a}_i X^i Y^i + \sum_{i = 1} \vec{b}_i X^{-i} Y^{-i} + \sum_{i = 1}^N \vec{c}_i X^{-i - N} Y^{-i - N} \\
&= \; &&\vec{a_1} XY + \vec{a_2} X^2 Y^2 + \cdots + \vec{a_N} X^N Y^N \\
& && + \vec{b_1} X^{-1} Y^{-1} + \vec{b_2} X^{-2} Y^{-2} + \cdots + \vec{b_N} X^{-N} Y^{-N} \\
& && + \vec{c_1} X^{-1 -N} Y^{-1 -N} + \vec{c_2} X^{-2-N} Y^{-2-N} + \cdots + \vec{c_N} X^{-N-N} Y^{-N-N}
\end{alignat*}

\begin{alignat*}{2}
r(X, 1) &= && \sum_{i = 1}^N \vec{a}_i X^i + \sum_{i = 1} \vec{b}_i X^{-i} + \sum_{i = 1}^N \vec{c}_i X^{-i - N} \\
&= \; &&\vec{a_1} X + \vec{a_2} X^2 + \cdots + \vec{a_N} X^N \\
& && + \vec{b_1} X^{-1} + \vec{b_2} X^{-2} + \cdots + \vec{b_N} X^{-N} \\
& && + \vec{c_1} X^{-1 -N} + \vec{c_2} X^{-2-N} + \cdots + \vec{c_N} X^{-N-N}
\end{alignat*}

$$s(X, Y) = \sum_{i = 1}^N u_i(Y)X^{-i} + \sum_{i = 1}^N v_i(Y) X^i + \sum_{i = 1}^N w_i(Y) X^{i + N}$$
\begin{alignat*}{2}
s'(X, Y) &= && \; Y^N s(X, Y) - \sum_{i = 1}^N (Y^i + Y^{-i}) X^{i + N} \\
&= && \sum_{i = 1}^N X^{-i} Y^N u_i(Y) + \sum_{i = 1}^N X^i Y^N v_i(Y) + \sum_{i = 1}^N X^{i + N} Y^N w_i(Y)  - \sum_{i = 1}^N X^{i + N} Y^i - \sum_{i = 1}^N X^{i + N} Y^{-i} \\
&= && \; X^{-1} Y^N u_1(Y) + X^{-2} Y^N u_2(Y) + \cdots + X^{-N} Y^N u_N{Y} \\
& && + X Y^N v_1(Y) + X^2 Y^N v_2(Y) + \cdots + X^N Y^N v_N(Y) \\
& && + X^{N + 1} Y^N w_1(Y) + X^{N + 2} Y^N w_2(Y) + \cdots + X^{N + N} w_N(Y) \\
&= && \; X^{-1} (Y^{1 + N} u_{1,1} + \cdots + Y^{Q + N} u_{Q,1}) \\
& && + X^{-2} (Y^{1 + N} u_{1,2} + \cdots + Y^{Q + N} u_{Q,2}) \\
& && + \cdots \\
& && + X^{-N} (Y^{1 + N} u_{1,N} + \cdots + Y^{Q + N} u_{Q,N}) \\
& && + X (Y^{1 + N} v_{1,1} + \cdots + Y^{Q + N} v_{Q,1}) \\
& && + X^2 (Y^{1 + N} v_{1,2} + \cdots + Y^{Q + N} v_{Q,2}) \\
& && + \cdots \\
& && + X^N (Y^{1 + N} v_{1,N} + \cdots + Y^{Q + N} v_{Q,N}) \\
& && + X^{N + 1} (Y^{1 + N} w_{1,1} + \cdots + Y^{Q + N} w_{Q,1}) \\
& && + X^{N + 2} (Y^{1 + N} w_{1,2} + \cdots + Y^{Q + N} w_{Q,2}) \\
& && + \cdots \\
& && + X^{N + N} (Y^{1 + N} w_{1,N} + \cdots + Y^{Q + N} w_{Q,N}) \\
& && - X^{1 + N} Y - X^{1 + N} Y^{-1} \\
& && - X^{2 + N} Y^2 - X^{2 + N} Y^{-2} \\
& && + \cdots \\
& && - X^{N + N} Y^N - X^{N + N} Y^{-N}
\end{alignat*}

The last expansion above is not necessary for the rest of our argument and is simply included for completeness.

First we compute $r(X, 1) r(X, Y)$ and then $r(X, 1) s'(X, Y)$ to show that the constant argument of $t(X, Y)$ is the left hand side of the combined constraints equation.

We focus only on expanding the terms where powers of $X$ cancel to $0$.

\begin{alignat*}{2}
r(X, 1) r(X, Y) &= && \cdots + \vec{a_1} X \vec{b_1} X^{-1} Y^{-1} + \cdots + \vec{a_2} X^2 \vec{b_2} X^{-2} Y^{-2} + \cdots + \vec{a_N} X^N \vec{b_N} X^{-N} Y^{-N} + \cdots \\
& && + \vec{b_1} X^{-1} \vec{a_1} XY + \cdots + \vec{b_2} X^{-2} \vec{a_2} X^2 Y^2 + \cdots + \vec{b_N} X^{-N} \vec{a_N} X^N Y^N + \cdots \\
&= && \; \cdots + \vec{a_1} \vec{b_1} Y^{-1} + \vec{a_2} \vec{b_2} Y^{-2} + \cdots + \vec{a_N} \vec{b_N} Y^{-N} + \vec{a_1} \vec{b_1} Y + \vec{a_2} \vec{b_2} Y^2 + \cdots + \vec{a_N} \vec{b_N} Y^N \\
&= && \; \sum_{i = 1}^N \vec{a}_i \vec{b}_i (Y_i + Y^{-i})
\end{alignat*}

\begin{alignat*}{2}
r(X, 1) s'(X, Y) &= && \; \vec{a_1} X X^{-1} Y^N u_1(Y) + \vec{a_2} X^2 X^{-2} Y^N u_2(Y) + \cdots + \vec{a_N} X^N X^{-N} Y^N u_N(Y) \\
& && + \vec{b_1} X^{-1} X Y^N v_1(Y) + \vec{b_2} X^{-2} X^2 Y^N v_2(Y) + \cdots + \vec{b_N} X^{-N} X^N Y^N v_N(Y) \\
& && + \vec{c_1} X^{-1-N} X^{N+1} Y^N w_1(Y) + \vec{c_2} X^{-2 - N} X^{N + 2} Y^N w_2(Y) + \cdots + \vec{c_N} X^{-N-N} X^{N + N} Y^N w_N(Y) \\
& && + \vec{c_1} X^{-1-N} (-X^{1 + N} Y - X^{1 + N} Y^{-1}) \\
& && + \vec{c_2} X^{-2-N} (-X^{2+N}Y^2 - X^{2+N}Y^{-2}) \\
& && + \cdots + \vec{c_N} X^{-N-N} (-X^{N+N} Y^N - X^{N+N} Y^{-N}) \\
& && + \cdots \\
&= && \; \vec{a_1} Y^N u_1(Y) + \vec{a_2} Y^N u_2(Y) + \cdots + \vec{a_N} Y^N u_N(Y) \\
& && + \vec{b_1} Y^N v_1(Y) + \vec{b_2} Y^N v_2(Y) + \cdots + \vec{b_N} Y^N v_N(Y) \\
& && + \vec{c_1} Y^N w_1(Y) + \vec{c_2} Y^N w_2(Y) + \cdots + \vec{c_N} Y^N w_N(Y) \\
& && + \vec{c_1} (-Y - Y^{-1}) + \vec{c_2} (-Y^2 - Y^{-2}) + \cdots + \vec{c_N} (-Y^N - Y^{-N}) \\
& && + \cdots \\
&= && \; \sum_{i = 1}^N \vec{a_i} \cdot Y^N u_i(Y) + \sum_{i = 1}^N \vec{b_i} \cdot Y^N v_i(Y) + \sum_{i = 1}^N \vec{c_i} \cdot (Y^N w_i(Y) - Y^i - Y^{-i})
\end{alignat*}
