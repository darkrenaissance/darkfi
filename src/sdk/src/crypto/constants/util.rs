/// Takes in an FnMut closure and returns a constant-length array with elements of
/// type `Output`.
pub fn gen_const_array<Output: Copy + Default, const LEN: usize>(
    closure: impl FnMut(usize) -> Output,
) -> [Output; LEN] {
    gen_const_array_with_default(Default::default(), closure)
}

pub(crate) fn gen_const_array_with_default<Output: Copy, const LEN: usize>(
    default_value: Output,
    closure: impl FnMut(usize) -> Output,
) -> [Output; LEN] {
    let mut ret: [Output; LEN] = [default_value; LEN];
    for (bit, val) in ret.iter_mut().zip((0..LEN).map(closure)) {
        *bit = val;
    }
    ret
}

/// The sequence of bits representing a u64 in little-endian order.
///
/// # Panics
///
/// Panics if the expected length of the sequence `NUM_BITS` exceeds
/// 64.
pub fn i2lebsp<const NUM_BITS: usize>(int: u64) -> [bool; NUM_BITS] {
    assert!(NUM_BITS <= 64);
    gen_const_array(|mask: usize| (int & (1 << mask)) != 0)
}
