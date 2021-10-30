use std::borrow::Cow;
use std::io::{Cursor, Read, Write};
use std::net::{IpAddr, SocketAddr};
use std::{io, mem};

use crate::endian;
use crate::error::{Error, Result};

/// Encode an object into a vector
pub fn serialize<T: Encodable + ?Sized>(data: &T) -> Vec<u8> {
    let mut encoder = Vec::new();
    let len = data.encode(&mut encoder).unwrap();
    assert_eq!(len, encoder.len());
    encoder
}

/// Encode an object into a hex-encoded string
pub fn serialize_hex<T: Encodable + ?Sized>(data: &T) -> String {
    hex::encode(serialize(data))
}

/// Deserialize an object from a vector, will error if said deserialization
/// doesn't consume the entire vector.
pub fn deserialize<T: Decodable>(data: &[u8]) -> Result<T> {
    let (rv, consumed) = deserialize_partial(data)?;

    // Fail if data are not consumed entirely.
    if consumed == data.len() {
        Ok(rv)
    } else {
        Err(Error::ParseFailed(
            "data not consumed entirely when explicitly deserializing",
        ))
    }
}

/// Deserialize an object from a vector, but will not report an error if said
/// deserialization doesn't consume the entire vector.
pub fn deserialize_partial<T: Decodable>(data: &[u8]) -> Result<(T, usize)> {
    let mut decoder = Cursor::new(data);
    let rv = Decodable::decode(&mut decoder)?;
    let consumed = decoder.position() as usize;

    Ok((rv, consumed))
}

/// Extensions of `Write` to encode data as per Bitcoin consensus
pub trait WriteExt {
    /// Output a 64-bit uint
    fn write_u64(&mut self, v: u64) -> Result<()>;
    /// Output a 32-bit uint
    fn write_u32(&mut self, v: u32) -> Result<()>;
    /// Output a 16-bit uint
    fn write_u16(&mut self, v: u16) -> Result<()>;
    /// Output a 8-bit uint
    fn write_u8(&mut self, v: u8) -> Result<()>;

    /// Output a 64-bit int
    fn write_i64(&mut self, v: i64) -> Result<()>;
    /// Output a 32-bit int
    fn write_i32(&mut self, v: i32) -> Result<()>;
    /// Output a 16-bit int
    fn write_i16(&mut self, v: i16) -> Result<()>;
    /// Output a 8-bit int
    fn write_i8(&mut self, v: i8) -> Result<()>;

    /// Output a boolean
    fn write_bool(&mut self, v: bool) -> Result<()>;

    /// Output a byte slice
    fn write_slice(&mut self, v: &[u8]) -> Result<()>;
}

/// Extensions of `Read` to decode data as per Bitcoin consensus
pub trait ReadExt {
    /// Read a 64-bit uint
    fn read_u64(&mut self) -> Result<u64>;
    /// Read a 32-bit uint
    fn read_u32(&mut self) -> Result<u32>;
    /// Read a 16-bit uint
    fn read_u16(&mut self) -> Result<u16>;
    /// Read a 8-bit uint
    fn read_u8(&mut self) -> Result<u8>;

    /// Read a 64-bit int
    fn read_i64(&mut self) -> Result<i64>;
    /// Read a 32-bit int
    fn read_i32(&mut self) -> Result<i32>;
    /// Read a 16-bit int
    fn read_i16(&mut self) -> Result<i16>;
    /// Read a 8-bit int
    fn read_i8(&mut self) -> Result<i8>;

    /// Read a boolean
    fn read_bool(&mut self) -> Result<bool>;

    /// Read a byte slice
    fn read_slice(&mut self, slice: &mut [u8]) -> Result<()>;
}

macro_rules! encoder_fn {
    ($name:ident, $val_type:ty, $writefn:ident) => {
        #[inline]
        fn $name(&mut self, v: $val_type) -> Result<()> {
            self.write_all(&endian::$writefn(v))
                .map_err(|e| Error::Io(e.kind()))
        }
    };
}

macro_rules! decoder_fn {
    ($name:ident, $val_type:ty, $readfn:ident, $byte_len: expr) => {
        #[inline]
        fn $name(&mut self) -> Result<$val_type> {
            assert_eq!(::std::mem::size_of::<$val_type>(), $byte_len); // size_of isn't a constfn in 1.22
            let mut val = [0; $byte_len];
            self.read_exact(&mut val[..])
                .map_err(|e| Error::Io(e.kind()))?;
            Ok(endian::$readfn(&val))
        }
    };
}

impl<W: Write> WriteExt for W {
    encoder_fn!(write_u64, u64, u64_to_array_le);
    encoder_fn!(write_u32, u32, u32_to_array_le);
    encoder_fn!(write_u16, u16, u16_to_array_le);
    encoder_fn!(write_i64, i64, i64_to_array_le);
    encoder_fn!(write_i32, i32, i32_to_array_le);
    encoder_fn!(write_i16, i16, i16_to_array_le);

    #[inline]
    fn write_i8(&mut self, v: i8) -> Result<()> {
        self.write_all(&[v as u8]).map_err(|e| Error::Io(e.kind()))
    }
    #[inline]
    fn write_u8(&mut self, v: u8) -> Result<()> {
        self.write_all(&[v]).map_err(|e| Error::Io(e.kind()))
    }
    #[inline]
    fn write_bool(&mut self, v: bool) -> Result<()> {
        self.write_all(&[v as u8]).map_err(|e| Error::Io(e.kind()))
    }
    #[inline]
    fn write_slice(&mut self, v: &[u8]) -> Result<()> {
        self.write_all(v).map_err(|e| Error::Io(e.kind()))
    }
}

impl<R: Read> ReadExt for R {
    decoder_fn!(read_u64, u64, slice_to_u64_le, 8);
    decoder_fn!(read_u32, u32, slice_to_u32_le, 4);
    decoder_fn!(read_u16, u16, slice_to_u16_le, 2);
    decoder_fn!(read_i64, i64, slice_to_i64_le, 8);
    decoder_fn!(read_i32, i32, slice_to_i32_le, 4);
    decoder_fn!(read_i16, i16, slice_to_i16_le, 2);

    #[inline]
    fn read_u8(&mut self) -> Result<u8> {
        let mut slice = [0u8; 1];
        self.read_exact(&mut slice)?;
        Ok(slice[0])
    }
    #[inline]
    fn read_i8(&mut self) -> Result<i8> {
        let mut slice = [0u8; 1];
        self.read_exact(&mut slice)?;
        Ok(slice[0] as i8)
    }
    #[inline]
    fn read_bool(&mut self) -> Result<bool> {
        ReadExt::read_i8(self).map(|bit| bit != 0)
    }
    #[inline]
    fn read_slice(&mut self, slice: &mut [u8]) -> Result<()> {
        self.read_exact(slice).map_err(|e| Error::Io(e.kind()))
    }
}

/// Data which can be encoded in a consensus-consistent way
pub trait Encodable {
    /// Encode an object with a well-defined format, should only ever error if
    /// the underlying `Write` errors. Returns the number of bytes written on
    /// success
    fn encode<W: io::Write>(&self, e: W) -> Result<usize>;
}

/// Data which can be encoded in a consensus-consistent way
pub trait Decodable: Sized {
    /// Decode an object with a well-defined format
    fn decode<D: io::Read>(d: D) -> Result<Self>;
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct VarInt(pub u64);

// Primitive types
macro_rules! impl_int_encodable {
    ($ty:ident, $meth_dec:ident, $meth_enc:ident) => {
        impl Decodable for $ty {
            #[inline]
            fn decode<D: io::Read>(mut d: D) -> Result<Self> {
                ReadExt::$meth_dec(&mut d).map($ty::from_le)
            }
        }
        impl Encodable for $ty {
            #[inline]
            fn encode<S: WriteExt>(&self, mut s: S) -> Result<usize> {
                s.$meth_enc(self.to_le())?;
                Ok(mem::size_of::<$ty>())
            }
        }
    };
}

impl_int_encodable!(u8, read_u8, write_u8);
impl_int_encodable!(u16, read_u16, write_u16);
impl_int_encodable!(u32, read_u32, write_u32);
impl_int_encodable!(u64, read_u64, write_u64);
impl_int_encodable!(i8, read_i8, write_i8);
impl_int_encodable!(i16, read_i16, write_i16);
impl_int_encodable!(i32, read_i32, write_i32);
impl_int_encodable!(i64, read_i64, write_i64);

impl VarInt {
    /// Gets the length of this VarInt when encoded.
    /// Returns 1 for 0...0xFC, 3 for 0xFD...(2^16-1), 5 for 0x10000...(2^32-1),
    /// and 9 otherwise.
    #[inline]
    pub fn len(&self) -> usize {
        match self.0 {
            0..=0xFC => 1,
            0xFD..=0xFFFF => 3,
            0x10000..=0xFFFFFFFF => 5,
            _ => 9,
        }
    }
}

impl Encodable for VarInt {
    #[inline]
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        match self.0 {
            0..=0xFC => {
                (self.0 as u8).encode(s)?;
                Ok(1)
            }
            0xFD..=0xFFFF => {
                s.write_u8(0xFD)?;
                (self.0 as u16).encode(s)?;
                Ok(3)
            }
            0x10000..=0xFFFFFFFF => {
                s.write_u8(0xFE)?;
                (self.0 as u32).encode(s)?;
                Ok(5)
            }
            _ => {
                s.write_u8(0xFF)?;
                (self.0 as u64).encode(s)?;
                Ok(9)
            }
        }
    }
}

impl Decodable for VarInt {
    #[inline]
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let n = ReadExt::read_u8(&mut d)?;
        match n {
            0xFF => {
                let x = ReadExt::read_u64(&mut d)?;
                if x < 0x100000000 {
                    Err(self::Error::NonMinimalVarInt)
                } else {
                    Ok(VarInt(x))
                }
            }
            0xFE => {
                let x = ReadExt::read_u32(&mut d)?;
                if x < 0x10000 {
                    Err(self::Error::NonMinimalVarInt)
                } else {
                    Ok(VarInt(x as u64))
                }
            }
            0xFD => {
                let x = ReadExt::read_u16(&mut d)?;
                if x < 0xFD {
                    Err(self::Error::NonMinimalVarInt)
                } else {
                    Ok(VarInt(x as u64))
                }
            }
            n => Ok(VarInt(n as u64)),
        }
    }
}

// Booleans
impl Encodable for bool {
    #[inline]
    fn encode<S: WriteExt>(&self, mut s: S) -> Result<usize> {
        s.write_bool(*self)?;
        Ok(1)
    }
}

impl Decodable for bool {
    #[inline]
    fn decode<D: io::Read>(mut d: D) -> Result<bool> {
        ReadExt::read_bool(&mut d)
    }
}

// Strings
impl Encodable for String {
    #[inline]
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let b = self.as_bytes();
        let vi_len = VarInt(b.len() as u64).encode(&mut s)?;
        s.write_slice(&b)?;
        Ok(vi_len + b.len())
    }
}

impl Decodable for String {
    #[inline]
    fn decode<D: io::Read>(d: D) -> Result<String> {
        String::from_utf8(Decodable::decode(d)?)
            .map_err(|_| self::Error::ParseFailed("String was not valid UTF8"))
    }
}

// Cow<'static, str>
impl Encodable for Cow<'static, str> {
    #[inline]
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let b = self.as_bytes();
        let vi_len = VarInt(b.len() as u64).encode(&mut s)?;
        s.write_slice(&b)?;
        Ok(vi_len + b.len())
    }
}

impl Decodable for Cow<'static, str> {
    #[inline]
    fn decode<D: io::Read>(d: D) -> Result<Cow<'static, str>> {
        String::from_utf8(Decodable::decode(d)?)
            .map_err(|_| self::Error::ParseFailed("String was not valid UTF8"))
            .map(Cow::Owned)
    }
}

// Arrays
macro_rules! impl_array {
    ( $size:expr ) => {
        impl Encodable for [u8; $size] {
            #[inline]
            fn encode<S: WriteExt>(&self, mut s: S) -> Result<usize> {
                s.write_slice(&self[..])?;
                Ok(self.len())
            }
        }

        impl Decodable for [u8; $size] {
            #[inline]
            fn decode<D: io::Read>(mut d: D) -> Result<Self> {
                let mut ret = [0; $size];
                d.read_slice(&mut ret)?;
                Ok(ret)
            }
        }
    };
}

impl_array!(2);
impl_array!(4);
impl_array!(8);
impl_array!(12);
impl_array!(16);
impl_array!(32);
impl_array!(33);

// Options
impl<T: Encodable> Encodable for Option<T> {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        if let Some(v) = self {
            len += true.encode(&mut s)?;
            len += v.encode(&mut s)?;
        } else {
            len += false.encode(&mut s)?;
        }
        Ok(len)
    }
}
impl<T: Decodable> Decodable for Option<T> {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let valid: bool = Decodable::decode(&mut d)?;
        let mut val: Option<T> = None;

        if valid {
            val = Some(Decodable::decode(&mut d)?);
        }

        Ok(val)
    }
}

impl<T: Encodable> Encodable for Vec<Option<T>> {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += VarInt(self.len() as u64).encode(&mut s)?;
        for val in self {
            len += val.encode(&mut s)?;
        }
        Ok(len)
    }
}
impl<T: Decodable> Decodable for Vec<Option<T>> {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let len = VarInt::decode(&mut d)?.0;
        let mut ret = Vec::with_capacity(len as usize);
        for _ in 0..len {
            ret.push(Decodable::decode(&mut d)?);
        }
        Ok(ret)
    }
}

// Vectors
#[macro_export]
macro_rules! impl_vec {
    ($type: ty) => {
        impl Encodable for Vec<$type> {
            #[inline]
            fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
                let mut len = 0;
                len += VarInt(self.len() as u64).encode(&mut s)?;
                for c in self.iter() {
                    len += c.encode(&mut s)?;
                }
                Ok(len)
            }
        }
        impl Decodable for Vec<$type> {
            #[inline]
            fn decode<D: io::Read>(mut d: D) -> Result<Self> {
                let len = VarInt::decode(&mut d)?.0;
                let mut ret = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    ret.push(Decodable::decode(&mut d)?);
                }
                Ok(ret)
            }
        }
    };
}
impl_vec!(SocketAddr);
impl_vec!([u8; 32]);

impl Encodable for IpAddr {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        match self {
            IpAddr::V4(ip) => {
                let version: u8 = 4;
                len += version.encode(&mut s)?;
                len += ip.octets().encode(s)?;
            }
            IpAddr::V6(ip) => {
                let version: u8 = 6;
                len += version.encode(&mut s)?;
                len += ip.octets().encode(s)?;
            }
        }
        Ok(len)
    }
}

impl Decodable for IpAddr {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let version: u8 = Decodable::decode(&mut d)?;
        match version {
            4 => {
                let addr: [u8; 4] = Decodable::decode(&mut d)?;
                Ok(IpAddr::from(addr))
            }
            6 => {
                let addr: [u8; 16] = Decodable::decode(&mut d)?;
                Ok(IpAddr::from(addr))
            }
            _ => Err(Error::ParseFailed("couldn't decode IpAddr")),
        }
    }
}

impl Encodable for SocketAddr {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.ip().encode(&mut s)?;
        len += self.port().encode(s)?;
        Ok(len)
    }
}

impl Decodable for SocketAddr {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let ip = Decodable::decode(&mut d)?;
        let port: u16 = Decodable::decode(d)?;
        Ok(SocketAddr::new(ip, port))
    }
}

pub fn encode_with_size<S: io::Write>(data: &[u8], mut s: S) -> Result<usize> {
    let vi_len = VarInt(data.len() as u64).encode(&mut s)?;
    s.write_slice(&data)?;
    Ok(vi_len + data.len())
}

impl Encodable for Vec<u8> {
    #[inline]
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        encode_with_size(self, s)
    }
}

impl Decodable for Vec<u8> {
    #[inline]
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let len = VarInt::decode(&mut d)?.0 as usize;
        let mut ret = vec![0u8; len];
        d.read_slice(&mut ret)?;
        Ok(ret)
    }
}

impl Encodable for Box<[u8]> {
    #[inline]
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        encode_with_size(self, s)
    }
}

impl Decodable for Box<[u8]> {
    #[inline]
    fn decode<D: io::Read>(d: D) -> Result<Self> {
        <Vec<u8>>::decode(d).map(From::from)
    }
}

// Tuples
macro_rules! tuple_encode {
($($x:ident),*) => (
impl <$($x: Encodable),*> Encodable for ($($x),*) {
#[inline]
#[allow(non_snake_case)]
fn encode<S: io::Write>(
&self,
mut s: S,
) -> Result<usize> {
let &($(ref $x),*) = self;
let mut len = 0;
$(len += $x.encode(&mut s)?;)*
Ok(len)
}
}

impl<$($x: Decodable),*> Decodable for ($($x),*) {
#[inline]
#[allow(non_snake_case)]
fn decode<D: io::Read>(mut d: D) -> Result<Self> {
Ok(($({let $x = Decodable::decode(&mut d)?; $x }),*))
}
}
);
}

tuple_encode!(T0, T1);
tuple_encode!(T0, T1, T2, T3);
tuple_encode!(T0, T1, T2, T3, T4, T5);
tuple_encode!(T0, T1, T2, T3, T4, T5, T6, T7);

#[cfg(test)]
mod tests {
    use super::{deserialize, serialize, Error, Result, VarInt};
    use super::{deserialize_partial, Encodable};
    use crate::endian::{u16_to_array_le, u32_to_array_le, u64_to_array_le};
    use std::io;
    use std::mem::discriminant;

    #[test]
    fn serialize_int_test() {
        // bool
        assert_eq!(serialize(&false), vec![0u8]);
        assert_eq!(serialize(&true), vec![1u8]);
        // u8
        assert_eq!(serialize(&1u8), vec![1u8]);
        assert_eq!(serialize(&0u8), vec![0u8]);
        assert_eq!(serialize(&255u8), vec![255u8]);
        // u16
        assert_eq!(serialize(&1u16), vec![1u8, 0]);
        assert_eq!(serialize(&256u16), vec![0u8, 1]);
        assert_eq!(serialize(&5000u16), vec![136u8, 19]);
        // u32
        assert_eq!(serialize(&1u32), vec![1u8, 0, 0, 0]);
        assert_eq!(serialize(&256u32), vec![0u8, 1, 0, 0]);
        assert_eq!(serialize(&5000u32), vec![136u8, 19, 0, 0]);
        assert_eq!(serialize(&500000u32), vec![32u8, 161, 7, 0]);
        assert_eq!(serialize(&168430090u32), vec![10u8, 10, 10, 10]);
        // i32
        assert_eq!(serialize(&-1i32), vec![255u8, 255, 255, 255]);
        assert_eq!(serialize(&-256i32), vec![0u8, 255, 255, 255]);
        assert_eq!(serialize(&-5000i32), vec![120u8, 236, 255, 255]);
        assert_eq!(serialize(&-500000i32), vec![224u8, 94, 248, 255]);
        assert_eq!(serialize(&-168430090i32), vec![246u8, 245, 245, 245]);
        assert_eq!(serialize(&1i32), vec![1u8, 0, 0, 0]);
        assert_eq!(serialize(&256i32), vec![0u8, 1, 0, 0]);
        assert_eq!(serialize(&5000i32), vec![136u8, 19, 0, 0]);
        assert_eq!(serialize(&500000i32), vec![32u8, 161, 7, 0]);
        assert_eq!(serialize(&168430090i32), vec![10u8, 10, 10, 10]);
        // u64
        assert_eq!(serialize(&1u64), vec![1u8, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&256u64), vec![0u8, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&5000u64), vec![136u8, 19, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&500000u64), vec![32u8, 161, 7, 0, 0, 0, 0, 0]);
        assert_eq!(
            serialize(&723401728380766730u64),
            vec![10u8, 10, 10, 10, 10, 10, 10, 10]
        );
        // i64
        assert_eq!(
            serialize(&-1i64),
            vec![255u8, 255, 255, 255, 255, 255, 255, 255]
        );
        assert_eq!(
            serialize(&-256i64),
            vec![0u8, 255, 255, 255, 255, 255, 255, 255]
        );
        assert_eq!(
            serialize(&-5000i64),
            vec![120u8, 236, 255, 255, 255, 255, 255, 255]
        );
        assert_eq!(
            serialize(&-500000i64),
            vec![224u8, 94, 248, 255, 255, 255, 255, 255]
        );
        assert_eq!(
            serialize(&-723401728380766730i64),
            vec![246u8, 245, 245, 245, 245, 245, 245, 245]
        );
        assert_eq!(serialize(&1i64), vec![1u8, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&256i64), vec![0u8, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&5000i64), vec![136u8, 19, 0, 0, 0, 0, 0, 0]);
        assert_eq!(serialize(&500000i64), vec![32u8, 161, 7, 0, 0, 0, 0, 0]);
        assert_eq!(
            serialize(&723401728380766730i64),
            vec![10u8, 10, 10, 10, 10, 10, 10, 10]
        );
    }

    #[test]
    fn serialize_varint_test() {
        assert_eq!(serialize(&VarInt(10)), vec![10u8]);
        assert_eq!(serialize(&VarInt(0xFC)), vec![0xFCu8]);
        assert_eq!(serialize(&VarInt(0xFD)), vec![0xFDu8, 0xFD, 0]);
        assert_eq!(serialize(&VarInt(0xFFF)), vec![0xFDu8, 0xFF, 0xF]);
        assert_eq!(
            serialize(&VarInt(0xF0F0F0F)),
            vec![0xFEu8, 0xF, 0xF, 0xF, 0xF]
        );
        assert_eq!(
            serialize(&VarInt(0xF0F0F0F0F0E0)),
            vec![0xFFu8, 0xE0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0, 0]
        );
        assert_eq!(
            test_varint_encode(0xFF, &u64_to_array_le(0x100000000)).unwrap(),
            VarInt(0x100000000)
        );
        assert_eq!(
            test_varint_encode(0xFE, &u64_to_array_le(0x10000)).unwrap(),
            VarInt(0x10000)
        );
        assert_eq!(
            test_varint_encode(0xFD, &u64_to_array_le(0xFD)).unwrap(),
            VarInt(0xFD)
        );

        // Test that length calc is working correctly
        test_varint_len(VarInt(0), 1);
        test_varint_len(VarInt(0xFC), 1);
        test_varint_len(VarInt(0xFD), 3);
        test_varint_len(VarInt(0xFFFF), 3);
        test_varint_len(VarInt(0x10000), 5);
        test_varint_len(VarInt(0xFFFFFFFF), 5);
        test_varint_len(VarInt(0xFFFFFFFF + 1), 9);
        test_varint_len(VarInt(u64::max_value()), 9);
    }

    fn test_varint_len(varint: VarInt, expected: usize) {
        let mut encoder = io::Cursor::new(vec![]);
        assert_eq!(varint.encode(&mut encoder).unwrap(), expected);
        assert_eq!(varint.len(), expected);
    }

    fn test_varint_encode(n: u8, x: &[u8]) -> Result<VarInt> {
        let mut input = [0u8; 9];
        input[0] = n;
        input[1..x.len() + 1].copy_from_slice(x);
        deserialize_partial::<VarInt>(&input).map(|t| t.0)
    }

    #[test]
    fn deserialize_nonminimal_vec() {
        // Check the edges for variant int
        assert_eq!(
            discriminant(&test_varint_encode(0xFF, &u64_to_array_le(0x100000000 - 1)).unwrap_err()),
            discriminant(&Error::NonMinimalVarInt)
        );
        assert_eq!(
            discriminant(&test_varint_encode(0xFE, &u32_to_array_le(0x10000 - 1)).unwrap_err()),
            discriminant(&Error::NonMinimalVarInt)
        );
        assert_eq!(
            discriminant(&test_varint_encode(0xFD, &u16_to_array_le(0xFD - 1)).unwrap_err()),
            discriminant(&Error::NonMinimalVarInt)
        );

        assert_eq!(
            discriminant(&deserialize::<Vec<u8>>(&[0xfd, 0x00, 0x00]).unwrap_err()),
            discriminant(&Error::NonMinimalVarInt)
        );
        assert_eq!(
            discriminant(&deserialize::<Vec<u8>>(&[0xfd, 0xfc, 0x00]).unwrap_err()),
            discriminant(&Error::NonMinimalVarInt)
        );
        assert_eq!(
            discriminant(&deserialize::<Vec<u8>>(&[0xfd, 0xfc, 0x00]).unwrap_err()),
            discriminant(&Error::NonMinimalVarInt)
        );
        assert_eq!(
            discriminant(&deserialize::<Vec<u8>>(&[0xfe, 0xff, 0x00, 0x00, 0x00]).unwrap_err()),
            discriminant(&Error::NonMinimalVarInt)
        );
        assert_eq!(
            discriminant(&deserialize::<Vec<u8>>(&[0xfe, 0xff, 0xff, 0x00, 0x00]).unwrap_err()),
            discriminant(&Error::NonMinimalVarInt)
        );
        assert_eq!(
            discriminant(
                &deserialize::<Vec<u8>>(&[0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
                    .unwrap_err()
            ),
            discriminant(&Error::NonMinimalVarInt)
        );
        assert_eq!(
            discriminant(
                &deserialize::<Vec<u8>>(&[0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00])
                    .unwrap_err()
            ),
            discriminant(&Error::NonMinimalVarInt)
        );

        let mut vec_256 = vec![0; 259];
        vec_256[0] = 0xfd;
        vec_256[1] = 0x00;
        vec_256[2] = 0x01;
        assert!(deserialize::<Vec<u8>>(&vec_256).is_ok());

        let mut vec_253 = vec![0; 256];
        vec_253[0] = 0xfd;
        vec_253[1] = 0xfd;
        vec_253[2] = 0x00;
        assert!(deserialize::<Vec<u8>>(&vec_253).is_ok());
    }

    #[test]
    fn serialize_vector_test() {
        assert_eq!(serialize(&vec![1u8, 2, 3]), vec![3u8, 1, 2, 3]);
        // TODO: test vectors of more interesting objects
    }

    #[test]
    fn serialize_strbuf_test() {
        assert_eq!(
            serialize(&"Andrew".to_string()),
            vec![6u8, 0x41, 0x6e, 0x64, 0x72, 0x65, 0x77]
        );
    }

    #[test]
    fn deserialize_int_test() {
        // bool
        assert!((deserialize(&[58u8, 0]) as Result<bool>).is_err());
        assert_eq!(deserialize(&[58u8]).ok(), Some(true));
        assert_eq!(deserialize(&[1u8]).ok(), Some(true));
        assert_eq!(deserialize(&[0u8]).ok(), Some(false));
        assert!((deserialize(&[0u8, 1]) as Result<bool>).is_err());

        // u8
        assert_eq!(deserialize(&[58u8]).ok(), Some(58u8));

        // u16
        assert_eq!(deserialize(&[0x01u8, 0x02]).ok(), Some(0x0201u16));
        assert_eq!(deserialize(&[0xABu8, 0xCD]).ok(), Some(0xCDABu16));
        assert_eq!(deserialize(&[0xA0u8, 0x0D]).ok(), Some(0xDA0u16));
        let failure16: Result<u16> = deserialize(&[1u8]);
        assert!(failure16.is_err());

        // u32
        assert_eq!(deserialize(&[0xABu8, 0xCD, 0, 0]).ok(), Some(0xCDABu32));
        assert_eq!(
            deserialize(&[0xA0u8, 0x0D, 0xAB, 0xCD]).ok(),
            Some(0xCDAB0DA0u32)
        );
        let failure32: Result<u32> = deserialize(&[1u8, 2, 3]);
        assert!(failure32.is_err());
        // TODO: test negative numbers
        assert_eq!(deserialize(&[0xABu8, 0xCD, 0, 0]).ok(), Some(0xCDABi32));
        assert_eq!(
            deserialize(&[0xA0u8, 0x0D, 0xAB, 0x2D]).ok(),
            Some(0x2DAB0DA0i32)
        );
        let failurei32: Result<i32> = deserialize(&[1u8, 2, 3]);
        assert!(failurei32.is_err());

        // u64
        assert_eq!(
            deserialize(&[0xABu8, 0xCD, 0, 0, 0, 0, 0, 0]).ok(),
            Some(0xCDABu64)
        );
        assert_eq!(
            deserialize(&[0xA0u8, 0x0D, 0xAB, 0xCD, 0x99, 0, 0, 0x99]).ok(),
            Some(0x99000099CDAB0DA0u64)
        );
        let failure64: Result<u64> = deserialize(&[1u8, 2, 3, 4, 5, 6, 7]);
        assert!(failure64.is_err());
        // TODO: test negative numbers
        assert_eq!(
            deserialize(&[0xABu8, 0xCD, 0, 0, 0, 0, 0, 0]).ok(),
            Some(0xCDABi64)
        );
        assert_eq!(
            deserialize(&[0xA0u8, 0x0D, 0xAB, 0xCD, 0x99, 0, 0, 0x99]).ok(),
            Some(-0x66ffff663254f260i64)
        );
        let failurei64: Result<i64> = deserialize(&[1u8, 2, 3, 4, 5, 6, 7]);
        assert!(failurei64.is_err());
    }

    #[test]
    fn deserialize_vec_test() {
        assert_eq!(deserialize(&[3u8, 2, 3, 4]).ok(), Some(vec![2u8, 3, 4]));
        assert!((deserialize(&[4u8, 2, 3, 4, 5, 6]) as Result<Vec<u8>>).is_err());
    }

    #[test]
    fn deserialize_strbuf_test() {
        assert_eq!(
            deserialize(&[6u8, 0x41, 0x6e, 0x64, 0x72, 0x65, 0x77]).ok(),
            Some("Andrew".to_string())
        );
        assert_eq!(
            deserialize(&[6u8, 0x41, 0x6e, 0x64, 0x72, 0x65, 0x77]).ok(),
            Some(::std::borrow::Cow::Borrowed("Andrew"))
        );
    }
}
