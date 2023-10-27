-- LPEG lexer for the zkas zk language
local l = require('lexer')
local token, word_match = l.token, l.word_match
local P, R, S = lpeg.P, lpeg.R, lpeg.S

local lex = l.new('zk', {fold_by_indentation = true})

-- Whitespace.
local indent = #l.starts_line(S(' \t')) *
  (token(l.WHITESPACE, ' ') + token('indent_error', '\t'))^1
lex:add_rule('indent', indent)
lex:add_style('indent_error', {back = l.colors.red})
lex:add_rule('whitespace', token(l.WHITESPACE, S(' \t')^1 + l.newline^1))

-- Comments.
local comment = token(l.COMMENT, '#' * l.nonnewline_esc^0)
lex:add_rule('comment', comment)

-- Strings.
local dq_str = P('U')^-1 * l.range('"', true)
local string = token(l.STRING, dq_str)
lex:add_rule('string', string)

-- Numbers.
local number = token(l.NUMBER, l.integer)
lex:add_rule('number', number)

-- Keywords.
local keyword = token(l.KEYWORD, word_match{
  'k', "field", 'constant', 'witness', 'circuit',
})
lex:add_rule('keyword', keyword)

-- Constants.
local constant = token(l.CONSTANT, word_match{
  'true', 'false',
  'VALUE_COMMIT_VALUE', 'VALUE_COMMIT_RANDOM', 'NULLIFIER_K',
})
lex:add_rule('constant', constant)

-- Types.
local type = token(l.TYPE, word_match{
  'EcPoint', 'EcFixedPoint', 'EcFixedPointBase', 'EcFixedPointShort',
  'EcNiPoint', 'Base', 'BaseArray', 'Scalar', 'ScalarArray',
  'MerklePath', 'Uint32', 'Uint64',
})
lex:add_rule('type', type)

-- Instructions.
local instruction = token('instruction', word_match{
  'ec_add', 'ec_mul', 'ec_mul_base', 'ec_mul_short', 'ec_mul_var_base',
  'ec_get_x', 'ec_get_y',
  'base_add', 'base_mul', 'base_sub',
  'poseidon_hash', 'merkle_root',
  'range_check', 'less_than_strict', 'less_than_loose', 'bool_check',
  'cond_select', 'zero_cond', 'witness_base',
  'constrain_equal_base', 'constrain_equal_point',
  'constrain_instance', 'debug',
})
lex:add_rule('instruction', instruction)

-- Identifiers.
local identifier = token(l.IDENTIFIER, l.word)
lex:add_rule('identifier', identifier)

-- Operators.
local operator = token(l.OPERATOR, S('(){}=;,'))
lex:add_rule('operator', operator)

return lex
