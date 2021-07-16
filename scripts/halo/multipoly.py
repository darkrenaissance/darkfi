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

    def clean(self):
        for symbol in list(self.symbols.keys()):
            if self.symbols[symbol] == 0:
                del self.symbols[symbol]

    def matches(self, other):
        return self.symbols == other.symbols

    def set_symbol(self, var_name, power):
        self.symbols[var_name] = power

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
        return result

    def __add__(self, expr):
        if isinstance(expr, Variable):
            expr = expr.termify()

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
        return MultivariatePolynomial(self.terms[:])

    def __add__(self, term):
        if isinstance(term, Variable):
            term = term.termify()

        if hasattr(term, "field"):
            expr = MultiplyExpression()
            expr.coeff = term
            term = expr

        if isinstance(term, MultiplyExpression):
            # Delete ^0 variables
            term.clean()
            # Skip zero terms
            #if term.coeff is None or term.coeff == 0:
            #    return self

            result = self.copy()
            result_term = result.find(term)
            if result_term is None:
                result.terms.append(term)
            else:
                result_term.coeff += term.coeff
            return result
        else:
            assert isinstance(term, MultivariatePolynomial)
            result = self.copy()
            for other_term in term.terms:
                found = False
                for self_term in result.terms:
                    if self_term.matches(other_term):
                        self_term.coeff += other_term.coeff
                        found = True
                        break
                if not found:
                    result.terms.append(other_term)
            return result

    def find(self, other):
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

    p = x**3 * y**2 * x**2 * fp(5) * fp(2) + x**3 * y + z + fp(6)
    q = x**3 * y * fp(3) + y
    print(p)
    print(q)
    print(p + q)

