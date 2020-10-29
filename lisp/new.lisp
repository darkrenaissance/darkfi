(def! x "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000")
(def! one "0000000000000000000000000000000000000000000000000000000000000001")
(defzk! circuit ())
(def! bits (unpack-bits x))
;;zkcons add a constraints instruction to the circuit
    (zkcons! circuit (
                    (add lc0 one) 
                    (sub lc0 x)
                    )
              )
;;    (map (fn* [b] (zkcons! circuit (
;;                    (add lc0 one) 
;;                    '(sub lc0 b)
;;                    '(add lc1 x)
;;                    'enforce)
;;              )) bits)
;;   (map (fn* [b] (zkcons! circuit (
;;                    '(add lc0 b) 
;;                    'double-coeff-lc)
;;             )) bits)
;;   (zkcons! circuit (
;;                    'reset-coeff-lc
;;                    '(sub lc0 x)
;;                    '(add lc1 one)
;;                    'enforce)
;;            )
(println circuit)
