(def! inc (fn* [a] (i+ a 1)))
(def! gensym
  (let* [counter (atom 0)]
    (fn* []
      (symbol (str "G__" (swap! counter inc))))))

(defmacro! zk-square (fn* [var] (
        (let* [v1 (gensym)
               v2 (gensym)] (
        `(alloc ~v1 ~var)
        `(def! output (alloc ~v2 (square ~var)))
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
        `(def! result (alloc ~var (* ~val1 ~val2)))
        `(enforce  
            (scalar::one ~v1) 
            (scalar::one ~v2) 
            (scalar::one ~var) 
         )
        `{ "result" result }
        )
    ))
))

;; -u^2 + v^2 = 1 + du^2v^2
(defmacro! zk-witness (fn* [val1 val2] (
        (let* [u2v2 (gensym)] (
        ;; i know this is ugly, we need to work better on how to 
        ;; fetch the result of the function that is by design 
        ;; a list/vector so we use nth to get the element that 
        ;; is the hashmap defined inside zk-square with key v2
        `(def! u2 (alloc ~u2 (get (nth (nth (zk-square ~val1) 0) 3) "v2")))
        `(def! v2 (alloc ~v2 (get (nth (nth (zk-square ~val2) 0) 3) "v2")))
        `(alloc ~u2v2 (get (nth (nth (zk-mul u2 v2) 0) 3) "XXXX"))
        `(enforce  
            ((scalar::one::neg ~u2) (scalar::one ~v2))
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
    ;; (def! result1 (zk-square param1))
    ;; (println 'result1_map (get (nth (nth result1 0) 3) "v2"))
    ;; (def! result2 (zk-square param2))
    ;; (println 'result_mul (get (last (last (zk-mul param1 param1))) "result"))
  )
)