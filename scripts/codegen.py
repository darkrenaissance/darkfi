# Functions here are called from pism.py using getattr()
# and the function name as a string.

def witness(line, out, point):
    return \
r"""let %s = ecc::EdwardsPoint::witness(
    cs.namespace(|| "%s"),
    %s.map(jubjub::ExtendedPoint::from))?;""" % (out, line, point)

def assert_not_small_order(line, point):
    return '%s.assert_not_small_order(cs.namespace(|| "%s"))?;' % (point, line)

def fr_as_binary_le(line, out, fr):
    return \
r"""let %s = boolean::field_into_boolean_vec_le(
    cs.namespace(|| "%s"), %s)?;""" % (out, line, fr)

def ec_mul_const(line, out, fr, base):
    return \
r"""let %s = ecc::fixed_base_multiplication(
    cs.namespace(|| "%s"),
    &%s,
    &%s,
)?;""" % (out, line, base, fr)

def ec_add(line, out, a, b):
    return 'let %s = %s.add(cs.namespace(|| "%s"), &%s)?;' % (out, a, line, b)

def ec_repr(line, out, point):
    return 'let %s = %s.repr(cs.namespace(|| "%s"))?;' % (out, point, line)

def emit_ec(line, point):
    return '%s.inputize(cs.namespace(|| "%s"))?;' % (point, line)

def alloc_binary(line, out):
    return "let mut %s = vec![];" % out

def binary_clone(line, out, binary):
    return "let %s = %s.iter().cloned();" % (out, binary)

def binary_extend(line, binary, value):
    return "%s.extend(%s);" % (binary, value)

def static_assert_binary_size(line, binary, size):
    return "assert_eq!(%s.len(), %s);" % (binary, size)

def blake2s(line, out, input, personalization):
    return \
r"""let mut %s = blake2s::blake2s(
    cs.namespace(|| "%s"),
    &%s,
    %s,
)?;""" % (out, line, input, personalization)

def emit_binary(line, binary):
    return 'multipack::pack_into_inputs(cs.namespace(|| "%s"), &%s)?;' % (
        line, binary)

