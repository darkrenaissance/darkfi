" Syntax highlighting for zkas scripts.
" Symlink into ~/.vim/syntax/zk.vim
" Add to your vim init file:
"   au BufNewFile,BufRead,BufReadPost *.zk set syntax=zk

if exists("b:current_syntax")
    finish
endif

syn keyword zkasKeyword 
    \ constant
    \ witness
    \ circuit

syn keyword zkasType 
    \ EcPoint EcFixedPoint EcFixedPointBase EcFixedPointShort EcNiPoint
    \ Base BaseArray Scalar ScalarArray
    \ MerklePath Uint32 Uint64

syn keyword zkasInstruction
    \ ec_add ec_mul ec_mul_base ec_mul_short ec_mul_var_base
    \ ec_get_x ec_get_y
    \ base_add base_mul base_sub
    \ poseidon_hash merkle_root
    \ range_check less_than_strict less_than_loose bool_check
    \ cond_select zero_cond witness_base
    \ constrain_equal_base constrain_equal_point
    \ constrain_instance debug

syn region zkasString start='"' end='"' contained

syn keyword zkasTodo contained TODO FIXME XXX NOTE
syn match zkasComment "#.*$" contains=zkasTodo

hi def link zkasKeyword      Statement
hi def link zkasType         Type
hi def link zkasInstruction  Function
hi def link zkasString       Constant
hi def link zkasTodo         Todo
hi def link zkasComment      Comment

let b:current_syntax = "zk"
