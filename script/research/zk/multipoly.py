/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

import numpy as np
from finite_fields import finitefield

class Variable:

    def __init__(self, name, fp):
        self.name = name
        self.fp = fp

    def __pow__(self, n):
        expr = MultiplyExpression(self.fp)
        expr.set_symbol(self.name, n)
        return expr

    def __eq__(self, other):
        return self.name == other.name

    def __hash__(self):
        return hash(self.name)

    def termify(self):
        expr = MultiplyExpression(self.fp)
        expr.set_symbol(self.name, 1)
        return expr

class MultiplyExpression:

    def __init__(self, fp):
        self.coeff = fp(1)
        self.symbols = {}
        self.fp = fp

    def copy(self):
        result = MultiplyExpression(self.fp)
        result.coeff = self.coeff
        result.symbols = self.symbols.copy()
        return result

    def clean(self):
        for symbol in list(self.symbols.keys()):
            if self.symbols[symbol] == 0:
                del self.symbols[symbol]

    def matches(self, other):
        return self.symbols == other.symbols

    def set_symbol(self, var_name, power):
        self.symbols[var_name] = power

    def __eq__(self, other):
        return (self.coeff == other.coeff and
                self.symbols == other.symbols)

    def __neg__(self):
        result = self.copy()
        result.coeff *= -1
        return result

    def __mul__(self, expr):
        result = MultiplyExpression(self.fp)
        result.coeff = self.coeff
        result.symbols = self.symbols.copy()

        if isinstance(expr, np.int64) or isinstance(expr, int):
            expr = self.fp(int(expr))

        if hasattr(expr, "field"):
            result.coeff *= expr
            return result

        if isinstance(expr, Variable):
            expr = expr.termify()

        for var_name, power in expr.symbols.items():
            if var_name in result.symbols:
                result.symbols[var_name] += power
            else:
                result.symbols[var_name] = power

        # Remember to multiply the coefficients
        result.coeff *= expr.coeff
        return result

    def __add__(self, expr):
        if isinstance(expr, Variable):
            expr = expr.termify()

        if self.matches(expr):
            result = self.copy()
            result.coeff += expr.coeff
            return result

        return MultivariatePolynomial([self, expr])

    def __sub__(self, expr):
        expr = -expr
        return self + expr

    def evaluate(self, symbol_map):
        result = MultiplyExpression(self.fp)
        for symbol, power in self.symbols.items():
            if symbol in symbol_map:
                value = symbol_map[symbol]
                result *= value**power
            else:
                result *= Variable(symbol, self.fp)**power
        return result

    def __str__(self):
        repr = ""
        first = True
        if self.coeff != 1:
            repr += str(self.coeff)
            first = False
        for var_name, power in self.symbols.items():
            if first:
                first = False
            else:
                repr += " "

            if power == 1:
                repr += var_name
            else:
                repr += var_name + "^" + str(power)

        return repr

class MultivariatePolynomial:

    def __init__(self, terms=[]):
        self.terms = terms

    def copy(self):
        terms = [term.copy() for term in self.terms]
        return MultivariatePolynomial(terms)

    # Operations can accept Variables and constants
    # so we make sure to convert them to MultiplyExpression types
    def _convert_term(self, term):
        if isinstance(term, Variable):
            term = term.termify()

        if hasattr(term, "field"):
            expr = MultiplyExpression(term.field)
            expr.coeff = term
            term = expr

        return term

    def __bool__(self):
        return bool(self.terms)

    def __eq__(self, other):
        return self.terms == other.terms

    def __neg__(self):
        terms = [-term for term in self.terms]
        return MultivariatePolynomial(terms)

    def __add__(self, term):
        term = self._convert_term(term)

        if isinstance(term, MultivariatePolynomial):
            # Recursively apply addition operation
            result = self.copy()
            for other_term in term.terms:
                result += other_term
            return result

        assert isinstance(term, MultiplyExpression)
        # Delete ^0 variables
        term.clean()

        # Skip terms where the coeff is 0
        if term.coeff == 0:
            return self

        result = self.copy()
        result_term = result._find(term)

        if result_term is None:
            result.terms.append(term)
        else:
            result_term.coeff += term.coeff

        return result

    def __sub__(self, term):
        term = -term
        return self + term

    def __mul__(self, term):
        term = self._convert_term(term)

        if isinstance(term, MultivariatePolynomial):
            # Recursively apply addition operation
            result = MultivariatePolynomial()
            for other_term in term.terms:
                result += self * other_term
            return result

        assert isinstance(term, MultiplyExpression)
        # Delete ^0 variables
        term.clean()

        # Skip terms where the coeff is 0
        if term.coeff == 0:
            return self

        terms = [self_term * term for self_term in self.terms]
        result = MultivariatePolynomial(terms)

        return result

    def divmod(self, poly):
        assert isinstance(poly, MultivariatePolynomial)
        # https://www.win.tue.nl/~aeb/2WF02/groebner.pdf

    def _find(self, other):
        for term in self.terms:
            if term.matches(other):
                return term
        return None

    def evaluate(self, variable_map):
        p = MultivariatePolynomial()
        for term in self.terms:
            assert isinstance(term, MultiplyExpression)
            p += term.evaluate(variable_map)
        return p

    def _assert_unique_terms(self):
        for i, term1 in enumerate(self.terms):
            for q, term2 in enumerate(self.terms):
                if i == q:
                    continue
                assert not term1.matches(term2)

    def filter(self, variables):
        p = MultivariatePolynomial()
        for term in self.terms:
            assert isinstance(term, MultiplyExpression)

            skip = False
            for variable in variables:
                symbol = variable.name
                if symbol in term.symbols:
                    skip = True

            if not skip:
                p += term
        return p

    def __str__(self):
        if not self.terms:
            return "0"

        repr = ""
        first = True
        for term in self.terms:
            if first:
                first = False
            else:
                repr += " + "
            repr += str(term)
        return repr

if __name__ == "__main__":
    from finite_fields import finitefield

    p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
    fp = finitefield.IntegersModP(p)

    x = Variable("X")
    y = Variable("Y")
    z = Variable("Z")

    print(y**2 + y**2)

    p = x**3 * y**2 * x**2 * fp(5) * fp(2) + x**3 * y + z + fp(6)
    q = x**3 * y * fp(3) + y
    print(p)
    print(q)
    print(p + q)
    print(p * q)
    print(-q)
    print(p - q)

