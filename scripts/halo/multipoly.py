class Variable:

    def __init__(self, name):
        self.name = name

    def __pow__(self, n):
        expr = MultiplyExpression()
        expr.set_symbol(self.name, n)
        return expr

    def termify(self):
        expr = MultiplyExpression()
        expr.set_symbol(self.name, 1)
        return expr

class MultiplyExpression:

    def __init__(self):
        self.coeff = None
        self.symbols = {}

    def set_symbol(self, var_name, power):
        self.symbols[var_name] = power

    def __mul__(self, expr):
        result = MultiplyExpression()
        result.coeff = self.coeff
        result.symbols = self.symbols.copy()

        if hasattr(expr, "field"):
            if result.coeff is None:
                result.coeff = expr
            else:
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
        return MultivariatePolynomial([self, expr])

    def __str__(self):
        repr = ""
        first = True
        if self.coeff is not None:
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

    def __init__(self, terms):
        self.terms = terms

    def __add__(self, term):
        if isinstance(term, Variable):
            term = term.termify()

        result = MultivariatePolynomial(self.terms[:])
        result.terms.append(term)
        return result

    def __str__(self):
        repr = ""
        first = True
        for term in self.terms:
            if first:
                first = False
            else:
                repr += " + "
            repr += str(term)
        return repr

from finite_fields import finitefield

p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
fp = finitefield.IntegersModP(p)

x = Variable("X")
y = Variable("Y")
z = Variable("Z")

p = x**3 * y**2 * x**2 * fp(5) * fp(2) + x**3 * y + z + fp(6)
print(p)

