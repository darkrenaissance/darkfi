(load-file "util.lisp")

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
               EDWARDS_D (gensym)
               result (gensym)] (
        `(def! ~EDWARDS_D (alloc-const ~EDWARDS_D (scalar "2a9318e74bfa2b48f5fd9207e6bd7fd4292d7f6d37579d2601065fd6d6343eb1")))
        `(def! ~u2 (alloc ~u2 (get (nth (nth (zk-square ~val1) 0) 3) "v2")))
        `(def! ~v2 (alloc ~v2 (get (nth (nth (zk-square ~val2) 0) 3) "v2")))
        `(def! result (alloc-input ~u2v2 (get (last (last (zk-mul ~u2 ~v2))) "result")))        
        ;; `(def! result (alloc-input ~result (* ~u2 ~v2)))        
        `(enforce  
            ((scalar::one::neg ~u2) (scalar::one ~v2))
            (scalar::one cs::one)
            ((scalar::one cs::one) (~EDWARDS_D ~u2v2))
         )
        `{ "result" result }
        )
    ))
))

(def! param1 (scalar 3))
(def! param2 (scalar 9))
(def! param-u (scalar "6800f4fa0f001cfc7ff6826ad58004b4d1d8da41af03744e3bce3b7793664337"))
(def! param-v (scalar "6d81d3a9cb45dedbe6fb2a6e1e22ab50ad46f1b0473b803b3caefab9380b6a8b"))
(prove 
  (
    ;; (println (zk-square param1))
    ;; (println (zk-mul param1 param2))
    (println (zk-witness param-u param-v))
  )
)