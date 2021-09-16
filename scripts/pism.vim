"For autoload, add this to your VIM config:
"   VIM:    .vimrc
"   NeoVIM: .config/nvim/init.vim
"
"autocmd BufRead *.pism call SetPismOptions()
"function SetPismOptions()
"    set syntax=pism
"    source /home/narodnik/src/drk/scripts/pism.vim
"endfunction

if exists('b:current_syntax')
    finish
endif

syn keyword drkKeyword constant contract start end constraint
"syn keyword drkAttr
syn keyword drkType FixedGenerator BlakePersonalization PedersenPersonalization ByteSize U64 Fr Point Bool Scalar BinarySize
syn keyword drkFunctionKeyword enforce lc0_add_one lc1_add_one lc2_add_one lc_coeff_reset lc_coeff_double lc0_sub_one lc1_sub_one lc2_sub_one dump_alloc dump_local
syn match drkFunction "^[ ]*[a-z_0-9]* "
syn match drkComment "#.*$"
syn match drkNumber ' \zs\d\+\ze'
syn match drkHexNumber ' \zs0x[a-z0-9]\+\ze'
syn match drkConst '[A-Z_]\{2,}[A-Z0-9_]*'
syn keyword drkBoolVal true false
syn match drkPreproc "{%.*%}"
syn match drkPreproc2 "{{.*}}"

hi def link drkKeyword    Statement
"hi def link drkAttr       StorageClass
hi def link drkPreproc    PreProc
hi def link drkPreproc2   PreProc
hi def link drkType       Type
hi def link drkFunction   Function
hi def link drkFunctionKeyword Function
hi def link drkComment    Comment
hi def link drkNumber     Constant
hi def link drkHexNumber  Constant
hi def link drkConst      Constant
hi def link drkBoolVal    Constant

let b:current_syntax = "pism"
