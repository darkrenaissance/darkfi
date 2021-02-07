(println "jubjub-add.lisp")
;; Compute U = (u1 + v1) * (v2 - EDWARDS_A*u2)
;;           = (u1 + v1) * (u2 + v2)
( (let* [
      EDWARDS_D (alloc-const "EDWARDS_D" (scalar "2a9318e74bfa2b48f5fd9207e6bd7fd4292d7f6d37579d2601065fd6d6343eb1"))
      u1 (alloc-input "u1" (scalar "15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e"))
      v1 (alloc-input "v1" (scalar "015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891"))
      u2 (alloc-input  "u2" (scalar "15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e"))
      v2 (alloc-input  "v2" (scalar "015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891"))
      U (alloc "U" (* (+ u1 u2) (+ v1 v2)))
      A (alloc "A" (* v2 u1))
      B (alloc "B" (* u2 v1))
      C (alloc "C" (* EDWARDS_D (* A B)))
      ;; Compute u3 = (A + B) / (1 + C)
      u3 (alloc "u3" (/ (+ A B) (+ scalar::one C)))
      ;; Compute v3 = (U - A - B) / (1 - C)
      v3 (alloc "v3" (/ (- (- U A) B) (- scalar::one C)))
      ]
(prove
 (setup 
  (
  (enforce  
    (
     (scalar::one u1)
     (scalar::one v1)
    )
    (
     (scalar::one u2)
     (scalar::one v2)
    )
    (scalar::one U)
  )
  (enforce
    (EDWARDS_D A)
    (scalar::one B)
    (scalar::one C)
  )
  (enforce
    (scalar::one C)
    (scalar::one u3)
    ((scalar::one A) (scalar::one B))
  )
  (enforce
    (scalar::one::neg C)
    (scalar::one v3)
    ((scalar::one::neg U) (scalar::one::neg A) (scalar::one::neg B))
  )
  )
 )
)
)
 )
;; (println 'verify  (MyCircuit (scalar 27)))
