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
(defzk! MyCircuit (fn* [aux]  
[let x  (alloc "num" (first aux))
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
))
(def! a (scalar 3))
(setup MyCircuit (a (* a a) (* a a a)))
;; (prove MyCircuit)
;; (verify (prove MyCircuit) (scalar 27))

     
