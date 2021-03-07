(load-file "util.lisp")

(defmacro! zk-nonzero? (fn* [var] (
        (let* [inv (gensym)
               v1 (gensym)] (
        `(alloc ~inv (invert ~var))
        `(alloc ~v1 ~var)
        `(enforce  
            (scalar::one ~v1) 
            (scalar::one ~inv) 
            (scalar::one cs::one) 
         )
        ;; { "result" `(zero? ~var) }
        )
    ))
))

(defmacro! zk-square (fn* [var] (
        (let* [v1 (gensym)
               v2 (gensym)] (
        `(alloc ~v1 ~var)
        `(def! output (alloc-input ~v2 (square ~var)))
        `(enforce  
            (scalar::one ~v1) 
            (scalar::one ~v1) 
            (scalar::one ~v2) 
         )
        `{ "v2" output }
        )
    ))
))

(defmacro! zk-mul (fn* [val1 val2] (
        (let* [v1 (gensym)
               v2 (gensym)
               var (gensym)] (
        `(alloc ~v1 ~val1)
        `(alloc ~v2 ~val2)
        `(def! result (alloc-input ~var (* ~val1 ~val2)))
        `(enforce  
            (scalar::one ~v1) 
            (scalar::one ~v2) 
            (scalar::one ~var) 
         )
        `{ "result" result }
        )
    ))
))

(defmacro! zk-witness (fn* [val1 val2] (
        (let* [u2 (gensym)
               v2 (gensym)
               u2v2 (gensym)
               EDWARDS_D (gensym)] (
        `(def! ~EDWARDS_D (alloc-const ~EDWARDS_D (scalar "2a9318e74bfa2b48f5fd9207e6bd7fd4292d7f6d37579d2601065fd6d6343eb1")))
        `(def! ~u2 (alloc ~u2 (get (nth (nth (zk-square ~val1) 0) 3) "v2")))
        `(def! ~v2 (alloc ~v2 (get (nth (nth (zk-square ~val2) 0) 3) "v2")))
        `(def! result (alloc-input ~u2v2 (get (last (last (zk-mul ~u2 ~v2))) "result")))        
        `(enforce  
            ((scalar::one::neg ~u2) (scalar::one ~v2))
            (scalar::one cs::one)
            ((scalar::one cs::one) (~EDWARDS_D ~u2v2))
         )
        `{ "result" result }
        )
    ))
))


(def! zk-not-small-order? (fn* [u v] (
        (def! first-doubling (last (last (zk-double u v))))
        (def! second-doubling (last (last 
            (zk-double (get first-doubling "u3") (get first-doubling "v3")))))
        (def! third-doubling (last (last 
            (zk-double (get second-doubling "u3") (get second-doubling "v3")))))
        (zk-nonzero? (get third-doubling "u3"))
        )
    )
)


(defmacro! zk-double (fn* [val1 val2] (
        (let* [u (gensym)
               v (gensym)
               u3 (gensym)
               v3 (gensym)
               T (gensym)
               A (gensym)
               C (gensym)
               EDWARDS_D (gensym)] (
        `(def! ~EDWARDS_D (alloc-const ~EDWARDS_D (scalar "2a9318e74bfa2b48f5fd9207e6bd7fd4292d7f6d37579d2601065fd6d6343eb1")))
        `(def! ~u (alloc ~u ~val1))
        `(def! ~v (alloc ~v ~val2))
        `(def! ~T (alloc ~T (* (+ ~val1 ~val2) (+ ~val1 ~val2))))
        `(def! ~A (alloc ~A (* ~u ~v)))
        `(def! ~C (alloc ~C (* (square ~A) ~EDWARDS_D)))
        `(def! ~u3 (alloc-input ~u3 (/ (double ~A) (+ scalar::one ~C))))
        `(def! ~v3 (alloc-input ~v3 (/ (- ~T (double ~A)) (- scalar::one ~C))))
        `(enforce  
            ((scalar::one ~u) (scalar::one ~v))
            ((scalar::one ~u) (scalar::one ~v))
            (scalar::one ~T)
         )
         `(enforce  
            (~EDWARDS_D ~A)
            (scalar::one ~A)
            (scalar::one ~C)
         )
         `(enforce  
            ((scalar::one cs::one) (scalar::one ~C))
            (scalar::one ~u3)
            ((scalar::one ~A) (scalar::one ~A))    
         )
         `(enforce  
            ((scalar::one cs::one) (scalar::one::neg ~C))
            (scalar::one ~v3)
            ((scalar::one ~T) (scalar::one::neg ~A) (scalar::one::neg ~A))    
         )    
        { "u3" u3, "v3" v3 }
        )
    ))
))

(def! param1 (scalar 3))
(def! param2 (scalar 9))
(def! param3 scalar::zero)
(def! param-u (scalar "273f910d9ecc1615d8618ed1d15fef4e9472c89ac043042d36183b2cb4d7ef51"))
(def! param-v (scalar "466a7e3a82f67ab1d32294fd89774ad6bc3332d0fa1ccd18a77a81f50667c8d7"))
(prove 
  (
    ;; (println (zk-square param1))
    ;; (println (zk-mul param1 param2))
    ;; (println 'witness (zk-witness param-u param-v))
    ;; (println 'double (last (last (zk-double param-u param-v))))
    ;; (println 'nonzero (zk-nonzero? param3))    
    (println 'not-small-order? (zk-not-small-order? param-u param-v))
  )
)