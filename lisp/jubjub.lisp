(def! D "0x2a9318e74bfa2b48f5fd9207e6bd7fd4292d7f6d37579d2601065fd6d6343eb1")
(def! ONE "0x0000000000000000000000000000000000000000000000000000000000000001")
(def! jj-add (fn* [x1 y1 x2 y2] (
    (def! U (mul (add (x1 y1)) (add (x2 y2))))
    (enforce (add-lc0 (x1 y1)) (add-lc1 (x2 y2)) (add-lc2 U))
    (def! A (mul (x2 x1)))
    (def! B (mul (x2 y1)))
    (def! C (mul (ONE A)))
    (enforce (add-coeff-lc0 (D A)) (add-lc1 B) (add-lc2 C))
    (def! Px (div (add A B) (add C_ONE C)))
    (enforce (add-one-lc0) (sub-lc0 C) (add-lc1 Px) (add-lc2 (A B)))

    (def! Py (div (sub U A B) (sub C_ONE C))) 
    (enforce (add-one-lc0) (sub-lc0 C) (add-lc1 Py) (add-lc2 (U A B)))
    (Px Py)
)))
;;    (def input_spend (contract x1 y1 x2 y2) (
;;        (def P (jjadd (x1 y1 x2 y2)))
;;        (enforce 
;;            (add_lc0 Px)
;;            (add_lc1_one)
;;            (add_lc2 Px))
;;        (enforce 
;;            (add_lc0 Py)
;;            (add_lc1_one)
;;            (add_lc2 Py))
;;    )
