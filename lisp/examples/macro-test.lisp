(def! inc (fn* [a] (i+ a 1)))
(def! gensym
  (let* [counter (atom 0)]
    (fn* []
      (symbol (str "G__" (swap! counter inc))))))

(defmacro! zk-square (fn* [var] (
        (let* [v1 (gensym)
               v2 (gensym)] (
        `(alloc ~v1 ~var)
        `(def! v2 (alloc ~v2 (square ~var)))
        `(enforce  
            (scalar::one ~v1) 
            (scalar::one ~v1) 
            (scalar::one ~v2) 
         )
        `{ "v2" v2 }
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

(def! param1 (scalar 3))
(def! param2 (scalar 1))
(prove 
  (
    (def! result1 (zk-square param1))
    ;; (def! result2 (zk-square param2))
    (println 'result1_map (nth (nth result1 0) 3))
    (println 'result1 (nth (nth result1 0) 1))   
  )
)