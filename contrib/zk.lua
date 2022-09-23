-- LPEG lexer for the zkas zk language
local l = require('lexer')
local token, word_match = l.token, l.word_match
local P, R, S = lpeg.P, lpeg.R, lpeg.S

local M = {_NAME = 'zk'}

-- Whitespace.
local ws = token(l.WHITESPACE, l.space^1)

-- Comments.
local comment = token(l.COMMENT, '#' * l.nonnewline_esc^0)

-- Strings.
local dq_str = P('U')^-1 * l.delimited_range('"', true)
local string = token(l.STRING, dq_str)

-- Numbers.
local number = token(l.NUMBER, l.integer)

-- Keywords.
local keyword = token(l.KEYWORD, word_match{
  'constant', 'contract', 'circuit',
})

-- Constants.
local constant = token(l.CONSTANT, word_match{
	'true', 'false',
	'VALUE_COMMIT_VALUE', 'VALUE_COMMIT_RANDOM', 'NULLIFIER_K',
})

-- Types.
local type = token(l.TYPE, word_match{
  'EcPoint', 'EcFixedPoint', 'EcFixedPointBase', 'EcFixedPointShort',
  'Base', 'BaseArray', 'Scalar', 'ScalarArray',
  'MerklePath',
  'Uint32', 'Uint64',
})

-- Instructions.
local instruction = token('instruction', word_match{
  'ec_add', 'ec_mul', 'ec_mul_base', 'ec_mul_short',
  'ec_get_x', 'ec_get_y',
  'base_add', 'base_mul', 'base_sub', 'greater_than',
  'poseidon_hash', 'merkle_root', 'constrain_instance',
  'range_check', 'less_than', 'bool_check',
  'witness_base',
})

-- Identifiers.
local identifier = token(l.IDENTIFIER, l.word)

-- Operators.
local operator = token(l.OPERATOR, S('(){}=;,'))

M._rules = {
  {'whitespace', ws},
  {'comment', comment},
  {'keyword', keyword},
  {'type', type},
  {'constant', constant},
  {'string', string},
  {'number', number},
  {'instruction', instruction},
  {'identifier', identifier},
  {'operator', operator},
}

return M
