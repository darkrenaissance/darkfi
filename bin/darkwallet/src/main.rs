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

// channel wait until deadline
#![feature(deadline_api)]
// Adds remainder() fn for String::split() result
#![feature(str_split_whitespace_remainder)]
// instant.elapsed().as_millis_f32()
#![feature(duration_millis_float)]
// Allow attributes on statements and code blocks
#![feature(stmt_expr_attributes)]
// if let Some(is_foo) = is_foo && is_foo { ... }
#![feature(let_chains)]
// consume a box
#![feature(box_into_inner)]
// we need Arc::get_mut_unchecked() to workaround the lack of Arc::new_cyclic() which
// accepts async fns.
// See https://github.com/rust-lang/rust/issues/112566
#![feature(get_mut_unchecked)]
// string.chars().advance_back_by(n), not strictly needed but makes life easier
#![feature(iter_advance_by)]

// Use these to incrementally fix warnings with cargo fix
//#![allow(warnings, unused)]
//#![deny(unused_imports)]

use async_lock::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use darkfi::system::CondVar;
use file_rotate::{compression::Compression, suffix::AppendCount, ContentLimit, FileRotate};
use std::sync::{mpsc, Arc};

#[macro_use]
extern crate log;
#[allow(unused_imports)]
use log::LevelFilter;

#[cfg(target_os = "android")]
mod android;
mod app;
mod build_info;
mod error;
mod expr;
mod gfx;
mod logger;
mod mesh;
mod net;
mod plugin;
mod prop;
mod pubsub;
//mod py;
mod ringbuf;
mod scene;
mod shape;
use scene::SceneNode as SceneNode3;
mod text;
mod ui;
mod util;

use crate::{net::ZeroMQAdapter, text::TextShaper};

pub type ExecutorPtr = Arc<smol::Executor<'static>>;

fn panic_hook(panic_info: &std::panic::PanicHookInfo) {
    error!("panic occurred: {panic_info}");
    error!("{}", std::backtrace::Backtrace::force_capture().to_string());
    std::process::abort()
}

fn main() {
    // Abort the application on panic right away
    std::panic::set_hook(Box::new(panic_hook));

    logger::setup_logging();

    #[cfg(target_os = "android")]
    {
        // Workaround for this bug
        // https://gitlab.torproject.org/tpo/core/arti/-/issues/999
        unsafe {
            std::env::set_var("HOME", "/data/data/darkfi.darkwallet/");
        }

        let paths = std::fs::read_dir("/data/data/darkfi.darkwallet/").unwrap();
        for path in paths {
            debug!("{}", path.unwrap().path().display())
        }
    }

    let exe_path = std::env::current_exe().unwrap();
    let basename = exe_path.parent().unwrap();
    std::env::set_current_dir(basename);

    info!("Target OS: {}", build_info::TARGET_OS);
    info!("Target arch: {}", build_info::TARGET_ARCH);
    let cwd = std::env::current_dir().unwrap();
    info!("Current dir: {}", cwd.display());

    let ex = Arc::new(smol::Executor::new());
    let sg_root = SceneNode3::root();

    let async_runtime = app::AsyncRuntime::new(ex.clone());
    async_runtime.start();

    if cfg!(features = "enable-netdebug") {
        let sg_root2 = sg_root.clone();
        let ex2 = ex.clone();
        let zmq_task = ex.spawn(async {
            let zmq_rpc = ZeroMQAdapter::new(sg_root2, ex2).await;
            zmq_rpc.run().await;
        });
        async_runtime.push_task(zmq_task);
    }

    let (method_req, method_rep) = mpsc::channel();
    // The UI actually needs to be running for this to reply back.
    // Otherwise calls will just hang.
    let render_api = gfx::RenderApi::new(method_req);
    let event_pub = gfx::GraphicsEventPublisher::new();

    let text_shaper = TextShaper::new();

    let cv_gfxwin_started = Arc::new(CondVar::new());
    let cv_gfxwin_started2 = cv_gfxwin_started.clone();
    let cv_app_started = Arc::new(CondVar::new());
    let cv_app_started2 = cv_app_started.clone();
    let app = app::App::new(sg_root, render_api, event_pub.clone(), text_shaper, ex.clone());
    let app2 = app.clone();
    let app_task = ex.spawn(async move {
        app2.setup().await;
        // Needed because accessing screen_size() is not allowed until window init
        cv_gfxwin_started2.wait().await;
        app2.start().await;
        cv_app_started2.notify();
    });
    async_runtime.push_task(app_task);

    /*
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
    */

    //let stage = gfx::Stage::new(method_rep, event_pub);
    gfx::run_gui(app, async_runtime, method_rep, event_pub, cv_gfxwin_started);
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
