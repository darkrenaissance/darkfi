    (def C_D (const "0x2a9318e74bfa2b48f5fd9207e6bd7fd4292d7f6d37579d2601065fd6d6343eb1"))
    (def C_ONE (const "0x0000000000000000000000000000000000000000000000000000000000000001"))
    (def jjadd (fn x1 y1 x2 y2) (
        (def U (mul (add (x1 y1)) (add (x2 y2))))
        (enforce 
            (add_lc0 (x1 y1)) 
            (add_lc1 (x2 y2))
            (add_lc2 U))
        (def A (mul (x2 x1)))
        (def B (mul (x2 y1)))
        (def C (mul (C_ONE A)))
        (enforce
            (add_coeff_lc0 (C_D A))
            (add_lc1 B)
            (add_lc2 C))
        (def Px
            (div (add A B)
                 (add C_ONE C)))
        (enforce 
            (add_one_lc0)
            (sub_lc0 C)
            (add_lc1 Px)
            (add_lc2 (A B)))

        (def Py
            (div (sub U A B)
                 (sub C_ONE C)))
        (enforce 
            (add_one_lc0)
            (sub_lc0 C)
            (add_lc1 Py)
            (add_lc2 (U A B)))
        (Px Py)
    )
    (def input_spend (contract x1 y1 x2 y2) (
        (def P (jjadd (x1 y1 x2 y2)))
        (enforce 
            (add_lc0 Px)
            (add_lc1_one)
            (add_lc2 Px))
        (enforce 
            (add_lc0 Py)
            (add_lc1_one)
            (add_lc2 Py))
    )

