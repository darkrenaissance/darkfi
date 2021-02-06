(println "jubjub-add.lisp")
;; Compute U = (u1 + v1) * (v2 - EDWARDS_A*u2)
;;           = (u1 + v1) * (u2 + v2)
( (let* [
      EDWARDS_A (scalar "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000")
      u1 (alloc "u1" (scalar "15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e"))
      v1 (alloc "v1" (scalar "015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891"))
      u2 (alloc "u2" (scalar "15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e"))
      v2 (alloc "v2" (scalar "015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891"))
      U (alloc-input "U" (* (+ u1 u2) (+ v1 v2)))
      A (alloc-input "A" (* v2 u1))
      B (alloc-input "B" (* u2 v1))
      C (alloc-input "C" (* EDWARDS_A (* A B)))
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
  )
  )
 )
)
)
;; (println 'verify  (MyCircuit (scalar 27)))
