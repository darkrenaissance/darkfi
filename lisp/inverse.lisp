(println "new-cs.lisp")

( (let* [aux (scalar 3)
      x (alloc "x" aux)
      x2 (alloc "x2" (* aux aux))
      x3 (alloc "x3" (* aux (* aux aux)))
      input (alloc-input "input" (scalar 3))
      ]
(prove
 (setup 
  (
  (enforce 
    (scalar::one input)
    (scalar::one cs::one)
    (scalar::one x3)  
  )

  (enforce 
    (scalar::one x2)
    (scalar::one x)
    (scalar::one x3)
  )

  (enforce  
    (scalar::one x)
    (scalar::one x)
    (scalar::one x2)
  )
  )
  )
 )
)
)
;; (println 'verify  (MyCircuit (scalar 27)))
