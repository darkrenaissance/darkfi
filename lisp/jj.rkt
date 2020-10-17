#lang racket

(struct zk_variable 
    (name type)
)

(define (zk_constant name hex_value)
    (fprintf out "constant ~a ~a\n" name hex_value)
    name
)

(define (zk_param name)
    (fprintf out "param ~a\n" name)
    (zk_variable name 'param)
)

(define (zk_public name)
    (fprintf out "public ~a\n" name)
    (zk_variable name 'public)
)

(define (strings->string sts)
    (apply string-append sts))

(define (apply_ns namespace name)
    (strings->string
        (append namespace
            (list "__" (symbol->string name))
        )))

(define (zk_local namespace name)
    (let ([name (apply_ns namespace name)])
        (fprintf out "local ~a\n" name)
        (zk_variable name 'local)
    )
)

(define (zk_private namespace name)
    (let ([name (apply_ns namespace name)])
        (fprintf out "private ~a\n" name)
        (zk_variable name 'private)
    )
)

(define (zk_comment str)
    (fprintf out "# ~a\n" str)
)

(define (zk_set self other)
    (fprintf out "set ~a ~a\n" (zk_variable-name self) (zk_variable-name other))
)
(define (zk_add self other)
    (fprintf out "add ~a ~a\n" (zk_variable-name self) (zk_variable-name other))
)
(define (zk_sub self other)
    (fprintf out "sub ~a ~a\n" (zk_variable-name self) (zk_variable-name other))
)
(define (zk_mul self other)
    (fprintf out "mul ~a ~a\n" (zk_variable-name self) (zk_variable-name other))
)
(define (zk_divide self other)
    (fprintf out "divide ~a ~a\n"
        (zk_variable-name self) (zk_variable-name other))
)

(define (zk_load self constant)
    (fprintf out "load ~a ~a\n" (zk_variable-name self) constant)
)

(define (zk_lc0_add self)
    (fprintf out "lc0_add ~a\n" (zk_variable-name self)))
(define (zk_lc1_add self)
    (fprintf out "lc1_add ~a\n" (zk_variable-name self)))
(define (zk_lc2_add self)
    (fprintf out "lc2_add ~a\n" (zk_variable-name self)))
(define (zk_lc0_sub self)
    (fprintf out "lc0_sub ~a\n" (zk_variable-name self)))
(define (zk_lc1_sub self)
    (fprintf out "lc1_sub ~a\n" (zk_variable-name self)))
(define (zk_lc2_sub self)
    (fprintf out "lc2_sub ~a\n" (zk_variable-name self)))
(define (zk_lc0_add_coeff constant self)
    (fprintf out "lc0_add_coeff ~a ~a\n" constant (zk_variable-name self)))
(define (zk_lc1_add_coeff constant self)
    (fprintf out "lc1_add_coeff ~a ~a\n" constant (zk_variable-name self)))
(define (zk_lc2_add_coeff constant self)
    (fprintf out "lc2_add_coeff ~a ~a\n" constant (zk_variable-name self)))
(define (zk_lc0_add_one)
    (fprintf out "lc0_add_one\n"))
(define (zk_lc1_add_one)
    (fprintf out "lc1_add_one\n"))
(define (zk_lc2_add_one)
    (fprintf out "lc2_add_one\n"))
(define (zk_enforce)
    (fprintf out "enforce\n")
)

(define (zk_distance namespace result x y)
    (let ([namespace (append namespace (list "_distance"))])
        (zk_set result x)
        (zk_mul result x)
        (let ([tmp (zk_local namespace 'tmp)])
            (zk_set tmp y)
            (zk_mul tmp y)
            (zk_add result tmp)
        )
    )
)

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

(define-syntax-rule (create_param name)
    (define name (zk_param 'name))
)
(define-syntax-rule (create_public name)
    (define name (zk_public 'name))
)

(define out (open-output-file "jj.psm" #:exists 'truncate))

(define const_d (zk_constant
    "d" "0x2a9318e74bfa2b48f5fd9207e6bd7fd4292d7f6d37579d2601065fd6d6343eb1"))
(define const_one (zk_constant
    "one" "0x0000000000000000000000000000000000000000000000000000000000000001"))

(fprintf out "contract foo\n")
(define namespace (list "_"))
(define a (create_jj_param_point "a"))
(define b (create_jj_param_point "b"))
(define result (create_jj_public_point "result"))
;(zk_distance (list "_") result x y)
(zk_jj_add namespace result a b)
(fprintf out "end\n")

