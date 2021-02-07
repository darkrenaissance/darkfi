use bellman::{gadgets::Assignment, groth16, Circuit, ConstraintSystem, SynthesisError};
use sapvi::bls_extensions::BlsStringConversion;
use std::cell::RefCell;
use std::ops::{Add, AddAssign, MulAssign, SubAssign};
use std::rc::Rc;
//use std::collections::HashMap;
use fnv::FnvHashMap;
use itertools::Itertools;

use crate::env::{env_bind, Env};
use crate::types::MalErr::{ErrMalVal, ErrString};
use crate::types::MalVal::{Atom, Bool, Func, Hash, Int, List, MalFunc, Nil, Str, Sym, Vector};
use bellman::Variable;
use bls12_381::Scalar;

#[derive(Debug, Clone)]
pub struct Allocation {
    pub symbol: String,
    pub value: Scalar,
}

#[derive(Debug, Clone)]
pub struct EnforceAllocation {
    pub left: Vec<(String, String)>,
    pub right: Vec<(String, String)>,
    pub output: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct LispCircuit {
    pub params: FnvHashMap<String, MalVal>,
    pub allocs: FnvHashMap<String, MalVal>,
    pub alloc_inputs: FnvHashMap<String, MalVal>,
    pub constraints: Vec<EnforceAllocation>,
    pub env: Env,
}

impl Circuit<bls12_381::Scalar> for LispCircuit {
    fn synthesize<CS: ConstraintSystem<bls12_381::Scalar>>(
        self,
        cs: &mut CS,
    ) -> Result<(), SynthesisError> {
        let mut variables: FnvHashMap<String, Variable> = FnvHashMap::default();

        println!("Allocations\n");
        for (k, v) in &self.allocs {
            println!("k {:?} v {:?}", k, v);
            match v {
                MalVal::ZKScalar(val) => {
                    let var = cs.alloc(|| "alloc", || Ok(*val))?;
                    variables.insert(k.to_string(), var);
                }
                MalVal::Str(val) => {
                    let val_scalar = bls12_381::Scalar::from_string(&*val);
                    let var = cs.alloc(|| "alloc", || Ok(val_scalar))?;
                    variables.insert(k.to_string(), var);
                }
                _ => {
                    println!("not allocated k {:?} v {:?}", k, v);
                }
            }
        }

        println!("Allocations Input\n");
        for (k, v) in &self.alloc_inputs {
            println!("k {:?} v {:?}", k, v);
            match v {
                MalVal::ZKScalar(val) => {
                    let var = cs.alloc_input(|| "alloc", || Ok(*val))?;
                    variables.insert(k.to_string(), var);
                }
                MalVal::Str(val) => {
                    let val_scalar = bls12_381::Scalar::from_string(&*val);
                    let var = cs.alloc_input(|| "alloc", || Ok(val_scalar))?;
                    variables.insert(k.to_string(), var);
                }
                _ => {
                    println!("not allocated k {:?} v {:?}", k, v);
                }
            }
        }

        println!("Enforce Allocations\n");
        for alloc_value in &self.constraints {
            let coeff = bls12_381::Scalar::one();
            let mut left = bellman::LinearCombination::<Scalar>::zero();
            let mut right = bellman::LinearCombination::<Scalar>::zero();
            let mut output = bellman::LinearCombination::<Scalar>::zero();
            for values in alloc_value.left.iter() {
                let (a, b) = values;
                let mut val_b = CS::one();
                if b != "cs::one" {
                    val_b = *variables.get(b).unwrap();
                }
                if a == "scalar::one" {
                    left = left + (coeff, val_b);
                } else if a == "scalar::one::neg" {
                    left = left + (coeff.neg(), val_b);
                } else {
                    if let Some(value) = self.params.get(a) {
                        if let MalVal::ZKScalar(val) = value {
                            left = left + (*val, val_b);
                        }
                    }
                }
            }

            for values in alloc_value.right.iter() {
                let (a, b) = values;
                let mut val_b = CS::one();
                if b != "cs::one" {
                    val_b = *variables.get(b).unwrap();
                }
                if a == "scalar::one" {
                    right = right + (coeff, val_b);
                } else if a == "scalar::one::neg" {
                    right = right + (coeff.neg(), val_b);
                }
            }

            for values in alloc_value.output.iter() {
                let (a, b) = values;
                let mut val_b = CS::one();
                if b != "cs::one" {
                    val_b = *variables.get(b).unwrap();
                }
                if a == "scalar::one" {
                    output = output + (coeff, val_b);
                } else if a == "scalar::one::neg" {
                    output = output + (coeff.neg(), val_b);
                }
            }

            cs.enforce(
                || "constraint",
                |_| left.clone(),
                |_| right.clone(),
                |_| output.clone(),
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum MalVal {
    Nil,
    Bool(bool),
    Int(i64),
    Str(String),
    Sym(String),
    List(Rc<Vec<MalVal>>, Rc<MalVal>),
    Vector(Rc<Vec<MalVal>>, Rc<MalVal>),
    Hash(Rc<FnvHashMap<String, MalVal>>, Rc<MalVal>),
    Func(fn(MalArgs) -> MalRet, Rc<MalVal>),
    MalFunc {
        eval: fn(ast: MalVal, env: Env) -> MalRet,
        ast: Rc<MalVal>,
        env: Env,
        params: Rc<MalVal>,
        is_macro: bool,
        meta: Rc<MalVal>,
    },
    Atom(Rc<RefCell<MalVal>>),
    Zk(Rc<LispCircuit>), // TODO remote it
    Enforce(Rc<Vec<EnforceAllocation>>),
    ZKScalar(bls12_381::Scalar),
}

#[derive(Debug)]
pub enum MalErr {
    ErrString(String),
    ErrMalVal(MalVal),
}

pub type MalArgs = Vec<MalVal>;
pub type MalRet = Result<MalVal, MalErr>;

// type utility macros

macro_rules! list {
  ($seq:expr) => {{
    List(Rc::new($seq),Rc::new(Nil))
  }};
  [$($args:expr),*] => {{
    let v: Vec<MalVal> = vec![$($args),*];
    List(Rc::new(v),Rc::new(Nil))
  }}
}

macro_rules! vector {
  ($seq:expr) => {{
    Vector(Rc::new($seq),Rc::new(Nil))
  }};
  [$($args:expr),*] => {{
    let v: Vec<MalVal> = vec![$($args),*];
    Vector(Rc::new(v),Rc::new(Nil))
  }}
}

// type utility functions

pub fn error(s: &str) -> MalRet {
    Err(ErrString(s.to_string()))
}

pub fn format_error(e: MalErr) -> String {
    match e {
        ErrString(s) => s.clone(),
        ErrMalVal(mv) => mv.pr_str(true),
    }
}

pub fn atom(mv: &MalVal) -> MalVal {
    Atom(Rc::new(RefCell::new(mv.clone())))
}

impl MalVal {
    pub fn keyword(&self) -> MalRet {
        match self {
            Str(s) if s.starts_with("\u{29e}") => Ok(Str(s.to_string())),
            Str(s) => Ok(Str(format!("\u{29e}{}", s))),
            _ => error("invalid type for keyword"),
        }
    }

    pub fn empty_q(&self) -> MalRet {
        match self {
            List(l, _) | Vector(l, _) => Ok(Bool(l.len() == 0)),
            Nil => Ok(Bool(true)),
            _ => error("invalid type for empty?"),
        }
    }

    pub fn count(&self) -> MalRet {
        match self {
            List(l, _) | Vector(l, _) => Ok(Int(l.len() as i64)),
            Nil => Ok(Int(0)),
            _ => error("invalid type for count"),
        }
    }

    pub fn apply(&self, args: MalArgs) -> MalRet {
        match *self {
            Func(f, _) => f(args),
            MalFunc {
                eval,
                ref ast,
                ref env,
                ref params,
                ..
            } => {
                let a = &**ast;
                let p = &**params;
                let fn_env = env_bind(Some(env.clone()), p.clone(), args)?;
                Ok(eval(a.clone(), fn_env)?)
            }
            _ => error("attempt to call non-function"),
        }
    }

    pub fn keyword_q(&self) -> bool {
        match self {
            Str(s) if s.starts_with("\u{29e}") => true,
            _ => false,
        }
    }

    pub fn deref(&self) -> MalRet {
        match self {
            Atom(a) => Ok(a.borrow().clone()),
            _ => error("attempt to deref a non-Atom"),
        }
    }

    pub fn reset_bang(&self, new: &MalVal) -> MalRet {
        match self {
            Atom(a) => {
                *a.borrow_mut() = new.clone();
                Ok(new.clone())
            }
            _ => error("attempt to reset! a non-Atom"),
        }
    }

    pub fn swap_bang(&self, args: &MalArgs) -> MalRet {
        match self {
            Atom(a) => {
                let f = &args[0];
                let mut fargs = args[1..].to_vec();
                fargs.insert(0, a.borrow().clone());
                *a.borrow_mut() = f.apply(fargs)?;
                Ok(a.borrow().clone())
            }
            _ => error("attempt to swap! a non-Atom"),
        }
    }

    pub fn get_meta(&self) -> MalRet {
        match self {
            List(_, meta) | Vector(_, meta) | Hash(_, meta) => Ok((&**meta).clone()),
            Func(_, meta) => Ok((&**meta).clone()),
            MalFunc { meta, .. } => Ok((&**meta).clone()),
            _ => error("meta not supported by type"),
        }
    }

    pub fn with_meta(&mut self, new_meta: &MalVal) -> MalRet {
        match self {
            List(_, ref mut meta)
            | Vector(_, ref mut meta)
            | Hash(_, ref mut meta)
            | Func(_, ref mut meta)
            | MalFunc { ref mut meta, .. } => {
                *meta = Rc::new((&*new_meta).clone());
            }
            _ => return error("with-meta not supported by type"),
        };
        Ok(self.clone())
    }
}

impl PartialEq for MalVal {
    fn eq(&self, other: &MalVal) -> bool {
        match (self, other) {
            (Nil, Nil) => true,
            (Bool(ref a), Bool(ref b)) => a == b,
            (Int(ref a), Int(ref b)) => a == b,
            (Str(ref a), Str(ref b)) => a == b,
            (Sym(ref a), Sym(ref b)) => a == b,
            (List(ref a, _), List(ref b, _))
            | (Vector(ref a, _), Vector(ref b, _))
            | (List(ref a, _), Vector(ref b, _))
            | (Vector(ref a, _), List(ref b, _)) => a == b,
            (Hash(ref a, _), Hash(ref b, _)) => a == b,
            (MalFunc { .. }, MalFunc { .. }) => false,
            _ => false,
        }
    }
}

pub fn func(f: fn(MalArgs) -> MalRet) -> MalVal {
    Func(f, Rc::new(Nil))
}

pub fn _assoc(mut hm: FnvHashMap<String, MalVal>, kvs: MalArgs) -> MalRet {
    if kvs.len() % 2 != 0 {
        return error("odd number of elements");
    }
    for (k, v) in kvs.iter().tuples() {
        match k {
            Str(s) => {
                hm.insert(s.to_string(), v.clone());
            }
            _ => return error("key is not string"),
        }
    }
    Ok(Hash(Rc::new(hm), Rc::new(Nil)))
}

pub fn _dissoc(mut hm: FnvHashMap<String, MalVal>, ks: MalArgs) -> MalRet {
    for k in ks.iter() {
        match k {
            Str(ref s) => {
                hm.remove(s);
            }
            _ => return error("key is not string"),
        }
    }
    Ok(Hash(Rc::new(hm), Rc::new(Nil)))
}

pub fn hash_map(kvs: MalArgs) -> MalRet {
    let hm: FnvHashMap<String, MalVal> = FnvHashMap::default();
    _assoc(hm, kvs)
}
