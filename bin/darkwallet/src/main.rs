/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

#![feature(deadline_api)]
#![feature(str_split_whitespace_remainder)]
#![feature(duration_millis_float)]
#![feature(stmt_expr_attributes)]

// Use these to incrementally fix warnings with cargo fix
//#![allow(warnings, unused)]
//#![deny(unused_imports)]

use async_lock::Mutex as AsyncMutex;
use std::sync::{mpsc, Arc};

#[macro_use]
extern crate log;
#[allow(unused_imports)]
use log::LevelFilter;

mod app;
mod darkirc;
mod error;
mod expr;
mod gfx;
mod mesh;
mod net;
//mod plugin;
mod prop;
mod pubsub;
//mod py;
mod scene;
mod text;
mod ui;
mod util;

use crate::{darkirc::DarkIrcBackend, net::ZeroMQAdapter, scene::SceneGraph, text::TextShaper};

pub type ExecutorPtr = Arc<smol::Executor<'static>>;

fn panic_hook(panic_info: &std::panic::PanicInfo) {
    error!("panic occurred: {panic_info}");
    //error!("panic: {}", std::backtrace::Backtrace::force_capture().to_string());
    std::process::exit(1);
}

fn main() {
    // Exit the application on panic right away
    std::panic::set_hook(Box::new(panic_hook));

    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default().with_max_level(LevelFilter::Debug).with_tag("darkfi"),
        );

        let paths = std::fs::read_dir("/data/data/darkfi.darkwallet/").unwrap();
        for path in paths {
            debug!("{}", path.unwrap().path().display())
        }
    }

    #[cfg(target_os = "linux")]
    {
        // For ANSI colors in the terminal
        colored::control::set_override(true);

        let term_logger = simplelog::TermLogger::new(
            simplelog::LevelFilter::Debug,
            simplelog::Config::default(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        );
        simplelog::CombinedLogger::init(vec![term_logger]).expect("logger");
    }

    let ex = Arc::new(smol::Executor::new());
    let sg = Arc::new(AsyncMutex::new(SceneGraph::new()));

    let async_runtime = app::AsyncRuntime::new(ex.clone());
    async_runtime.start();

    let sg2 = sg.clone();
    let ex2 = ex.clone();
    let zmq_task = ex.spawn(async {
        let zmq_rpc = ZeroMQAdapter::new(sg2, ex2).await;
        zmq_rpc.run().await;
    });
    async_runtime.push_task(zmq_task);

    let (method_req, method_rep) = mpsc::channel();
    // The UI actually needs to be running for this to reply back.
    // Otherwise calls will just hang.
    let render_api = gfx::RenderApi::new(method_req);
    let event_pub = gfx::GraphicsEventPublisher::new();

    let text_shaper = TextShaper::new();

    let darkirc_backend = DarkIrcBackend::new();
    let app = app::App::new(
        sg.clone(),
        ex.clone(),
        render_api.clone(),
        event_pub.clone(),
        text_shaper,
        darkirc_backend,
    );
    let app_task = ex.spawn(app.clone().start());
    async_runtime.push_task(app_task);

    // Nice to see which events exist
    let ev_sub = event_pub.subscribe_key_down();
    let ev_relay_task = ex.spawn(async move {
        debug!(target: "main", "event relayer started");
        loop {
            let Ok((key, mods, repeat)) = ev_sub.receive().await else {
                debug!(target: "main", "Event relayer closed");
                break
            };
            // Ignore keys which get stuck repeating when switching windows
            match key {
                miniquad::KeyCode::LeftShift | miniquad::KeyCode::LeftSuper => continue,
                _ => {}
            }
            if !repeat {
                debug!(target: "main", "key_down event: {:?} {:?} {}", key, mods, repeat);
            }
        }
    });
    async_runtime.push_task(ev_relay_task);
    let ev_sub = event_pub.subscribe_key_up();
    let ev_relay_task = ex.spawn(async move {
        debug!(target: "main", "event relayer started");
        loop {
            let Ok((key, mods)) = ev_sub.receive().await else {
                debug!(target: "main", "Event relayer closed");
                break
            };
            // Ignore keys which get stuck repeating when switching windows
            match key {
                miniquad::KeyCode::LeftShift | miniquad::KeyCode::LeftSuper => continue,
                _ => {}
            }
            debug!(target: "main", "key_up event: {:?} {:?}", key, mods);
        }
    });
    async_runtime.push_task(ev_relay_task);
    let ev_sub = event_pub.subscribe_char();
    let ev_relay_task = ex.spawn(async move {
        debug!(target: "main", "event relayer started");
        loop {
            let Ok((key, mods, repeat)) = ev_sub.receive().await else {
                debug!(target: "main", "Event relayer closed");
                break
            };
            debug!(target: "main", "char event: {:?} {:?} {}", key, mods, repeat);
        }
    });
    async_runtime.push_task(ev_relay_task);

    //let stage = gfx::Stage::new(method_rep, event_pub);
    gfx::run_gui(app, async_runtime, method_rep, event_pub);
    debug!(target: "main", "Started GFX backend");
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
