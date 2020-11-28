;; defzk!
;; enforce LABEL 
;; alloc
;; alloc-input 
;; scalar::one
;; scalar
;; cs::one
;; bellman::zero
;; setup
;; prove
;; verify
(println "new-cs.lisp")
(def! MyCircuit (fn* [aux]  
(let* [x  (alloc "num" (first aux))
     x2 (alloc "product num" (second aux)) 
     x3 (alloc "product num" (last aux))
     input (alloc-input "input variable" (last aux))
     ;; let coeff = bls12_381::Scalar::one();
     coeff scalar::one]
    (enforce "mc" coeff 
         ;; let lc0 = bellman::LinearCombination::zero() + (coeff, x_var);
         (zero + x) 
         ;; let lc1 = bellman::LinearCombination::zero() + (coeff, x_var);
         (zero + x) 
         ;; let lc2 = bellman::LinearCombination::zero() + (coeff, x2_var);
         (zero + x3))
    (enforce "mc" coeff (zero + x2) (zero + x) (zero + x3))
    (enforce "mc" coeff (zero + input) (zero + cs::one) (zero + x3))
)))
(def! a (scalar "0000000000000000000000000000000000000000000000000000000000000003"))
(println (* (* a a) a))
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

     
