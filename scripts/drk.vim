if exists('b:current_syntax')
    finish
endif

syn keyword drkKeyword assert enforce for in def return const as let emit contract private proof
syn keyword drkAttr mut
syn keyword drkType BinaryNumber Point Fr SubgroupPoint EdwardsPoint Scalar EncryptedNum list Bool U64 Num Binary
syn match drkFunction "\zs[a-zA-Z0-9_]*\ze("
syn match drkComment "#.*$"
syn match drkNumber '\d\+'
syn match drkConst '[A-Z_]\{2,}[A-Z0-9_]*'

hi def link drkKeyword    Statement
hi def link drkAttr       StorageClass
hi def link drkType       Type
hi def link drkFunction   Function
hi def link drkComment    Comment
hi def link drkNumber     Constant
hi def link drkConst      Constant

let b:current_syntax = "drk"
