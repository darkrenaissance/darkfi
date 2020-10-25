
(def! dec "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000")
(def! bits (unpack-bits dec 256))
(println (count bits))
(map (fn* [b] (println (add-one-lc0))) bits)

;;        (map (fn* [b] ((add-lc0 b) (double-coeff-lc))) bits)
;;        (reset-coeff-lc)
;;        (sub-lc0 x)
;;        (add-one-lc1)
