#lang racket

(require "zk.rkt")

(struct jj_point
    (u v)
)

(define (create_jj_param_point name)
    (jj_point
        (zk_param (string-append name "_u"))
        (zk_param (string-append name "_v"))
    )
)
(define (create_jj_public_point name)
    (jj_point
        (zk_public (string-append name "_u"))
        (zk_public (string-append name "_v"))
    )
)

(define (zk_jj_add namespace result a b)
    (zk_comment "call jj_add()")
    (let* ([namespace (append namespace (list "_jj_add"))]
           [U (zk_private namespace 'U)]
           [A (zk_private namespace 'A)]
           [B (zk_private namespace 'B)]
           [C (zk_private namespace 'C)]
           [tmp (zk_local namespace 'tmp)])
        (zk_comment "Compute U = (x1 + y1) * (y2 - EDWARDS_A*x2)")
        (zk_comment "          = (x1 + y1) * (x2 + y2)")
        (zk_set U (jj_point-u a))
        (zk_add U (jj_point-v a))

        (zk_set tmp (jj_point-u b))
        (zk_add tmp (jj_point-v b))

        (zk_mul U tmp)

        (zk_comment "assert (x1 + y1) * (x2 + y2) == U")
        (zk_lc0_add (jj_point-u a))
        (zk_lc0_add (jj_point-v a))
        (zk_lc1_add (jj_point-u b))
        (zk_lc1_add (jj_point-v b))
        (zk_lc2_add U)
        (zk_enforce)

        (zk_comment "Compute A = y2 * x1")
        (zk_set A (jj_point-v b))
        (zk_mul A (jj_point-u a))
        (zk_comment "Compute B = x2 * y1")
        (zk_set B (jj_point-u b))
        (zk_mul B (jj_point-v a))
        (zk_comment "Compute C = d*A*B")
        (zk_load C const_d)
        (zk_mul C A)
        (zk_mul C B)

        (zk_comment "assert (d * A) * (B) == C")
        (zk_lc0_add_coeff const_d A)
        (zk_lc1_add B)
        (zk_lc2_add C)
        (zk_enforce)

        (zk_comment "Compute P.x = (A + B) / (1 + C)")
        (zk_set (jj_point-u result) A)
        (zk_add (jj_point-u result) B)
        ; Re-use the tmp variable from earlier here
        (zk_load tmp const_one)
        (zk_add tmp C)
        (zk_divide (jj_point-u result) tmp)

        (zk_lc0_add_one)
        (zk_lc0_add C)
        (zk_lc1_add (jj_point-u result))
        (zk_lc2_add A)
        (zk_lc2_add B)
        (zk_enforce)

        (zk_comment "Compute P.y = (U - A - B) / (1 - C)")
        (zk_set (jj_point-v result) U)
        (zk_sub (jj_point-v result) A)
        (zk_sub (jj_point-v result) B)
        ; Re-use the tmp variable from earlier here
        (zk_load tmp const_one)
        (zk_sub tmp C)
        (zk_divide (jj_point-v result) tmp)

        (zk_lc0_add_one)
        (zk_lc0_sub C)
        (zk_lc1_add (jj_point-v result))
        (zk_lc2_add U)
        (zk_lc2_sub A)
        (zk_lc2_sub B)
        (zk_enforce)
    )
)

(create_zk_output "jj.psm")

(define const_d (zk_constant
    "d" "0x2a9318e74bfa2b48f5fd9207e6bd7fd4292d7f6d37579d2601065fd6d6343eb1"))
(define const_one (zk_constant
    "one" "0x0000000000000000000000000000000000000000000000000000000000000001"))

(zk_contract_begin "foo")
(define namespace (list "_"))
(define a (create_jj_param_point "a"))
(define b (create_jj_param_point "b"))
(define result (create_jj_public_point "result"))
(zk_jj_add namespace result a b)
(zk_contract_end)

