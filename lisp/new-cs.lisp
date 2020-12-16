;; defzk!
;; enforce LABEL 
;; alloc
;; alloc-input 
;; scalar::one
;; scalar::zero
;; scalar
;; cs::one
;; bellman::zero
;; setup
;; prove
;; verify
(println "new-cs.lisp")
(def! MyCircuit (fn* [aux]  
(let* [x  (alloc "num" (first aux))
     x2 (alloc "product num" (first (rest aux))) 
     x3 (alloc "product num" (last aux))
     input (alloc-input "input variable" (last aux))]
   ;; Lc0: [(Scalar::one(), CS::one()), (Scalar::one().neg(), C)]
;; Lc1: [(Scalar::one(), y)]
;; Lc2: [(Scalar::one(), U), (Scalar::one().neg(), A), (Scalar::one().neg(), B)]
    (enforce (scalar::one x) ((neg scalar::one) x2) ((neg scalar::one) x3))
)))
(def! a (scalar "0000000000000000000000000000000000000000000000000000000000000003"))
(setup (MyCircuit (a (* a a) (* (* a a) a))))
;; (prove MyCircuit)
;; (verify (prove MyCircuit) (scalar 27))
;; (U - A - B) / (1 - C)
;; [(1 - C)] * [y] = [U - A - B]
;; Lc0: [(Scalar::one(), CS::one()), (Scalar::one().neg(), C)]
;; Lc1: [(Scalar::one(), y)]
;; Lc2: [(Scalar::one(), U), (Scalar::one().neg(), A), (Scalar::one().neg(), B)]
;; assert (x1 + y1) * (x2 + y2) == U
;; Lc0: [(Scalar::one(), x1), (Scalar::one(), y1)]
;; Lc1: [(Scalar::one(), x2), (Scalar::one(), y2)]
;; Lc2: [(Scalar::one(), U)]
;; (enforce ((scalar::one x1) (scalar::one y1)) ((scalar::one x2) (scalar::one y2) ((scalar::one U)))

     
