if exists('b:current_syntax')
    finish
endif

syn keyword sapviKeyword constant contract start end
"syn keyword sapviAttr
syn keyword sapviType FixedGenerator BlakePersonalization PedersenPersonalization ByteSize U64 Fr Point Bool Scalar
syn match sapviFunction "^[a-z_0-9]* "
syn match sapviComment "#.*$"
syn match sapviNumber ' \zs\d\+\ze'
syn match sapviConst '[A-Z_]\{2,}[A-Z0-9_]*'

hi def link sapviKeyword    Statement
"hi def link sapviAttr       StorageClass
hi def link sapviType       Type
hi def link sapviFunction   Function
hi def link sapviComment    Comment
hi def link sapviNumber     Constant
hi def link sapviConst      Constant

let b:current_syntax = "pism"
