if exists('b:current_syntax')
    finish
endif

syn keyword sapviKeyword assert enforce for in def return const as let
syn keyword sapviAttr mut
syn keyword sapviType Point Scalar EncryptedNum list bool u64 Num
syn match sapviFunction "\zs[a-zA-Z0-9_]*\ze("
syn match sapviComment "#.*$"
syn match sapviNumber '\d\+'
syn match sapviConst '[A-Z_]\{2,}[A-Z0-9_]*'

hi def link sapviKeyword    Statement
hi def link sapviAttr       StorageClass
hi def link sapviType       Type
hi def link sapviFunction   Function
hi def link sapviComment    Comment
hi def link sapviNumber     Constant
hi def link sapviConst      Constant

let b:current_syntax = "sapvi"
