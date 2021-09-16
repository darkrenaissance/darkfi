# Functions here are called from pism.py using getattr()
# and the function name as a string.

def witness(line, out, point):
    return \
r"""let %s = ecc::EdwardsPoint::witness(
    cs.namespace(|| "%s"),
    %s.map(jubjub::ExtendedPoint::from))?;""" % (out, line, point)

def assert_not_small_order(line, point):
    return '%s.assert_not_small_order(cs.namespace(|| "%s"))?;' % (point, line)

def u64_as_binary_le(line, out, val):
    return \
r"""let %s = boolean::u64_into_boolean_vec_le(
    cs.namespace(|| "%s"),
    %s,
)?;""" % (out, line, val)

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

def ec_mul(line, out, fr, base):
    return 'let %s = %s.mul(cs.namespace(|| "%s"), &%s)?;' % (
        out, base, line, fr)

def ec_add(line, out, a, b):
    return 'let %s = %s.add(cs.namespace(|| "%s"), &%s)?;' % (out, a, line, b)

def ec_repr(line, out, point):
    return 'let %s = %s.repr(cs.namespace(|| "%s"))?;' % (out, point, line)

def ec_get_u(line, out, point):
    return "let mut %s = %s.get_u().clone();" % (out, point)

def emit_ec(line, point):
    return '%s.inputize(cs.namespace(|| "%s"))?;' % (point, line)

def alloc_binary(line, out):
    return "let mut %s = vec![];" % out

def binary_clone(line, out, binary):
    return "let mut %s: Vec<_> = %s.iter().cloned().collect();" % (out, binary)

def binary_extend(line, binary, value):
    return "%s.extend(%s);" % (binary, value)

def binary_push(line, binary, bit):
    return "%s.push(%s);" % (binary, bit)

def binary_truncate(line, binary, size):
    return "%s.truncate(%s);" % (binary, size)

def static_assert_binary_size(line, binary, size):
    return "assert_eq!(%s.len(), %s);" % (binary, size)

def blake2s(line, out, input, personalization):
    return \
r"""let mut %s = blake2s::blake2s(
    cs.namespace(|| "%s"),
    &%s,
    %s,
)?;""" % (out, line, input, personalization)

def pedersen_hash(line, out, input, personalization):
    return \
r"""let mut %s = pedersen_hash::pedersen_hash(
    cs.namespace(|| "%s"),
    %s,
    &%s,
)?;""" % (out, line, personalization, input)

def emit_binary(line, binary):
    return 'multipack::pack_into_inputs(cs.namespace(|| "%s"), &%s)?;' % (
        line, binary)

def alloc_bit(line, out, value):
    return \
r"""let %s = boolean::Boolean::from(boolean::AllocatedBit::alloc(
    cs.namespace(|| "%s"),
    %s
)?);""" % (out, line, value)

def alloc_const_bit(line, out, value):
    return "let %s = Boolean::constant(%s);" % (out, value)

def clone_bit(line, out, value):
    return "let %s = %s.clone();" % (out, value)

def alloc_scalar(line, out, scalar):
    return \
r"""let %s =
    num::AllocatedNum::alloc(cs.namespace(|| "%s"), || Ok(*%s.get()?))?;""" % (
    out, line, scalar)

def scalar_as_binary(line, out, scalar):
    return 'let %s = %s.to_bits_le(cs.namespace(|| "%s"))?;' % (out, scalar,
                                                                line)

def emit_scalar(line, scalar):
    return '%s.inputize(cs.namespace(|| "%s"))?;' % (scalar, line)

def scalar_enforce_equal(line, scalar_left, scalar_right):
    return \
r"""cs.enforce(
    || "%s",
    |lc| lc + %s.get_variable(),
    |lc| lc + CS::one(),
    |lc| lc + %s.get_variable(),
);""" % (line, scalar_left, scalar_right)

def conditionally_reverse(line, out_left, out_right, in_left, in_right,
                          condition):
    return \
r"""let (%s, %s) = num::AllocatedNum::conditionally_reverse(
    cs.namespace(|| "%s"),
    &%s,
    &%s,
    &%s,
)?;""" % (out_left, out_right, line, in_left, in_right, condition)

