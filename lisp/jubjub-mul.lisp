(println "jubjub-mul.lisp")

(def! param4 (scalar "015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891"))
(def! param3 (scalar "15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e"))
(def! param2 (scalar "015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891"))
(def! param1 (scalar "15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e"))

(setup
    (prove 
      (
    (def! zk-square (fn* [var] (
            (def! result (alloc "square-var" (square var)))
            (enforce  
                (scalar::one square-var) 
                (scalar::one square-var)
                (scalar::one result)
            )
        )
    ))

     (def! u1 (alloc "u1" param1))
     (def! v1 (alloc "v1" param2))
     (def! u2 (alloc "u2" param3))
     (def! v2 (alloc "v2" param4))
     (def! EDWARDS_D (alloc-const "EDWARDS_D" (scalar "2a9318e74bfa2b48f5fd9207e6bd7fd4292d7f6d37579d2601065fd6d6343eb1")))
     (def! U (alloc "U" (* (+ u1 v1) (+ u2 v2))))
     (def! A (alloc "A" (* v2 u1)))
     (def! B (alloc "B" (* u2 v1)))
     (def! C (alloc "C" (* EDWARDS_D (* A B))))
     (def! u3 (alloc-input "u3" (/ (+ A B) (+ scalar::one C))))
     (def! v3 (alloc-input "v3" (/ (- (- U A) B) (- scalar::one C))))
     (println 'square (zk-square param1))
  (
  (enforce  
    ((scalar::one u1) (scalar::one v1))
    ((scalar::one u2) (scalar::one v2))
    (scalar::one U)
  )
  (enforce
    (EDWARDS_D A)
    (scalar::one B)
    (scalar::one C)
  )
  (enforce
    ((scalar::one cs::one)(scalar::one C))
    (scalar::one u3)
    ((scalar::one A) (scalar::one B))
  )
  (enforce
    ((scalar::one cs::one) (scalar::one::neg C))
    (scalar::one v3)
    ((scalar::one U) (scalar::one::neg A) (scalar::one::neg B))
  )
  )
 )
)
)
