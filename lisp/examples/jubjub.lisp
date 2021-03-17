(println "jubjub-add.lisp")
(load-file "util.lisp")

(defmacro! zk-square (fn* [var] (
        (let* [v1 (gensym)
               v2 (gensym)] (
        `(alloc ~v1 ~var)
        `(alloc ~v2 (square ~var))
        `(enforce  
            (scalar::one ~v1) 
            (scalar::one ~v1) 
            (scalar::one ~v2) 
        )
        )
    ))
))

;; -u^2 + v^2 = 1 + du^2v^2
(defmacro! zk-witness (fn* [val1 val2] (
        (let* [v (gensym)
               u (gensym)
               u2v2 (gensym)] (
        `(alloc ~v1 ~var)
        `(alloc ~v2 (square ~var))
        `(enforce  
            (scalar::one ~v1) 
            (scalar::one ~v1) 
            (scalar::one ~v2) 
        )
        )
    ))
))

(defmacro! jubjub-add (fn* [param1 param2 param3 param4]
    (let* [u1 (gensym) v1 (gensym) u2 (gensym) v2 (gensym)
           EDWARDS_D (gensym) U (gensym) A (gensym) B (gensym)
           C (gensym) u3 (gensym) v3 (gensym)] (
        `(def! ~u1 (alloc ~u1 param1))
        `(def! ~v1 (alloc ~v1 param2))
        `(def! ~u2 (alloc ~u2 param3))
        `(def! ~v2 (alloc ~v2 param4)) 
        `(def! ~EDWARDS_D (alloc-const ~EDWARDS_D (scalar "2a9318e74bfa2b48f5fd9207e6bd7fd4292d7f6d37579d2601065fd6d6343eb1")))
        `(def! ~U (alloc ~U (* (+ ~u1 ~v1) (+ ~u2 ~v2))))
        `(def! ~A (alloc ~A (* ~v2 ~u1)))
        `(def! ~B (alloc ~B (* ~u2 ~v1)))
        `(def! ~C (alloc ~C (* ~EDWARDS_D (* ~A ~B))))
        `(def! ~u3 (alloc-input ~u3 (/ (+ ~A ~B) (+ scalar::one ~C))))
        `(def! ~v3 (alloc-input ~v3 (/ (- (- ~U ~A) ~B) (- scalar::one ~C))))        
  `(enforce  
    ((scalar::one ~u1) (scalar::one ~v1))
    ((scalar::one ~u2) (scalar::one ~v2))
    (scalar::one ~U)
   )
  `(enforce
    (~EDWARDS_D ~A)
    (scalar::one ~B)
    (scalar::one ~C)
   )
  `(enforce
    ((scalar::one cs::one)(scalar::one ~C))
    (scalar::one ~u3)
    ((scalar::one ~A) (scalar::one ~B))
   )
  `(enforce
    ((scalar::one cs::one) (scalar::one::neg ~C))
    (scalar::one ~v3)
    ((scalar::one ~U) (scalar::one::neg ~A) (scalar::one::neg ~B))
   )
  )
  ;; improve return values
)
))

(prove 
  (
    (def! result-witness (zk-witness param1 param2))
    (println 'result-witness (nth (nth result-witness 0) 1))   
  )
)