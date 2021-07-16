class Variable:

    def __init__(self, name):
        self.name = name

    def __pow__(self, n):
        expr = MultiplyExpression()
        expr.set_symbol(self.name, n)
        return expr

    def __eq__(self, other):
        return self.name == other.name

    def termify(self):
        expr = MultiplyExpression()
        expr.set_symbol(self.name, 1)
        return expr

class MultiplyExpression:

    def __init__(self):
        self.coeff = fp(1)
        self.symbols = {}

    def copy(self):
        result = MultiplyExpression()
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

    def __neg__(self):
        result = self.copy()
        result.coeff *= -1
        return result

    def __mul__(self, expr):
        result = MultiplyExpression()
        result.coeff = self.coeff
        result.symbols = self.symbols.copy()

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

    def __str__(self):
        repr = ""
        first = True
        if self.coeff != fp(1):
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
            expr = MultiplyExpression()
            expr.coeff = term
            term = expr

        return term

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

        terms = [self_term * term for self_term in self.terms]
        result = MultivariatePolynomial(terms)

        return result

    def _find(self, other):
        for term in self.terms:
            if term.matches(other):
                return term
        return None

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

