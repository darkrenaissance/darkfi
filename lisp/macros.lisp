;; testing macros

(def! inc (fn* [a] (i+ a 1)))
(def! gensym
  (let* [counter (atom 0)]
    (fn* []
      (symbol (str "G__" (swap! counter inc))))))

(defmacro! zk-square (fn* [var] (
        (let* [v1 (gensym)
               v2 (gensym)] (
          (println 'values var)
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
(def! param1 (scalar 1))
(def! param2 (scalar 3))
(prove 
  (
    (zk-square param1)  
    (zk-square param2)
  )
)