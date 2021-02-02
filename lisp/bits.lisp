(def! bit-dec 
      (fn* [x] (
        (def! bits (unpack-bits x 256))                        
        (def! enforce-step-1 (fn* [b] (enforce (add-one-lc0 (sub-lc0 b) (add-lc1 b))))
        (map enforce-step-1 bits)
        (map (fn* [b] ((add-lc0 b) double-coeff-lc) bits)                       
        (enforce reset-coeff-lc sub-lc0 add-one-lc1)
      )))))
                            
