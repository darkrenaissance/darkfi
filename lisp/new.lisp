(def! circuit (zk* [x] (
    (def! bits (unpack-bits x 256))
    (map (fn* [b] (println b
                    '(add lc0 one) 
                    '(sub b)
                    '(add lc1 x)
                    'enforce)
              ) bits)
   (map (fn* [b] (println b '(add lc0 b) 
                          'double-coeff-lc)
             ) bits)
   (println 'reset-coeff-lc
   '(sub lc0 x)
   '(add lc1 one)
   'enforce)
)))
(def! dec "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000")
(circuit dec)
