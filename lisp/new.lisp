(def! x "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000")
(def! one "0000000000000000000000000000000000000000000000000000000000000001")
(def! bits (unpack-bits x))
(defzk! circuit ())
(def! cvalues (map (fn* [b] (eval
                    (add lc0 one) 
                    (sub lc0 b)
                    (add lc1 x)
                    enforce)
                        ) bits))
(def! cs (concat cvalues (list 
                 'reset-coeff-lc
                 (sub lc0 x)
                 (add lc1 one)
                 'enforce)))
(println "bit-dec")
(zkcons! circuit cs)
