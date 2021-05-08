use rand::Rng;
use std::fs::File;
use std::io::Read;
use std::rc::Rc;

use std::time::{SystemTime, UNIX_EPOCH};

use crate::printer::pr_seq;
use crate::reader::read_str;
use crate::types::MalErr::ErrMalVal;
use crate::types::MalVal::{
    Atom, Bool, Func, Hash, Int, List, MalFunc, Nil, Str, Sym, Vector, ZKScalar,
};
use crate::types::{MalArgs, MalRet, MalVal, _assoc, _dissoc, atom, error, func, hash_map};

use bls12_381;
use ff::{Field, PrimeField};
use rand::rngs::OsRng;

use sapvi::bls_extensions::BlsStringConversion;

use std::ops::{AddAssign, MulAssign, SubAssign};

macro_rules! fn_t_int_int {
    ($ret:ident, $fn:expr) => {{
        |a: MalArgs| match (a[0].clone(), a[1].clone()) {
            (Int(a0), Int(a1)) => Ok($ret($fn(a0, a1))),
            _ => error("expecting (int,int) args"),
        }
    }};
}

macro_rules! fn_is_type {
  ($($ps:pat),*) => {{
    |a:MalArgs| { Ok(Bool(match a[0] { $($ps => true,)* _ => false})) }
  }};
  ($p:pat if $e:expr) => {{
    |a:MalArgs| { Ok(Bool(match a[0] { $p if $e => true, _ => false})) }
  }};
  ($p:pat if $e:expr,$($ps:pat),*) => {{
    |a:MalArgs| { Ok(Bool(match a[0] { $p if $e => true, $($ps => true,)* _ => false})) }
  }};
}

macro_rules! fn_str {
    ($fn:expr) => {{
        |a: MalArgs| match a[0].clone() {
            Str(a0) => $fn(a0),
            _ => error("expecting (str) arg"),
        }
    }};
}

fn symbol(a: MalArgs) -> MalRet {
    match a[0] {
        Str(ref s) => Ok(Sym(s.to_string())),
        _ => error("illegal symbol call"),
    }
}

fn slurp(f: String) -> MalRet {
    let mut s = String::new();
    match File::open(f).and_then(|mut f| f.read_to_string(&mut s)) {
        Ok(_) => Ok(Str(s)),
        Err(e) => error(&e.to_string()),
    }
}

fn time_ms(_a: MalArgs) -> MalRet {
    let ms_e = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d,
        Err(e) => return error(&format!("{:?}", e)),
    };
    Ok(Int(
        ms_e.as_secs() as i64 * 1000 + ms_e.subsec_nanos() as i64 / 1_000_000
    ))
}

fn get(a: MalArgs) -> MalRet {
    match (a[0].clone(), a[1].clone()) {
        (Nil, _) => Ok(Nil),
        (Hash(ref hm, _), Str(ref s)) => match hm.get(s) {
            Some(mv) => Ok(mv.clone()),
            None => Ok(Nil),
        },
        _ => error("illegal get args"),
    }
}

fn assoc(a: MalArgs) -> MalRet {
    match a[0] {
        Hash(ref hm, _) => _assoc((**hm).clone(), a[1..].to_vec()),
        _ => error("assoc on non-Hash Map"),
    }
}

fn dissoc(a: MalArgs) -> MalRet {
    match a[0] {
        Hash(ref hm, _) => _dissoc((**hm).clone(), a[1..].to_vec()),
        _ => error("dissoc on non-Hash Map"),
    }
}

fn contains_q(a: MalArgs) -> MalRet {
    match (a[0].clone(), a[1].clone()) {
        (Hash(ref hm, _), Str(ref s)) => Ok(Bool(hm.contains_key(s))),
        _ => error("illegal get args"),
    }
}

fn keys(a: MalArgs) -> MalRet {
    match a[0] {
        Hash(ref hm, _) => Ok(list!(hm.keys().map(|k| { Str(k.to_string()) }).collect())),
        _ => error("keys requires Hash Map"),
    }
}

fn vals(a: MalArgs) -> MalRet {
    match a[0] {
        Hash(ref hm, _) => Ok(list!(hm.values().map(|v| { v.clone() }).collect())),
        _ => error("keys requires Hash Map"),
    }
}

fn vec(a: MalArgs) -> MalRet {
    match a[0] {
        List(ref v, _) | Vector(ref v, _) => Ok(vector!(v.to_vec())),
        _ => error("non-seq passed to vec"),
    }
}

fn cons(a: MalArgs) -> MalRet {
    match a[1].clone() {
        List(v, _) | Vector(v, _) => {
            let mut new_v = vec![a[0].clone()];
            new_v.extend_from_slice(&v);
            Ok(list!(new_v.to_vec()))
        }
        _ => error("cons expects seq as second arg"),
    }
}

fn concat(a: MalArgs) -> MalRet {
    let mut new_v = vec![];
    for seq in a.iter() {
        match seq {
            List(v, _) | Vector(v, _) => new_v.extend_from_slice(v),
            _ => return error("non-seq passed to concat"),
        }
    }
    Ok(list!(new_v.to_vec()))
}

fn nth(a: MalArgs) -> MalRet {
    match (a[0].clone(), a[1].clone()) {
        (List(seq, _), Int(idx)) | (Vector(seq, _), Int(idx)) => {
            if seq.len() <= idx as usize {
                return error("nth: index out of range");
            }
            Ok(seq[idx as usize].clone())
        }
        _ => error("invalid args to nth"),
    }
}

fn unpack_bits(a: MalArgs) -> MalRet {
    let mut result = vec![];
    match a[0].clone() {
        Str(ref s) => {
            let value = bls12_381::Scalar::from_string(s);
            for (_, bit) in value.to_le_bits().into_iter().enumerate() {
                match bit {
                    true => result.push(bls12_381::Scalar::one()),
                    false => result.push(bls12_381::Scalar::zero()),
                }
            }
            Ok(list!(result
                .iter()
                .map(|a| Str(std::string::ToString::to_string(&a)[2..].to_string()))
                .collect::<Vec<MalVal>>()))
        }
        ZKScalar(ref s) => {
            for (_, bit) in s.to_le_bits().into_iter().enumerate() {
                match bit {
                    true => result.push(bls12_381::Scalar::one()),
                    false => result.push(bls12_381::Scalar::zero()),
                }
            }
            Ok(list!(result
                .iter()
                .map(|a| Str(std::string::ToString::to_string(&a)[2..].to_string()))
                .collect::<Vec<MalVal>>()))
        }
        _ => error(&format!("invalid args to unpack-bits found \n {:?}", a).to_string()),
    }
}

fn last(a: MalArgs) -> MalRet {
    match a[0].clone() {
        List(ref seq, _) | Vector(ref seq, _) if seq.len() == 0 => Ok(Nil),
        List(ref seq, _) | Vector(ref seq, _) => Ok(seq[seq.len() - 1].clone()),
        Nil => Ok(Nil),
        _ => error("invalid args to first"),
    }
}

fn first(a: MalArgs) -> MalRet {
    match a[0].clone() {
        List(ref seq, _) | Vector(ref seq, _) if seq.len() == 0 => Ok(Nil),
        List(ref seq, _) | Vector(ref seq, _) => Ok(seq[0].clone()),
        Nil => Ok(Nil),
        _ => error("invalid args to first"),
    }
}

fn second(a: MalArgs) -> MalRet {
    match a[0].clone() {
        List(ref seq, _) | Vector(ref seq, _) if seq.len() < 2 => Ok(Nil),
        List(ref seq, _) | Vector(ref seq, _) => Ok(seq[1].clone()),
        Nil => Ok(Nil),
        _ => error("invalid args to first"),
    }
}

fn rest(a: MalArgs) -> MalRet {
    match a[0].clone() {
        List(ref seq, _) | Vector(ref seq, _) => {
            if seq.len() > 1 {
                Ok(list!(seq[1..].to_vec()))
            } else {
                Ok(list![])
            }
        }
        Nil => Ok(list![]),
        _ => error("invalid args to first"),
    }
}

fn apply(a: MalArgs) -> MalRet {
    match a[a.len() - 1] {
        List(ref v, _) | Vector(ref v, _) => {
            let f = &a[0];
            let mut fargs = a[1..a.len() - 1].to_vec();
            fargs.extend_from_slice(&v);
            f.apply(fargs)
        }
        _ => error("apply called with non-seq"),
    }
}

fn map(a: MalArgs) -> MalRet {
    match a[1] {
        List(ref v, _) | Vector(ref v, _) => {
            let mut res = vec![];
            for mv in v.iter() {
                res.push(a[0].apply(vec![mv.clone()])?)
            }
            Ok(list!(res))
        }
        _ => error("map called with non-seq"),
    }
}

fn conj(a: MalArgs) -> MalRet {
    match a[0] {
        List(ref v, _) => {
            let sl = a[1..]
                .iter()
                .rev()
                .map(|a| a.clone())
                .collect::<Vec<MalVal>>();
            Ok(list!([&sl[..], v].concat()))
        }
        Vector(ref v, _) => Ok(vector!([v, &a[1..]].concat())),
        _ => error("conj: called with non-seq"),
    }
}

fn sub_scalar(a: MalArgs) -> MalRet {
    match (a[0].clone(), a[1].clone()) {
        (Func(_, _), ZKScalar(a1)) => {
            if let Vector(ref values, _) = a[0].apply(vec![]).unwrap() {
                if let ZKScalar(mut a0) = values[0] {
                    a0.sub_assign(a1);
                    Ok(ZKScalar(a0))
                } else {
                    error("scalar sub expect (zkscalar, zkscalar) found (func, zkscalar)")
                }
            } else {
                error("scalar sub expect (zkscalar, zkscalar)")
            }
        }
        (Func(_, _), Str(a1)) => {
            if let Vector(ref values, _) = a[0].apply(vec![]).unwrap() {
                let s1 = bls12_381::Scalar::from_string(&a1);
                if let ZKScalar(mut a0) = values[0] {
                    a0.sub_assign(s1);
                    Ok(ZKScalar(a0))
                } else {
                    error("scalar sub expect (zkscalar, zkscalar) found (func, zkscalar)")
                }
            } else {
                error("scalar sub expect (zkscalar, zkscalar)")
            }
        }
        (ZKScalar(mut a0), ZKScalar(a1)) => {
            a0.sub_assign(a1);
            Ok(ZKScalar(a0))
        }
        (Str(a0), ZKScalar(a1)) => {
            let (mut s0, s1) = (bls12_381::Scalar::from_string(&a0), a1);
            s0.sub_assign(s1);
            Ok(Str(std::string::ToString::to_string(&s0)[2..].to_string()))
        }
        (ZKScalar(a0), Str(a1)) => {
            let (mut s0, s1) = (a0, bls12_381::Scalar::from_string(&a1));
            s0.sub_assign(s1);
            Ok(Str(std::string::ToString::to_string(&s0)[2..].to_string()))
        }
        (Str(a0), Str(a1)) => {
            let (mut s0, s1) = (
                bls12_381::Scalar::from_string(&a0),
                bls12_381::Scalar::from_string(&a1),
            );
            s0.sub_assign(s1);
            Ok(Str(std::string::ToString::to_string(&s0)[2..].to_string()))
        }
        _ => error(&format!("scalar sub expect (zkscalar, zkscalar) found \n {:?}", a).to_string()),
    }
}

fn mul_scalar(a: MalArgs) -> MalRet {
    println!("mul {:?}", a[0]);
    match (a[0].clone(), a[1].clone()) {
        (Func(_, _), ZKScalar(a1)) => {
            if let Vector(ref values, _) = a[0].apply(vec![]).unwrap() {
                if let ZKScalar(mut a0) = values[0] {
                    a0.mul_assign(a1);
                    Ok(ZKScalar(a0))
                } else {
                    error("scalar mul expect (zkscalar, zkscalar) found (func, zkscalar)")
                }
            } else {
                error("scalar mul expect (zkscalar, zkscalar)")
            }
        }
        (ZKScalar(a1), Func(_, _)) => {
            if let Vector(ref values, _) = a[1].apply(vec![]).unwrap() {
                if let ZKScalar(mut a0) = values[0] {
                    a0.mul_assign(a1);
                    Ok(ZKScalar(a0))
                } else {
                    error("scalar mul expect (zkscalar, zkscalar) found (func, zkscalar)")
                }
            } else {
                error("scalar mul expect (zkscalar, zkscalar)")
            }
        }
        (ZKScalar(mut a0), ZKScalar(a1)) => {
            a0.mul_assign(a1);
            Ok(ZKScalar(a0))
        }
        (Str(a0), Str(a1)) => {
            let (mut s0, s1) = (
                bls12_381::Scalar::from_string(&a0),
                bls12_381::Scalar::from_string(&a1),
            );
            s0.mul_assign(s1);
            Ok(Str(std::string::ToString::to_string(&s0)[2..].to_string()))
        }
        (ZKScalar(a0), Str(a1)) => {
            let (mut s0, s1) = (a0, bls12_381::Scalar::from_string(&a1));
            s0.mul_assign(s1);
            Ok(Str(std::string::ToString::to_string(&s0)[2..].to_string()))
        }
        (Str(a0), ZKScalar(a1)) => {
            let (mut s0, s1) = (bls12_381::Scalar::from_string(&a0), a1);
            s0.mul_assign(s1);
            Ok(Str(std::string::ToString::to_string(&s0)[2..].to_string()))
        }
        _ => error(&format!("scalar mul expect (zkscalar, zkscalar) \n {:?}", a).to_string()),
    }
}

fn div_scalar(a: MalArgs) -> MalRet {
    match (a[0].clone(), a[1].clone()) {
        (ZKScalar(s0), ZKScalar(s1)) => {
            let ret = s1.invert().map(|other| *&s0 * other);
            if bool::from(ret.is_some()) {
                Ok(Str(
                    std::string::ToString::to_string(&ret.unwrap())[2..].to_string()
                ))
            } else {
                error("DivisionByZero")
            }
        }
        (Str(a0), ZKScalar(a1)) => {
            let (s0, s1) = (bls12_381::Scalar::from_string(&a0), a1);
            let ret = s1.invert().map(|other| *&s0 * other);
            if bool::from(ret.is_some()) {
                Ok(Str(
                    std::string::ToString::to_string(&ret.unwrap())[2..].to_string()
                ))
            } else {
                error("DivisionByZero")
            }
        }
        (Str(a0), Str(a1)) => {
            let (s0, s1) = (
                bls12_381::Scalar::from_string(&a0),
                bls12_381::Scalar::from_string(&a1),
            );
            let ret = s1.invert().map(|other| *&s0 * other);
            if bool::from(ret.is_some()) {
                Ok(Str(
                    std::string::ToString::to_string(&ret.unwrap())[2..].to_string()
                ))
            } else {
                error("DivisionByZero")
            }
        }
        _ => error(&format!("scalar div expect (zkscalar, zkscalar) \n {:?}", a).to_string()),
    }
}

fn range(a: MalArgs) -> MalRet {
    let mut result = vec![];
    match (a[0].clone(), a[1].clone()) {
        (Int(a0), Int(a1)) => {
            for n in a0..a1 {
                result.push(n);
            }
            Ok(list!(result.iter().map(|_a| Nil).collect::<Vec<MalVal>>()))
        }
        _ => error("expected int int"),
    }
}

fn scalar_zero(a: MalArgs) -> MalRet {
    match a.len() {
        0 => Ok(vector![vec![ZKScalar(bls12_381::Scalar::zero())]]),
        _ => Ok(vector![vec![
            ZKScalar(bls12_381::Scalar::zero()),
            a[0].clone()
        ]]),
    }
}

fn scalar_one(a: MalArgs) -> MalRet {
    match a.len() {
        0 => Ok(vector![vec![ZKScalar(bls12_381::Scalar::one())]]),
        _ => Ok(vector![vec![
            ZKScalar(bls12_381::Scalar::one()),
            a[0].clone()
        ]]),
    }
}

fn scalar_one_neg(a: MalArgs) -> MalRet {
    match a.len() {
        0 => Ok(vector![vec![ZKScalar(bls12_381::Scalar::one().neg())]]),
        _ => Ok(vector![vec![
            ZKScalar(bls12_381::Scalar::one().neg()),
            a[0].clone()
        ]]),
    }
}

fn cs_one(_a: MalArgs) -> MalRet {
    Ok(vector![vec![Sym("cs::one".to_string())]])
}

fn negate_from(a: MalArgs) -> MalRet {
    match a[0].clone() {
        ZKScalar(a0) => Ok(ZKScalar(a0.neg())),
        _ => match a[0].apply(vec![])? {
            List(v, _) | Vector(v, _) => match v[0] {
                ZKScalar(val) => Ok(vector![vec![ZKScalar(val.neg())]]),
                _ => error("not scalar"),
            },
            _ => return error("non zkscalar passed to negate"),
        },
    }
}

fn scalar_from(a: MalArgs) -> MalRet {
    match a[0].clone() {
        ZKScalar(s0) => Ok(ZKScalar(s0)),
        Str(a0) => {
            let s0 = bls12_381::Scalar::from_string(&a0.to_string());
            Ok(ZKScalar(s0))
        }
        Int(a0) => {
            let s0 = bls12_381::Scalar::from(a0 as u64);
            Ok(ZKScalar(s0))
        }
        _ => error("scalar from expected (string or int)"),
    }
}

fn scalar_square(a: MalArgs) -> MalRet {
    match a[0].clone() {
        ZKScalar(a0) => {
            let z0 = a0.clone();
            Ok(ZKScalar(z0.square()))
        }
        Str(a0) => {
            let s0 = bls12_381::Scalar::from_string(&a0);
            Ok(ZKScalar(s0.square()))
        }
        _ => error(
            &format!("scalar square expect (zkscalar or string) found \n {:?}", a).to_string(),
        ),
    }
}

fn scalar_double(a: MalArgs) -> MalRet {
    match a[0].clone() {
        Func(_, _) => {
            if let Vector(ref values, _) = a[0].apply(vec![]).unwrap() {
                if let ZKScalar(a0) = values[0] {
                    a0.double();
                    Ok(ZKScalar(a0))
                } else {
                    error(
                        &format!("scalar double expect (zkscalar or string) found \n {:?}", a)
                            .to_string(),
                    )
                }
            } else {
                error(
                    &format!("scalar double expect (zkscalar or string) found \n {:?}", a)
                        .to_string(),
                )
            }
        }
        ZKScalar(a0) => {
            let z0 = a0.clone();
            Ok(ZKScalar(z0.double()))
        }
        Str(a0) => {
            let s0 = bls12_381::Scalar::from_string(&a0);
            Ok(ZKScalar(s0.double()))
        }
        _ => error(
            &format!("scalar double expect (zkscalar or string) found \n {:?}", a).to_string(),
        ),
    }
}

fn scalar_invert(a: MalArgs) -> MalRet {
    match a[0].clone() {
        Func(_, _) => {
            if let Vector(ref values, _) = a[0].apply(vec![]).unwrap() {
                if let ZKScalar(a0) = values[0] {
                    if a0.is_zero() {
                        error(&format!("scalar invert divizion by zero \n {:?}", a0).to_string())
                    } else {
                        Ok(ZKScalar(a0.invert().unwrap()))
                    }
                } else {
                    error(
                        &format!("scalar invert expect (zkscalar or string) found \n {:?}", a)
                            .to_string(),
                    )
                }
            } else {
                error(
                    &format!("scalar invert expect (zkscalar or string) found \n {:?}", a)
                        .to_string(),
                )
            }
        }
        ZKScalar(a0) => {
            let z0 = a0.clone();
            Ok(ZKScalar(z0.invert().unwrap()))
        }
        Str(a0) => {
            let s0 = bls12_381::Scalar::from_string(&a0);
            Ok(ZKScalar(s0.invert().unwrap()))
        }
        _ => error(
            &format!("scalar invert expect (zkscalar or string) found \n {:?}", a).to_string(),
        ),
    }
}

fn scalar_is_zero(a: MalArgs) -> MalRet {
    match a[0].clone() {
        Func(_, _) => {
            if let Vector(ref values, _) = a[0].apply(vec![]).unwrap() {
                if let ZKScalar(a0) = values[0] {
                    Ok(Bool(a0.is_zero()))
                } else {
                    error(
                        &format!(
                            "scalar is zero expect (zkscalar or string) found \n {:?}",
                            a
                        )
                        .to_string(),
                    )
                }
            } else {
                error(
                    &format!(
                        "scalar is zero expect (zkscalar or string) found \n {:?}",
                        a
                    )
                    .to_string(),
                )
            }
        }
        ZKScalar(a0) => {
            let z0 = a0.clone();
            Ok(Bool(z0.is_zero()))
        }
        Str(a0) => {
            let s0 = bls12_381::Scalar::from_string(&a0);
            Ok(Bool(s0.is_zero()))
        }
        _ => error(
            &format!(
                "scalar is zero expect (zkscalar or string) found \n {:?}",
                a
            )
            .to_string(),
        ),
    }
}

fn add_scalar(a: MalArgs) -> MalRet {
    println!("add_scalar {:?}", a);
    match (a[0].clone(), a[1].clone()) {
        (Func(_, _), ZKScalar(a1)) => {
            if let Vector(ref values, _) = a[0].apply(vec![]).unwrap() {
                if let ZKScalar(mut a0) = values[0] {
                    a0.add_assign(a1);
                    Ok(ZKScalar(a0))
                } else {
                    error("scalar add expect (zkscalar, zkscalar) found (func, zkscalar)")
                }
            } else {
                error("scalar add expect (zkscalar, zkscalar)")
            }
        }
        (Func(_, _), Str(a1)) => {
            if let Vector(ref values, _) = a[0].apply(vec![]).unwrap() {
                if let ZKScalar(mut a0) = values[0] {
                    let s1 = bls12_381::Scalar::from_string(&a1);
                    a0.add_assign(s1);
                    Ok(ZKScalar(a0))
                } else {
                    error("scalar add expect (zkscalar, zkscalar) found (func, zkscalar)")
                }
            } else {
                error("scalar add expect (zkscalar, zkscalar)")
            }
        }
        (ZKScalar(a0), ZKScalar(a1)) => {
            let (mut z0, z1) = (a0.clone(), a1.clone());
            z0.add_assign(z1);
            Ok(ZKScalar(z0))
        }
        (Str(a0), Str(a1)) => {
            let (mut s0, s1) = (
                bls12_381::Scalar::from_string(&a0),
                bls12_381::Scalar::from_string(&a1),
            );
            s0.add_assign(s1);
            Ok(ZKScalar(s0))
        }
        (Str(a0), ZKScalar(a1)) => {
            let (mut s0, s1) = (bls12_381::Scalar::from_string(&a0), a1);
            s0.add_assign(s1);
            Ok(ZKScalar(s0))
        }
        (ZKScalar(a1), Str(a0)) => {
            let (mut s0, s1) = (bls12_381::Scalar::from_string(&a0), a1);
            s0.add_assign(s1);
            Ok(ZKScalar(s0))
        }
        // (List(a0, _), ZKScalar(mut a1)) => {
        //     let first_slice = a0.to_vec();
        //     let result = first_slice[0].apply(first_slice[1..].to_vec());
        //     println!("result {:?}", result);
        //     if let ZKScalar(value) = result.unwrap() {
        //         a1.add_assign(value);
        //     }
        //     Ok(ZKScalar(a1))
        // }
        _ => error(&format!("scalar add expect (zkscalar, zkscalar) found \n {:?}", a).to_string()),
    }
}

fn seq(a: MalArgs) -> MalRet {
    match a[0] {
        List(ref v, _) | Vector(ref v, _) if v.len() == 0 => Ok(Nil),
        List(ref v, _) | Vector(ref v, _) => Ok(list!(v.to_vec())),
        Str(ref s) if s.len() == 0 => Ok(Nil),
        Str(ref s) if !a[0].keyword_q() => {
            Ok(list!(s.chars().map(|c| { Str(c.to_string()) }).collect()))
        }
        Nil => Ok(Nil),
        _ => error("seq: called with non-seq"),
    }
}

fn gen_rand(a: MalArgs) -> MalRet {
    let mut rng = rand::thread_rng();
    Ok(MalVal::Int(rng.gen::<i64>()))
}

fn scalar_rnd(a: MalArgs) -> MalRet {
    let randomness_value: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
    let value = bls12_381::Scalar::from_bytes(&randomness_value.to_bytes());
    Ok(MalVal::ZKScalar(value.unwrap()))
}

pub fn ns() -> Vec<(&'static str, MalVal)> {
    vec![
        ("=", func(|a| Ok(Bool(a[0] == a[1])))),
        ("throw", func(|a| Err(ErrMalVal(a[0].clone())))),
        ("nil?", func(fn_is_type!(Nil))),
        ("true?", func(fn_is_type!(Bool(true)))),
        ("false?", func(fn_is_type!(Bool(false)))),
        ("symbol", func(symbol)),
        ("symbol?", func(fn_is_type!(Sym(_)))),
        (
            "string?",
            func(fn_is_type!(Str(ref s) if !s.starts_with("\u{29e}"))),
        ),
        ("keyword", func(|a| a[0].keyword())),
        (
            "keyword?",
            func(fn_is_type!(Str(ref s) if s.starts_with("\u{29e}"))),
        ),
        ("number?", func(fn_is_type!(Int(_)))),
        (
            "fn?",
            func(fn_is_type!(MalFunc{is_macro,..} if !is_macro,Func(_,_))),
        ),
        (
            "macro?",
            func(fn_is_type!(MalFunc{is_macro,..} if is_macro)),
        ),
        ("pr-str", func(|a| Ok(Str(pr_seq(&a, true, "", "", " "))))),
        ("str", func(|a| Ok(Str(pr_seq(&a, false, "", "", ""))))),
        (
            "prn",
            func(|a| {
                println!("{}", pr_seq(&a, true, "", "", " "));
                Ok(Nil)
            }),
        ),
        (
            "println",
            func(|a| {
                println!("{}", pr_seq(&a, false, "", "", " "));
                Ok(Nil)
            }),
        ),
        ("read-string", func(fn_str!(|s| { read_str(s) }))),
        ("slurp", func(fn_str!(|f| { slurp(f) }))),
        ("<", func(fn_t_int_int!(Bool, |i, j| { i < j }))),
        ("<=", func(fn_t_int_int!(Bool, |i, j| { i <= j }))),
        (">", func(fn_t_int_int!(Bool, |i, j| { i > j }))),
        (">=", func(fn_t_int_int!(Bool, |i, j| { i >= j }))),
        ("+", func(add_scalar)),
        ("-", func(sub_scalar)),
        ("*", func(mul_scalar)),
        ("/", func(div_scalar)),
        ("time-ms", func(time_ms)),
        ("i+", func(fn_t_int_int!(Int, |i, j| { i + j }))),
        ("i-", func(fn_t_int_int!(Int, |i, j| { i - j }))),
        ("i*", func(fn_t_int_int!(Int, |i, j| { i * j }))),
        ("i/", func(fn_t_int_int!(Int, |i, j| { i / j }))),
        ("i<", func(fn_t_int_int!(Bool, |i, j| { i < j }))),
        ("i<=", func(fn_t_int_int!(Bool, |i, j| { i <= j }))),
        ("i>", func(fn_t_int_int!(Bool, |i, j| { i > j }))),
        ("i>=", func(fn_t_int_int!(Bool, |i, j| { i >= j }))),
        ("time-ms", func(time_ms)),
        ("sequential?", func(fn_is_type!(List(_, _), Vector(_, _)))),
        ("list", func(|a| Ok(list!(a)))),
        ("list?", func(fn_is_type!(List(_, _)))),
        ("vector", func(|a| Ok(vector!(a)))),
        ("vector?", func(fn_is_type!(Vector(_, _)))),
        ("hash-map", func(|a| hash_map(a))),
        ("map?", func(fn_is_type!(Hash(_, _)))),
        ("assoc", func(assoc)),
        ("dissoc", func(dissoc)),
        ("get", func(get)),
        ("contains?", func(contains_q)),
        ("keys", func(keys)),
        ("vals", func(vals)),
        ("vec", func(vec)),
        ("cons", func(cons)),
        ("concat", func(concat)),
        ("empty?", func(|a| a[0].empty_q())),
        ("nth", func(nth)),
        ("first", func(first)),
        ("last", func(last)),
        ("rest", func(rest)),
        ("count", func(|a| a[0].count())),
        ("apply", func(apply)),
        ("map", func(map)),
        ("conj", func(conj)),
        ("seq", func(seq)),
        ("meta", func(|a| a[0].get_meta())),
        ("with-meta", func(|a| a[0].clone().with_meta(&a[1]))),
        ("atom", func(|a| Ok(atom(&a[0])))),
        ("atom?", func(fn_is_type!(Atom(_)))),
        ("deref", func(|a| a[0].deref())),
        ("reset!", func(|a| a[0].reset_bang(&a[1]))),
        ("swap!", func(|a| a[0].swap_bang(&a[1..].to_vec()))),
        ("unpack-bits", func(unpack_bits)),
        ("range", func(range)),
        ("scalar::one", func(scalar_one)),
        ("scalar::one::neg", func(scalar_one_neg)),
        ("neg", func(negate_from)),
        ("scalar::zero", func(scalar_zero)),
        ("scalar", func(scalar_from)),
        ("square", func(scalar_square)),
        ("cs::one", func(cs_one)),
        ("second", func(second)),
        ("genrand", func(gen_rand)),
        ("double", func(scalar_double)),
        ("invert", func(scalar_invert)),
        ("zero?", func(scalar_is_zero)),
        ("rnd-scalar", func(scalar_rnd)),
    ]
}
