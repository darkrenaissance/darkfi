#![allow(non_snake_case)]

use crate::types::LispCircuit;
use bellman::groth16::PreparedVerifyingKey;

use sapvi::{BlsStringConversion, Decodable, Encodable, ZKContract, ZKProof};
use simplelog::*;

use bellman::groth16;
use bls12_381::Bls12;
use fnv::FnvHashMap;
use itertools::Itertools;
use rand::rngs::OsRng;
use std::fs::File;
use std::rc::Rc;
use std::time::Instant;
use std::{
    borrow::{Borrow, BorrowMut},
    fs,
};
use types::EnforceAllocation;

#[macro_use]
extern crate clap;
#[macro_use]
extern crate lazy_static;
extern crate fnv;
extern crate itertools;
extern crate regex;

#[macro_use]
mod types;
use crate::types::MalErr::{ErrMalVal, ErrString};
use crate::types::MalVal::{Bool, Enforce, Func, Hash, List, MalFunc, Nil, Str, Sym, Vector};
use crate::types::VerifyKeyParams;
use crate::types::{error, format_error, MalArgs, MalErr, MalRet, MalVal};
mod env;
mod printer;
mod reader;
use crate::env::{env_bind, env_find, env_get, env_new, env_set, env_sets, Env};
#[macro_use]
mod core;

// read
fn read(str: &str) -> MalRet {
    reader::read_str(str.to_string())
}

// eval

fn qq_iter(elts: &MalArgs) -> MalVal {
    let mut acc = list![];
    for elt in elts.iter().rev() {
        if let List(v, _) = elt {
            if v.len() == 2 {
                if let Sym(ref s) = v[0] {
                    if s == "splice-unquote" {
                        acc = list![Sym("concat".to_string()), v[1].clone(), acc];
                        continue;
                    }
                }
            }
        }
        acc = list![Sym("cons".to_string()), quasiquote(&elt), acc];
    }
    return acc;
}

fn quasiquote(ast: &MalVal) -> MalVal {
    match ast {
        List(v, _) => {
            if v.len() == 2 {
                if let Sym(ref s) = v[0] {
                    if s == "unquote" {
                        return v[1].clone();
                    }
                }
            }
            return qq_iter(&v);
        }
        Vector(v, _) => return list![Sym("vec".to_string()), qq_iter(&v)],
        Hash(_, _) | Sym(_) => return list![Sym("quote".to_string()), ast.clone()],
        _ => ast.clone(),
    }
}

fn is_macro_call(ast: &MalVal, env: &Env) -> Option<(MalVal, MalArgs)> {
    match ast {
        List(v, _) => match v[0] {
            Sym(ref s) => match env_find(env, s) {
                Some(e) => match env_get(&e, &v[0]) {
                    Ok(f @ MalFunc { is_macro: true, .. }) => Some((f, v[1..].to_vec())),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

fn macroexpand(mut ast: MalVal, env: &Env) -> (bool, MalRet) {
    let mut was_expanded = false;
    while let Some((mf, args)) = is_macro_call(&ast, env) {
        // println!("macroexpand 1: {:?}", ast);
        ast = match mf.apply(args) {
            Err(e) => return (false, Err(e)),
            Ok(a) => a,
        };
        // println!("macroexpand 2: {:?}", ast); 
        was_expanded = true;
    }
    (was_expanded, Ok(ast))
}

fn eval_ast(ast: &MalVal, env: &Env) -> MalRet {
    match ast {
        Sym(_) => Ok(env_get(&env, &ast)?),
        List(v, _) => {
            let mut lst: MalArgs = vec![];
            for a in v.iter() {
                lst.push(eval(a.clone(), env.clone())?)
            }
            Ok(list!(lst))
        }
        Vector(v, _) => {
            let mut lst: MalArgs = vec![];
            for a in v.iter() {
                lst.push(eval(a.clone(), env.clone())?)
            }
            Ok(vector!(lst))
        }
        Hash(hm, _) => {
            let mut new_hm: FnvHashMap<String, MalVal> = FnvHashMap::default();
            for (k, v) in hm.iter() {
                new_hm.insert(k.to_string(), eval(v.clone(), env.clone())?);
            }
            Ok(Hash(Rc::new(new_hm), Rc::new(Nil)))
        }
        _ => Ok(ast.clone()),
    }
}

fn eval(mut ast: MalVal, mut env: Env) -> MalRet {
    let ret: MalRet;

    'tco: loop {
        ret = match ast.clone() {
            List(l, _) => {
                if l.len() == 0 {
                    return Ok(ast);
                }
                match macroexpand(ast.clone(), &env) {
                    (true, Ok(new_ast)) => {
                        ast = new_ast;
                        continue 'tco;
                    }
                    (_, Err(e)) => return Err(e),
                    _ => (),
                }

                if l.len() == 0 {
                    return Ok(ast);
                }
                let a0 = &l[0];
                match a0 {
                    Sym(ref a0sym) if a0sym == "def!" => {
                        env_set(&env, l[1].clone(), eval(l[2].clone(), env.clone())?)
                    }
                    Sym(ref a0sym) if a0sym == "zk*" => {
                        // println!("zk* {:?}", l[1]);
                        let (a1, a2) = (l[1].clone(), l[2].clone());
                        match a1 {
                            List(ref binds, _) | Vector(ref binds, _) => {
                                for (b, e) in binds.iter().tuples() {
                                    match b {
                                        Sym(_) => {
                                            let _ = env_set(
                                                &env,
                                                b.clone(),
                                                eval(e.clone(), env.clone())?,
                                            );
                                        }
                                        _ => {
                                            return error("let* with non-Sym binding");
                                        }
                                    }
                                }
                            }
                            _ => {
                                return error("let* with non-List bindings");
                            }
                        };
                        ast = a2;
                        continue 'tco;
                    }
                    Sym(ref a0sym) if a0sym == "let*" => {
                        env = env_new(Some(env.clone()));
                        let (a1, a2) = (l[1].clone(), l[2].clone());
                        match a1 {
                            List(ref binds, _) | Vector(ref binds, _) => {
                                for (b, e) in binds.iter().tuples() {
                                    match b {
                                        Sym(_) => {
                                            let _ = env_set(
                                                &env,
                                                b.clone(),
                                                eval(e.clone(), env.clone())?,
                                            );
                                        }
                                        _ => {
                                            return error("let* with non-Sym binding");
                                        }
                                    }
                                }
                            }
                            _ => {
                                return error("let* with non-List bindings");
                            }
                        };
                        ast = a2;
                        continue 'tco;
                    }
                    Sym(ref a0sym) if a0sym == "quote" => Ok(l[1].clone()),
                    Sym(ref a0sym) if a0sym == "quasiquoteexpand" => Ok(quasiquote(&l[1])),
                    Sym(ref a0sym) if a0sym == "quasiquote" => {
                        ast = quasiquote(&l[1]);
                        continue 'tco;
                    }
                    Sym(ref a0sym) if a0sym == "defmacro!" => {
                        let (a1, a2) = (l[1].clone(), l[2].clone());
                        let r = eval(a2, env.clone())?;
                        match r {
                            MalFunc {
                                eval,
                                ast,
                                env,
                                params,
                                ..
                            } => Ok(env_set(
                                &env,
                                a1.clone(),
                                MalFunc {
                                    eval: eval,
                                    ast: ast.clone(),
                                    env: env.clone(),
                                    params: params.clone(),
                                    is_macro: true,
                                    meta: Rc::new(Nil),
                                },
                            )?),
                            _ => error("set_macro on non-function"),
                        }
                    }
                    Sym(ref a0sym) if a0sym == "macroexpand" => {
                        match macroexpand(l[1].clone(), &env) {
                            (_, Ok(new_ast)) => Ok(new_ast),
                            (_, e) => return e,
                        }
                    }
                    Sym(ref a0sym) if a0sym == "try*" => match eval(l[1].clone(), env.clone()) {
                        Err(ref e) if l.len() >= 3 => {
                            let exc = match e {
                                ErrMalVal(mv) => mv.clone(),
                                ErrString(s) => Str(s.to_string()),
                            };
                            match l[2].clone() {
                                List(c, _) => {
                                    let catch_env = env_bind(
                                        Some(env.clone()),
                                        list!(vec![c[1].clone()]),
                                        vec![exc],
                                    )?;
                                    eval(c[2].clone(), catch_env)
                                }
                                _ => error("invalid catch block"),
                            }
                        }
                        res => res,
                    },
                    Sym(ref a0sym) if a0sym == "do" => {
                        match eval_ast(&list!(l[1..l.len() - 1].to_vec()), &env)? {
                            List(_, _) => {
                                ast = l.last().unwrap_or(&Nil).clone();
                                continue 'tco;
                            }
                            _ => error("invalid do form"),
                        }
                    }
                    Sym(ref a0sym) if a0sym == "if" => {
                        let cond = eval(l[1].clone(), env.clone())?;
                        match cond {
                            Bool(false) | Nil if l.len() >= 4 => {
                                ast = l[3].clone();
                                continue 'tco;
                            }
                            Bool(false) | Nil => Ok(Nil),
                            _ if l.len() >= 3 => {
                                ast = l[2].clone();
                                continue 'tco;
                            }
                            _ => Ok(Nil),
                        }
                    }

                    Sym(ref a0sym) if a0sym == "fn*" => {
                        let (a1, a2) = (l[1].clone(), l[2].clone());
                        Ok(MalFunc {
                            eval: eval,
                            ast: Rc::new(a2),
                            env: env,
                            params: Rc::new(a1),
                            is_macro: false,
                            meta: Rc::new(Nil),
                        })
                    }
                    Sym(ref a0sym) if a0sym == "eval" => {
                        ast = eval(l[1].clone(), env.clone())?;
                        while let Some(ref e) = env.clone().outer {
                            env = e.clone();
                        }
                        continue 'tco;
                    }
                    Sym(ref a0sym) if a0sym == "setup" => {
                        let a1 = l[1].clone();
                        // todo
                        ast = eval(a1.clone(), env.clone())?;
                        //                        let _pvk = setup(a1.clone(), env.clone())?;
                        continue 'tco;
                    }
                    Sym(ref a0sym) if a0sym == "prove" => {
                        let a1 = l[1].clone();
                        eval(a1.clone(), env.clone())?;
                        prove(a1.clone(), env.clone())
                    }
                    Sym(ref a0sym) if a0sym == "alloc-const" => {
                        let a1 = l[1].clone();
                        let value = eval(l[2].clone(), env.clone())?;
                        let result = eval(value.clone(), env.clone())?;
                        let allocs = get_allocations(&env, "AllocationsConst");
                        let mut new_hm: FnvHashMap<String, MalVal> = FnvHashMap::default();
                        for (k, v) in allocs.iter() {
                            new_hm.insert(k.to_string(), eval(v.clone(), env.clone())?);
                        }
                        new_hm.insert(a1.pr_str(false), result.clone());      
                        if let Some(e) = &env.outer {
                            env_set(
                                &e,
                                Sym("AllocationsConst".to_string()),
                                Hash(Rc::new(new_hm), Rc::new(Nil)),
                            )?;
                        } else {
                            env_set(
                                &env,
                                Sym("AllocationsConst".to_string()),
                                Hash(Rc::new(new_hm), Rc::new(Nil)),
                            )?;
                        }
                        Ok(result.clone())
                    }
                    Sym(ref a0sym) if a0sym == "alloc-input" => {
                        let a1 = l[1].clone();
                        let value = eval(l[2].clone(), env.clone())?;
                        let result = eval(value.clone(), env.clone())?;
                        let allocs = get_allocations(&env, "AllocationsInput");
                        let mut new_hm: FnvHashMap<String, MalVal> = FnvHashMap::default();
                        for (k, v) in allocs.iter() {
                            new_hm.insert(k.to_string(), eval(v.clone(), env.clone())?);
                        }
                        new_hm.insert(a1.pr_str(false), result.clone());
                        if let Some(e) = &env.outer {
                            env_set(
                                &e,
                                Sym("AllocationsInput".to_string()),
                                Hash(Rc::new(new_hm), Rc::new(Nil)),
                            )?;
                        } else {
                            env_set(
                                &env,
                                Sym("AllocationsInput".to_string()),
                                Hash(Rc::new(new_hm), Rc::new(Nil)),
                            )?;
                        }
                        Ok(result.clone())
                    }
                    Sym(ref a0sym) if a0sym == "alloc" => {
                        let a1 = l[1].clone();
                        let value = eval(l[2].clone(), env.clone())?;
                        let result = eval(value.clone(), env.clone())?;
                        // println!("a1 {:?} ", a1);
                        let allocs = get_allocations(&env, "Allocations");
                        let mut new_hm: FnvHashMap<String, MalVal> = FnvHashMap::default();
                        for (k, v) in allocs.iter() {
                            new_hm.insert(k.to_string(), eval(v.clone(), env.clone())?);
                        }
                        new_hm.insert(a1.pr_str(false), result.clone());
                        if let Some(e) = &env.outer {
                            env_set(
                                &e,
                                Sym("Allocations".to_string()),
                                Hash(Rc::new(new_hm), Rc::new(Nil)),
                            )?;
                        } else {
                            env_set(
                                &env,
                                Sym("Allocations".to_string()),
                                Hash(Rc::new(new_hm), Rc::new(Nil)),
                            )?;
                        }
                        Ok(result.clone())
                    }
                    //Sym(ref a0sym) if a0sym == "verify" => {
                    Sym(ref a0sym) if a0sym == "enforce" => {
                        // here i'm considering that we always have tuple with only two elements
                        // also it's important to keep in mind for the sake of brevity of this v0
                        // we will not allow calculation or any lisp evaluations inside the enforce
                        // it means that every symbol will be on allocations and we will do the
                        // find/replace on the bellman circuit, it's nasty v0
                        let mut left_vec = vec![];
                        let mut right_vec = vec![];
                        let mut out_vec = vec![];
                        // todo extract a macro for this
                        match l[1].clone() {
                            List(v, _) | Vector(v, _) => {
                                if let List(_, _) = &v.to_vec()[0] {
                                    for ele in v.to_vec().iter() {
                                        if let List(ele_vec, _) = ele {
                                            left_vec.push((
                                                ele_vec[0].pr_str(false),
                                                ele_vec[1].pr_str(false),
                                            ));
                                        }
                                    }
                                } else {
                                    left_vec.push((v[0].pr_str(false), v[1].pr_str(false)));
                                }
                            }
                            _ => {}
                        };
                        match l[2].clone() {
                            List(v, _) | Vector(v, _) => {
                                if let List(_, _) = &v.to_vec()[0] {
                                    for ele in v.to_vec().iter() {
                                        if let List(ele_vec, _) = ele {
                                            right_vec.push((
                                                ele_vec[0].pr_str(false),
                                                ele_vec[1].pr_str(false),
                                            ));
                                        }
                                    }
                                } else {
                                    right_vec.push((v[0].pr_str(false), v[1].pr_str(false)));
                                }
                            }
                            _ => {}
                        };
                        match l[3].clone() {
                            List(v, _) | Vector(v, _) => {
                                if let List(_, _) = &v.to_vec()[0] {
                                    for ele in v.to_vec().iter() {
                                        if let List(ele_vec, _) = ele {
                                            out_vec.push((
                                                ele_vec[0].pr_str(false),
                                                ele_vec[1].pr_str(false),
                                            ));
                                        }
                                    }
                                } else {
                                    out_vec.push((v[0].pr_str(false), v[1].pr_str(false)));
                                }
                            }
                            _ => {}
                        };
                        let enforce_vec = get_enforce_allocs(&env);
                        let enforce = EnforceAllocation {
                            idx: enforce_vec.len() + 1,
                            left: left_vec,
                            right: right_vec,
                            output: out_vec,
                        };
                        let mut new_vec: Vec<EnforceAllocation> = vec![enforce];
                        for value in enforce_vec.iter() {
                            new_vec.push(value.clone());
                        }
                        // TODO change it
                        if let Some(e) = &env.outer {
                            env_set(
                                &e,
                                Sym("AllocationsEnforce".to_string()),
                                vector![vec![Enforce(Rc::new(new_vec.clone()))]],
                            )?;
                        } else {
                            env_set(
                                &env,
                                Sym("AllocationsEnforce".to_string()),
                                vector![vec![Enforce(Rc::new(new_vec.clone()))]],
                            )?;
                        }

                        // println!(
                        //     "allocs here {:?}",
                        //     get_allocations_nested(&env, "Allocations")
                        // );
                        // println!("enforce here {:?}", get_enforce_allocs_nested(&env));

                        Ok(MalVal::Nil)
                    }
                    _ => match eval_ast(&ast, &env)? {
                        List(ref el, _) => {
                            let ref f = el[0].clone();
                            let args = el[1..].to_vec();
                            match f {
                                Func(_, _) => f.apply(args),
                                MalFunc {
                                    ast: mast,
                                    env: menv,
                                    params,
                                    ..
                                } => {
                                    let a = &**mast;
                                    let p = &**params;
                                    env = env_bind(Some(menv.clone()), p.clone(), args)?;
                                    ast = a.clone();
                                    continue 'tco;
                                }
                                _ => {
                                    // println!("{:?}", args);
                                    Ok(vector![el.to_vec()])
                                    //error("call non-function")
                                }
                            }
                        }
                        _ => error("expected a list"),
                    },
                }
            }
            _ => eval_ast(&ast, &env),
        };

        break;
    } // end 'tco loop

    ret
}

pub fn get_enforce_allocs(env: &Env) -> Vec<EnforceAllocation> {
    if let Some(e) = &env.outer {
        get_enforce_allocs_nested(&e)
    } else {
        get_enforce_allocs_nested(&env)
    }
}

pub fn get_enforce_allocs_nested(env: &Env) -> Vec<EnforceAllocation> {
    match env_find(env, "AllocationsEnforce") {
        Some(e) => match env_get(&e, &Sym("AllocationsEnforce".to_string())) {
            Ok(f) => {
                if let Vector(val, _) = f {
                    if let Enforce(ret) = &val[0] {
                        ret.to_vec()
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
            _ => vec![],
        },
        _ => vec![],
    }
}

pub fn get_allocations(env: &Env, key: &str) -> Rc<FnvHashMap<String, MalVal>> {
    if let Some(e) = &env.outer {
        get_allocations_nested(&e, key)
    } else {
        get_allocations_nested(&env, key)
    }
}

pub fn get_allocations_nested(env: &Env, key: &str) -> Rc<FnvHashMap<String, MalVal>> {
    let alloc_hm: Rc<FnvHashMap<String, MalVal>> = Rc::new(FnvHashMap::default());
    match env_find(env, key) {
        Some(e) => match env_get(&e, &Sym(key.to_string())) {
            Ok(f) => {
                if let Hash(allocs, _) = f {
                    allocs
                } else {
                    alloc_hm
                }
            }
            _ => alloc_hm,
        },
        _ => alloc_hm,
    }
}

pub fn setup(_ast: MalVal, env: Env) -> Result<VerifyKeyParams, MalErr> {
    let start = Instant::now();
    let c = LispCircuit {
        params: FnvHashMap::default(),
        allocs: FnvHashMap::default(),
        alloc_inputs: FnvHashMap::default(),
        constraints: Vec::new(),
    };
    let random_parameters =
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap();
    let pvk = groth16::prepare_verifying_key(&random_parameters.vk);
    println!("Setup: [{:?}]", start.elapsed());

    Ok(VerifyKeyParams {
        verifying_key: pvk,
        random_params: random_parameters,
    })
}

pub fn prove(_ast: MalVal, env: Env) -> MalRet {
    // let start = Instant::now();
    let allocs_input = get_allocations(&env, "AllocationsInput");
    let allocs = get_allocations(&env, "Allocations");
    let enforce_allocs = get_enforce_allocs(&env);
    let allocs_const = get_allocations(&env, "AllocationsConst");
    //setup
    let params = Some({
        let circuit = LispCircuit {
            params: allocs_const.as_ref().clone(),
            allocs: allocs.as_ref().clone(),
            alloc_inputs: allocs_input.as_ref().clone(),
            constraints: enforce_allocs.clone(),
        };
        groth16::generate_random_parameters::<Bls12, _, _>(circuit, &mut OsRng)?
    });
    let verifying_key = Some(groth16::prepare_verifying_key(&params.as_ref().unwrap().vk));
    // prove
    let circuit = LispCircuit {
        params: allocs_const.as_ref().clone(),
        allocs: allocs.as_ref().clone(),
        alloc_inputs: allocs_input.as_ref().clone(),
        constraints: enforce_allocs.clone(),
    };

    let proof = groth16::create_random_proof(circuit, params.as_ref().unwrap(), &mut OsRng)?;
    let mut vec_input = vec![];
    for (k, val) in allocs_input.iter() {
        match val {
            MalVal::Str(v) => {
                vec_input.push(bls12_381::Scalar::from_string(&v.to_string()));
            }
            MalVal::ZKScalar(v) => {
                vec_input.push(bls12_381::Scalar::from(*v));
            }
            _ => {}
        };
    }
    let result = groth16::verify_proof(verifying_key.as_ref().unwrap(), &proof, &vec_input);
    println!("vec public {:?}", vec_input);
    println!("result {:?}", result);

    Ok(MalVal::Nil)
}

pub fn verify(_ast: &MalVal) -> MalRet {
    let _public_input = vec![bls12_381::Scalar::from(27)];
    let start = Instant::now();
    println!("Verify: [{:?}]", start.elapsed());
    Ok(MalVal::Nil)
}

// print
fn print(ast: &MalVal) -> String {
    ast.pr_str(true)
}

fn rep(str: &str, env: &Env) -> Result<String, MalErr> {
    let ast = read(str)?;
    let exp = eval(ast, env.clone())?;
    Ok(print(&exp))
}

fn main() -> Result<(), ()> {
    let matches = clap_app!(zklisp =>
        (version: "0.1.0")
        (author: "Dark Renaissance")
        (about: "A Lisp Interpreter for Zero Knowledge Virtual Machine")
        (@subcommand load =>
            (about: "Load the file into the interpreter")
            (@arg FILE: +required "Lisp Contract filename")
        )
    )
    .get_matches();

    // CombinedLogger::init(vec![TermLogger::new(
    //     LevelFilter::Debug,
    //     Config::default(),
    //     TerminalMode::Mixed,
    // )
    // .unwrap()])
    // .unwrap();

    match matches.subcommand() {
        Some(("load", matches)) => {
            let file: String = matches.value_of("FILE").unwrap().parse().unwrap();
            repl_load(file)?;
        }
        _ => {
            eprintln!("error: Invalid subcommand invoked");
            std::process::exit(-1);
        }
    }

    Ok(())
}

fn repl_load(file: String) -> Result<(), ()> {
    let repl_env = env_new(None);
    for (k, v) in core::ns() {
        env_sets(&repl_env, k, v);
    }
    let _ = rep("(def! not (fn* (a) (if a false true)))", &repl_env);
    let _ = rep(
        "(def! load-file (fn* (f) (eval (read-string (str \"(do \" (slurp f) \"\nnil)\")))))",
        &repl_env,
    );
    match rep(&format!("(load-file \"{}\")", file), &repl_env) {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            println!("Error: {}", format_error(e));
            std::process::exit(1);
        }
    }
}
