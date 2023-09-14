'''
bulletproof protocol 2 with multi-exponentiation.
'''
load('../mpc/curve.sage')
load('../mpc/beaver.sage')
load('utils.sage')

class Proof(object):
      def __init__(self, transcript, Q, G_factors, H_factors, G, H, a, b):
          '''
          create inner product proof
          '''
          self.source = Source(p)
          n = len(G)
          assert (n == len(H) == len(H_factors) == len(a) == len(b))
          L_l = []
          R_l = []
          if n!=1:
                n /=2
                a_l, a_r = a[0:n], a[n:]
                b_l, b_r = b[0:n], b[n:]
                G_l, G_r = G[0:n], G[n:]
                H_l, H_r = H[0:n], H[n:]
                c_l = [sum([a*b for a,b in zip(a_l, b_r)])]
                c_r = [sum([a*b for a,b in zip(a_r, b_l)])]
                al_g = [al*g for al, g in zip(a_l, G_factors[n:2*n])]
                br_h = [br*h for br,h in zip(b_r, H_factors[0:n])]
                L_gr_al_g = CurvePoint.msm(G_r, al_g)
                L_hl_br_h = CurvePoint.msm(H_l, br_h)
                L_q_cl = CurvePoint.msm(Q, c_l)
                # L, R
                # note that P  = L*R
                L = [sum([L_gr_al_g, L_hl_br_h , L_q_cl])]
                R = [sum([CurvePoint.msm(G_l, [ar*g for ar, g in zip(a_r, G_factors[0:n])]), CurvePoint.msm(H_r, [bl*h for bl,h in zip(b_l, H_factors[n:2*n])]), CurvePoint.msm(Q, c_r)])]
                L_l += L
                R_l += R

                # choose true random challenges u, u^{-1}
                transcript.append_message(b'L', bytes(''.join([l.__str__() for l in L]), encoding='utf-8'))
                transcript.append_message(b'R', bytes(''.join([r.__str__() for r in R]), encoding='utf-8'))
                u = K(transcript.challenge_bytes(b'u'))
                u_inv = 1/u

                for i in range(n):
                    # a_prime
                    a_l[i] = a_l[i] * u + u_inv * a_r[i]
                    # p_prime
                    b_l[i] = b_l[i] * u_inv + u * b_r[i]
                    # G_prime
                    G_l[i] = CurvePoint.msm([G_l[i], G_r[i]], [u_inv * G_factors[i], u * G_factors[n+i]])
                    # H_prime
                    H_l[i] = CurvePoint.msm([H_l[i], H_r[i]], [u * H_factors[i], u_inv * H_factors[n+i]])
                a = a_l # a is a_prime
                b = b_l # b is b_prime
                G = G_l # G is G_prime
                H = H_l # H is H_prime
          while n!=1:
                n /=2
                a_l, a_r = a[0:n], a[n:] # a_prime_l, a_prime_r
                b_l, b_r = b[0:n], b[n:] # b_prime_l, b_prime_r
                G_l, G_r = G[0:n], G[n:] # G_prime_l, G_prime_r
                H_l, H_r = H[0:n], H[n:] # H_prime_l, H_prime_r

                c_l = [sum([a*b for (a,b) in zip(a_l, b_r)])] # c_prime_l
                c_r = [sum([a*b for (a,b) in zip(a_r, b_l)])] # c_prime_r

                # L_prime
                L = [sum([CurvePoint.msm(G_r, a_l), CurvePoint.msm(H_l, b_r), CurvePoint.msm(Q, c_l)])]

                # R_prime
                R = [sum([CurvePoint.msm(G_l, a_r), CurvePoint.msm(H_r, b_l), CurvePoint.msm(Q, c_r)])]

                L_l += L
                R_l += R

                # choose true random challenges u, u^{-1]}
                transcript.append_message(b'L', bytes(''.join([l.__str__() for l in L]), encoding='utf-8'))
                transcript.append_message(b'R', bytes(''.join([r.__str__() for r in R]), encoding='utf-8'))

                u = K(transcript.challenge_bytes(b'u'))
                u_inv = 1/u
                for i in range(n):
                    # u * a_prime_l + u^{-1} * a_prime_r
                    a_l[i] = a_l[i] * u + u_inv * a_r[i]
                    # u^{-1} * b_prime_l + u * b_prime_r
                    b_l[i] = b_l[i] * u_inv + u * b_r[i]
                    # G_l_prime
                    G_l[i] = CurvePoint.msm([G_l[i], G_r[i]], [u_inv, u])
                    # H_l_prime
                    H_l[i] = CurvePoint.msm([H_l[i], H_r[i]], [u, u_inv])
                a = a_l
                b = b_l
                G = G_l
                H = H_l
          #
          self.lhs = L_l
          self.rhs = R_l
          self.a = a[0]
          self.b = b[0]

      def challenges(self, n, verifier):
          challenges = []
          challenges_inv = []
          lg_n = len(self.lhs)
          for L, R in zip(self.lhs, self.rhs):
              verifier.append_message(b'L', bytes(''.join([l.__str__() for l in [L]]), encoding='utf-8'))
              verifier.append_message(b'R', bytes(''.join([r.__str__() for r in [R]]), encoding='utf-8'))
              u = K(verifier.challenge_bytes(b'u'))
              u_inv = 1/u
              challenges += [u]
              challenges_inv += [1/u]
          inv_prod = K(1)
          for u_inv in challenges_inv:
              inv_prod *=K(1)
          challenges_sq = [i*i for i in challenges]
          challenges_inv_sq = [i*i for i in challenges_inv]
          mul_inv = K(1)
          for i in challenges_inv:
              mul_inv *=i
          S = [mul_inv]
          for i in range(1,n):
              lg_i = 32 - 1 - countZeros(i)
              k = 1 << lg_i
              u_lg_i_sq = challenges_sq[(lg_n -1) - lg_i]
              S += [S[i-k] * u_lg_i_sq]
          return challenges_sq, challenges_inv_sq, S


      def verify(self, n, verifier, G_factors, H_factors, P, Q, G, H):
          u_sq, u_inv_sq, s = self.challenges(n, verifier)
          g_times_a_times_s = [self.a * s_i * g_i for g_i, s_i in zip(G_factors, s)][:n]
          # inverse of count is reverse
          inv_s = reversed(s)
          h_times_b_div_s = [self.b * s_i_inv * h_i for h_i, s_i_inv in zip(H_factors, inv_s)]
          neg_u_sq = [i*K(-1) for i in u_sq]
          neg_u_inv_sq = [i*K(-1) for i in u_inv_sq]
          # P
          ## u^c
          res_p_1 = CurvePoint.msm(Q, [self.a*self.b])
          ## g^{g_factor_a_s}
          res_p_2 = CurvePoint.msm(G, g_times_a_times_s)
          ## h^{h_factor_b_s}
          res_p_3 = CurvePoint.msm(H, h_times_b_div_s)
          # L^(u^2)
          res_p_4 = CurvePoint.msm(self.lhs, neg_u_sq)
          # R^(u^-2)
          res_p_5 = CurvePoint.msm(self.rhs, neg_u_inv_sq)
          # P prime  = L^{u^2} * P * R^{u^{-1}}
          res_p = res_p_1 + res_p_2 + res_p_3 + res_p_4 + res_p_5;
          res = res_p == P
          # P prime == H(u^{-1} * a_prime_r, u * a_prime_l, u * b_prime_r, u ^ {-1} * b_prime_l, c_prime)
          assert (res), 'P: {}, expected P: {}'.format(res_p, P)
          return res_p, P, res
