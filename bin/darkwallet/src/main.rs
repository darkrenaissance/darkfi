#![feature(deadline_api)]
#![feature(str_split_whitespace_remainder)]

use std::{
    sync::{Arc, Mutex},
    thread,
};

mod chatview;

mod editbox;

mod error;

mod expr;

mod gfx;
use gfx::run_gui;

mod keysym;

mod net;
use net::ZeroMQAdapter;

mod scene;
use scene::{SceneGraph, SceneGraphPtr};

mod plugin;

mod prop;

mod py;

mod res;

mod shader;

mod text;

#[macro_use]
extern crate log;
#[allow(unused_imports)]
use log::LevelFilter;

fn start_zmq(scene_graph: SceneGraphPtr) {
    // detach thread
    let _ = thread::spawn(move || {
        let mut zmq_rpc = ZeroMQAdapter::new(scene_graph);
        zmq_rpc.run();
    });
}

fn start_sentinel(scene_graph: SceneGraphPtr) {
    // detach thread
    // Sentinel should cleanly close when sent a stop signal.
    let _ = thread::spawn(move || {
        let mut sentinel = plugin::Sentinel::new(scene_graph);
        sentinel.run();
    });
}

fn main() {
    let scene_graph = Arc::new(Mutex::new(SceneGraph::new()));
    start_zmq(scene_graph.clone());
    start_sentinel(scene_graph.clone());
    run_gui(scene_graph);
}

/*
use rustpython_vm::{self as pyvm, convert::ToPyObject};

fn main() {
    let module = pyvm::Interpreter::without_stdlib(Default::default()).enter(|vm| {
        let source = r#"
def foo():
    open("hihi", "w")
    return 110
#max(1 + lw/3, 4*10) + foo(2, True)
"#;
        //let code_obj = vm
        //    .compile(source, pyvm::compiler::Mode::Exec, "<embedded>".to_owned())
        //    .map_err(|err| vm.new_syntax_error(&err, Some(source))).unwrap();
        //code_obj
        pyvm::import::import_source(vm, "lain", source).unwrap()
    });

    fn foo(x: u32, y: bool) -> u32 {
        if y {
            2 * x
        } else {
            x
        }
    }

    let res = pyvm::Interpreter::without_stdlib(Default::default()).enter(|vm| {
        let globals = vm.ctx.new_dict();
        globals.set_item("lw", vm.ctx.new_int(110).to_pyobject(vm), vm).unwrap();
        globals.set_item("lh", vm.ctx.new_int(4).to_pyobject(vm), vm).unwrap();
        globals.set_item("foo", vm.new_function("foo", foo).into(), vm).unwrap();

        let scope = pyvm::scope::Scope::new(None, globals);

        let foo_fn = module.get_attr("foo", vm).unwrap();
        foo_fn.call((), vm).unwrap()

        //vm.run_code_obj(code_obj, scope).unwrap()
    });
    println!("{:?}", res);
}
*/
