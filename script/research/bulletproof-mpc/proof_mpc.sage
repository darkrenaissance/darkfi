'''
bulletproof protocol 2 with multi-exponentiation.
'''
load('../mpc/curve.sage')
load('../mpc/ec_share.sage')
load('../mpc/share.sage')
load('../mpc/beaver.sage')
load('utils.sage')

class MpcProof(object):
      def __init__(self, transcript, Q_generator, G_factors, H_factors, G, H, a_shares, b_shares, source, party_id):
          '''
          create inner product proof
          '''
          self.n = len(G)
          self.m = self.n
          assert (self.n == len(H) == len(H_factors) == len(a_shares) == len(b_shares))
          self.source = source
          self.party_id=party_id
          self.Q = Q_generator
          self.G = G
          self.H = H
          self.G_factors = G_factors
          self.H_factors = H_factors
          self.transcript = transcript
          self.L = []
          self.R = []
          L_l = []
          R_l = []
          self.c_l = []
          self.c_r = []
          self.a_shares_l = []
          self.a_shares_r = []
          self.b_shares_l = []
          self.b_shares_r = []
          self.G_hist = []
          self.H_hist = []
          if self.n!=1:
                self.n /=2
                a_shares_l, a_shares_r = a_shares[0:self.n].copy(), a_shares[self.n:].copy()
                b_shares_l, b_shares_r = b_shares[0:self.n].copy(), b_shares[self.n:].copy()
                self.a_shares_l += [a_shares_l.copy()]
                self.a_shares_r += [a_shares_r.copy()]
                self.b_shares_l += [b_shares_l.copy()]
                self.b_shares_r += [b_shares_r.copy()]
                G_l, G_r = G[0:self.n].copy(), G[self.n:].copy()
                H_l, H_r = H[0:self.n].copy(), H[self.n:].copy()
                self.G_hist+=[[G_l.copy(), G_r.copy()]]
                self.H_hist+=[[H_l.copy(), H_r.copy()]]
                # authenticated inner product
                c_shares_l = [MultiplicationAuthenticatedShares(a_share, b_share, self.source.triplet(self.party_id), self.party_id) for a_share, b_share in zip(a_shares_l, b_shares_r)].copy()
                c_shares_r = [MultiplicationAuthenticatedShares(a_share, b_share, self.source.triplet(self.party_id), self.party_id) for a_share, b_share in zip(a_shares_r, b_shares_l)].copy()
                self.c_l += [c_shares_l]
                self.c_r += [c_shares_r]
                #verifier.append_message(b'L', bytes(''.join([l.__str__() for l in [self.L]]), encoding='utf-8'))
                #verifier.append_message(b'R', bytes(''.join([r.__str__() for r in [self.R]]), encoding='utf-8'))
                #u = K(verifier.challenge_bytes(b'u'))
                u = K(1) #for testing purpose
                u_inv = 1/u

                for i in range(self.n):
                    # a_prime
                    a_shares_l[i] = a_shares_l[i].mul_scalar(u) +  a_shares_r[i].mul_scalar(u_inv)
                    # p_prime
                    b_shares_l[i] = b_shares_l[i].mul_scalar(u_inv) + b_shares_r[i].mul_scalar(u)
                    #TODO (research) get pt from share.
                    # G_prime
                    G_l[i] = to_ec_shares(CurvePoint.msm([G_l[i].share, G_r[i].share], [u_inv * G_factors[i], u * G_factors[self.n+i]]))
                    # H_prime
                    H_l[i] = to_ec_shares(CurvePoint.msm([H_l[i].share, H_r[i].share], [u * H_factors[i], u_inv * H_factors[self.n+i]]))

                a_shares = a_shares_l # a is a_prime
                b_shares = b_shares_l # b is b_prime
                G = G_l # G is G_prime
                H = H_l # H is H_prime

          while self.n!=1:
                self.n /=2
                a_shares_l, a_shares_r = a_shares[0:self.n], a_shares[self.n:] # a_prime_l, a_prime_r
                b_shares_l, b_shares_r = b_shares[0:self.n], b_shares[self.n:] # b_prime_l, b_prime_r
                self.a_shares_l += [a_shares_l.copy()]
                self.a_shares_r += [a_shares_r.copy()]
                self.b_shares_l += [b_shares_l.copy()]
                self.b_shares_r += [b_shares_r.copy()]
                G_l, G_r = G[0:self.n], G[self.n:] # G_prime_l, G_prime_r
                H_l, H_r = H[0:self.n], H[self.n:] # H_prime_l, H_prime_r
                self.G_hist += [[G_l, G_r]]
                self.H_hist += [[H_l, H_r]]
                c_shares_l = [MultiplicationAuthenticatedShares(a_share, b_share, self.source.triplet(self.party_id), self.party_id) for (a_share,b_share) in zip(a_shares_l, b_shares_r)] # c_prime_l
                c_shares_r = [MultiplicationAuthenticatedShares(a_share, b_share, self.source.triplet(self.party_id), self.party_id) for (a_share,b_share) in zip(a_shares_r, b_shares_l)] # c_prime_r
                self.c_l += [c_shares_l]
                self.c_r += [c_shares_r]
                #verifier.append_message(b'L', bytes(''.join([l.__str__() for l in [self.L]]), encoding='utf-8'))
                #verifier.append_message(b'R', bytes(''.join([r.__str__() for r in [self.R]]), encoding='utf-8'))
                #u = K(verifier.challenge_bytes(b'u'))
                u = K(1) # for testing purpose
                u_inv = 1/u
                for i in range(self.n):
                    # u * a_prime_l + u^{-1} * a_prime_r
                    a_shares_l[i] = a_shares_l[i].mul_scalar(u) + a_shares_r[i].mul_scalar(u_inv)
                    # u^{-1} * b_prime_l + u * b_prime_r
                    b_shares_l[i] = b_shares_l[i].mul_scalar(u_inv) + b_shares_r[i].mul_scalar(u)
                    # G_l_prime
                    G_l[i] = to_ec_shares(CurvePoint.msm([G_l[i].share, G_r[i].share], [u_inv, u]))
                    # H_l_prime
                    H_l[i] = to_ec_shares(CurvePoint.msm([H_l[i].share, H_r[i].share], [u, u_inv]))
                a_shares = a_shares_l
                b_shares = b_shares_l
                G = G_l
                H = H_l
          self.a_shares = a_shares[0]
          self.b_shares = b_shares[0]
          self.G = G
          self.H = H

      def create(self, their_c_l_shares, their_c_r_shares):
          '''
          create inner product proof
          '''

          self.c_l = [[my_c_l[i].mul(their_c_l[i].d, their_c_l[i].e) for i in range(len(my_c_l))] for my_c_l, their_c_l in zip(self.c_l, their_c_l_shares)]
          self.c_r = [[my_c_r[i].mul(their_c_r[i].d, their_c_r[i].e) for i in range(len(my_c_r))] for my_c_r, their_c_r in zip(self.c_r, their_c_r_shares)]

          # create L,R for proof validation
          L_l = []
          R_l = []
          counter = 0
          if self.m!=1:
                self.m /= 2
                al_share_g = [al_share.mul_scalar(g) for al_share, g in zip(self.a_shares_l[counter], self.G_factors[self.m:2*self.m])]
                br_share_h = [br_share.mul_scalar(h) for br_share, h in zip(self.b_shares_r[counter], self.H_factors[0:self.m])]
                self.L_gr_al_g_share = MSM(self.G_hist[counter][1], al_share_g, self.source, self.party_id)
                self.L_hl_br_h_share = MSM(self.H_hist[counter][0], br_share_h, self.source, self.party_id)


                self.L_q_cl_share = MSM(self.Q, self.c_l[counter], self.source, self.party_id)
                #self.L_q_cl_share = self.L_hl_br_h_share.copy()
                # L, R
                # note that P  = L*R
                L_shares = [self.L_gr_al_g_share, self.L_hl_br_h_share , self.L_q_cl_share]

                ar_share_g = [ar_share.mul_scalar(g) for ar_share, g in zip(self.a_shares_r[counter], G_factors[0:self.m])]
                bl_share_h = [bl_share.mul_scalar(h) for bl_share, h in zip(self.b_shares_l[counter], H_factors[self.m:2*self.m])]
                self.R_gl_ar_g_share = MSM(self.G_hist[counter][0], ar_share_g, self.source, self.party_id)
                self.R_hr_bl_h_share = MSM(self.H_hist[counter][1], bl_share_h, self.source, self.party_id)
                self.R_q_cr_share = MSM(self.Q, self.c_r[counter], self.source, self.party_id)
                R_shares = [self.R_gl_ar_g_share, self.R_hr_bl_h_share, self.R_q_cr_share]
                L_l += [L_shares]
                R_l += [R_shares]

                counter +=1
          while self.m!=1:
                #TODO
                assert(False)
                self.m /=2
                # L_prime
                L_gr_al_share = MSM(self.G_hist[counter][1], self.a_shares_l[counter], self.source, self.party_id)
                L_hl_br_share = MSM(self.H_hist[counter][0], self.b_shares_r[counter], self.source, self.party_id)
                L_q_cl_share = MSM(self.Q, self.c_l[counter], self.source, self.party_id)
                L_shares = [L_gr_al_share, L_hl_br_share, L_q_cl_share]

                # R_prime
                R_gl_ar_share = MSM(self.G_hist[counter][0], a_shares_r, self.source, self.party_id)
                R_hr_bl_share = MSM(self.H_hist[counter][1], b_shares_l, self.source, self.party_id)
                R_q_cr_share = MSM(self.Q, self.c_r[counter], self.source, self.party_id)
                R_shares = [R_gl_ar_share, R_hr_bl_share, R_q_cr_share]

                L_l += [L_shares]
                R_l += [R_shares]

                counter +=1
          #
          self.lhs = L_l
          self.rhs = R_l

      def challenges(self, n, verifier):
          challenges = []
          challenges_inv = []
          lg_n = len(self.lhs)
          for L, R in zip(self.lhs, self.rhs):
              #verifier.append_message(b'L', bytes(''.join([l.__str__() for l in [L]]), encoding='utf-8'))
              #verifier.append_message(b'R', bytes(''.join([r.__str__() for r in [R]]), encoding='utf-8'))
              #u = K(verifier.challenge_bytes(b'u'))
              u = K(1) # for testing purpose
              u_inv = 1/u
              challenges += [u]
              challenges_inv += [u_inv]
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

      def calculate_c_shares(self, n, verifier, G_factors, H_factors):
          self.u_sq, self.u_inv_sq, self.s = self.challenges(n, verifier)
          self.gas_shares = [self.a_shares.mul_scalar(s_i * g_i) for g_i, s_i in zip(G_factors, self.s)][:n]
          # inverse of count is reverse
          self.inv_s = reversed(self.s)
          self.hbs_shares = [self.b_shares.mul_scalar(s_i_inv * h_i) for h_i, s_i_inv in zip(H_factors, self.inv_s)]
          self.neg_u_sq = [i*K(-1) for i in self.u_sq]
          self.neg_u_inv_sq = [i*K(-1) for i in self.u_inv_sq]
          # P
          ## u^c
          self.my_c_shares = [MultiplicationAuthenticatedShares(a_share, b_share, self.source.triplet(self.party_id), self.party_id) for a_share, b_share in zip([self.a_shares], [self.b_shares])]

      def open_lr(self, Q, G, H, their_c_shares_de, peer_lhs, peer_rhs):
          c_shares = [my_c_share.mul(their_c_shares_de[i][0], their_c_shares_de[i][1]) for i, my_c_share in enumerate(self.my_c_shares)]

          self.res_p_1 = MSM(Q, c_shares, self.source, self.party_id)
          ## g^{g_factor_a_s}
          self.res_p_2 = MSM(G, self.gas_shares, self.source, self.party_id)
          ## h^{h_factor_b_s}
          self.res_p_3 = MSM(H, self.hbs_shares, self.source, self.party_id)
          ## L
          for my_lhs, their_lhs in zip(self.lhs, peer_lhs):
                L_triad = []
                for my_lhs_i, their_lhs_i in zip(my_lhs, their_lhs):
                      my_lhs_i_de = [[ps.d, ps.e] for ps in my_lhs_i.point_scalars]
                      their_lhs_i_de = [[ps.d, ps.e] for ps in their_lhs_i.point_scalars]
                      lhs_i_share = my_lhs_i.msm(their_lhs_i_de)
                      L_triad += [lhs_i_share]
                self.L += [sum_shares(L_triad, self.source, self.party_id)]
          ## R
          for my_rhs, their_rhs in zip(self.rhs, peer_rhs):
                R_triad = []
                for my_rhs_i, their_rhs_i in zip(my_rhs, their_rhs):
                      my_rhs_i_de = [[ps.d, ps.e] for ps in my_rhs_i.point_scalars]
                      their_rhs_i_de = [[ps.d, ps.e] for ps in their_rhs_i.point_scalars]
                      rhs_i_share = my_rhs_i.msm(their_rhs_i_de)
                      R_triad += [rhs_i_share]
                self.R += [sum_shares(R_triad, self.source, self.party_id)]
          # L^(u^2)
          temp = K(0)
          self.res_p_4 = MSM(self.L, [AuthenticatedShare(temp, self.source, self.party_id) if self.party_id==0 else AuthenticatedShare(neg_u_sq_i-temp, self.source, self.party_id) for neg_u_sq_i in self.neg_u_sq], self.source, self.party_id)
          # R^(u^-2)
          self.res_p_5 = MSM(self.R, [AuthenticatedShare(temp, self.source, self.party_id) if self.party_id==0 else AuthenticatedShare(neg_u_inv_sq_i-temp, self.source, self.party_id) for neg_u_inv_sq_i in self.neg_u_inv_sq], self.source, self.party_id)
          # P prime  = L^{u^2} * P * R^{u^{-1}}
          self.res_p = [self.res_p_1, self.res_p_2, self.res_p_3, self.res_p_4, self.res_p_5]

      def open_and_validate_P(self, res_p, P):
          P_msm_parts = []
          for my_res_p, their_res_p in zip(self.res_p, res_p):
                my_res_de = [[ps.d, ps.e] for ps in my_res_p.point_scalars]
                their_res_de = [[ps.d, ps.e] for ps in their_res_p.point_scalars]
                lhs = my_res_p.msm(their_res_de)
                rhs = their_res_p.msm(my_res_de)
                p_part = lhs.authenticated_open(rhs)
                P_msm_parts += [p_part]

          # P prime == H(u^{-1} * a_prime_r, u * a_prime_l, u * b_prime_r, u ^ {-1} * b_prime_l, c_prime)
          my_P = sum(P_msm_parts)
          assert (my_P == P), 'P: {}, expected: {}'.format(my_P, P)
