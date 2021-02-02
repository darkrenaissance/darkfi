;; public params
(def! a_u "15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e")
(def! a_v "015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891")
(def! b_u "15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e")
(def! b_v "015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891")
(def! a "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000")
(def! d "2a9318e74bfa2b48f5fd9207e6bd7fd4292d7f6d37579d2601065fd6d6343eb1")
(def! one "0000000000000000000000000000000000000000000000000000000000000001")
(defzk! circuit ())
;; U should be evaluated just once
(def! U (fn* [x1 y1 x2 y2] (* (+ x1 y1) (+ x2 y2))))
(def! A (fn* [x1 y2] (* y2 x1)))
(def! B (fn* [y1 x2] (* x2 y1)))
(def! C (fn* [x1 y1 x2 y2] (* d (A x1 y2) (B y1 x2))))
(def! P.x (fn* [x1 y1 x2 y2] (/ (+ (A x1 y2) (B y1 x2)) (+ one (C x1 y1 x2 y2)))))
(def! P.y (fn* [x1 y1 x2 y2] (/ (- (U x1 y1 x2 y2) (A x1 y2) (B y1 x2)) (+ one (C x1 y1 x2 y2)))))



;; lc0 = bellman::LinearCombination::<Scalar>::zero();
;; (lc0-args LinearCombination<Scalar>)

;; (cs! circuit (lc0-args) (lc1-args) (lc2-args))

;; (lc-add-coeff 1 1)

(def! jubjub-add (fn* [x1 y1 x2 y2] (cs! circuit (
                    (add lc0 x1)
                    (add lc0 y1)
                    (add lc1 x2)
                    (add lc1 y2)
                    (add lc2 (U x1 y1 x2 y2))
                    enforce
;; Compute P.x = (A + B) / (1 + C)
                    (add-one lc0)
                    (add lc0 (C x1 y1 x2 y2))
                    (add lc1 (P.x x1 y1 x2 y2))
                    (add lc1 (A x1 y2))
                    (add lc1 (B y1 x2))
                    enforce
;; Compute P.y = (U - A - B) / (1 - C)                    
                    (add-one lc0)
                    (sub lc0 (C x1 y1 x2 y2))
                    (add lc1 (P.y x1 y1 x2 y2))
                    (add lc2 (U x1 y1 x2 y2))
                    (sub lc2 (A x1 y2))
                    (sub lc2 (B y1 x2))
                    enforce 
                    ))))
(def! circuit (jubjub-add a_u a_v b_u b_v))
;;(println circuit)
(def! circuit (cs! circuit (
                    (public (P.x a_u a_v b_u b_v)) 
                    (public (P.y a_u a_v b_u b_v)) 
                    (add lc0 (P.x a_u a_v b_u b_v))
                    (add-one lc1)
                    (add lc2 (P.x a_u a_v b_u b_v))
                    enforce
                    (add lc0 (P.y a_u a_v b_u b_v))
                    (add-one lc1)
                    (add lc2 (P.y a_u a_v b_u b_v))
                    enforce
                  )))
;;(println circuit)
;; contract exection
(def! circuit (cs! circuit (
                            (params [a_u a_v b_u b_v])
                            )))

(println circuit)
