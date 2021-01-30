(println "new-cs.lisp")

( (let* [aux (scalar 3)
      x (alloc "x" aux)
      x2 (alloc "x2" (* aux aux))
      x3 (alloc "x3" (* aux (* aux aux)))
      input (alloc-input "input" aux)
      ]
 (setup 
   ;; (enforce left right output)
  (
  (enforce  
    (
     (scalar::one x)
     (scalar::one x)
    )
    ;;(scalar::one::neg x)
    (scalar::one x)
    (scalar::one x2)
  )

  (enforce 
    (scalar::one x2)
    (scalar::one x)
    (scalar::one x3)
  )

  (enforce 
    (scalar::one input)
    (scalar::one cs::one)
    (scalar::one x3)  
  )
  )
  )
 )
(prove)
)
;; (println 'verify  (MyCircuit (scalar 27)))
