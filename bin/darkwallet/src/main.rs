#![feature(deadline_api)]
#![feature(str_split_whitespace_remainder)]

use async_lock::Mutex;
use futures::{stream::FuturesUnordered, StreamExt};
use std::{
    sync::{mpsc, Arc},
    thread,
};

#[macro_use]
extern crate log;
#[allow(unused_imports)]
use log::LevelFilter;

mod app;
mod chatapp;
mod chatview;
mod editbox;
mod error;
mod expr;
mod gfx;
mod gfx2;
mod keysym;
mod net;
mod plugin;
mod prop;
mod pubsub;
mod py;
mod res;
mod scene;
mod shader;
mod text;

use crate::{
    error::{Error, Result},
    net::ZeroMQAdapter,
    scene::{SceneGraph, SceneGraphPtr},
};

fn start_zmq(scene_graph: SceneGraphPtr) {
    // detach thread
}

fn start_sentinel(scene_graph: SceneGraphPtr) {
    // detach thread
    // Sentinel should cleanly close when sent a stop signal.
    let _ = thread::spawn(move || {
        let mut sentinel = plugin::Sentinel::new(scene_graph);
        sentinel.run();
    });
}

/*
async fn greensq(render_api: Arc<gfx2::RenderApi>) -> (miniquad::BufferId, miniquad::BufferId) {
    let x1 = 0.1;
    let x2 = 0.6;
    let y1 = 0.1;
    let y2 = 0.6;
    let color = [1., 0., 0., 1.];

    let verts = vec![
        gfx2::Vertex { pos: [x1, y1], color, uv: [0., 0.] },
        gfx2::Vertex { pos: [x2, y1], color, uv: [1., 0.] },
        gfx2::Vertex { pos: [x1, y2], color, uv: [0., 1.] },
        gfx2::Vertex { pos: [x2, y2], color, uv: [1., 1.] },
    ];
    let vertex_buffer = render_api.new_vertex_buffer(verts).await.unwrap();

    let indices = vec![0, 2, 1, 1, 2, 3];
    let index_buffer = render_api.new_index_buffer(indices).await.unwrap();

    let (off_x, off_y) = (0., 0.);
    let (screen_width, screen_height) = miniquad::window::screen_size();
    let (scale_x, scale_y) = (1./screen_width, 1./screen_height);
    let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
        glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));
    let model = glam::Mat4::IDENTITY;

    // We have to handle window resizing for viewport and matrix

    let dc = gfx2::DrawCall {
        instrs: vec![
            //gfx2::DrawInstruction::ApplyViewport(gfx::Rectangle {
            //    x: 0, y: 0,
            //    w: screen_width as i32,
            //    h: screen_height as i32,
            //}),
        ],
        dcs: vec![
            gfx2::DrawCall {
                instrs: vec![
                    gfx2::DrawInstruction::ApplyMatrix(model),
                    gfx2::DrawInstruction::Draw(gfx2::DrawMesh {
                        vertex_buffer,
                        index_buffer,
                        texture: None,
                        num_elements: 6
                    })
                ],
                dcs: vec![]
            }
        ]
    };
    render_api.replace_draw_call(vec![], dc).await;
    (vertex_buffer, index_buffer)
}

async fn amain(ex: Arc<smol::Executor<'static>>, render_api: Arc<gfx2::RenderApi>,
    event_sub: pubsub::Subscription<gfx2::GraphicsEvent>
    ) {

    let task = ex.spawn(async move {
        let (vert_buffer, idx_buffer) = greensq(render_api).await;
        loop {
            let ev = event_sub.receive().await;
            debug!("ev: {:?}", ev);
        }
    });

    smol::Timer::after(std::time::Duration::from_secs(2)).await;

    let x1 = 0.1;
    let x2 = 0.95;
    let y1 = 0.1;
    let y2 = 0.95;
    let color = [0., 1., 0., 1.];

    let verts = vec![
        gfx2::Vertex { pos: [x1, y1], color, uv: [0., 0.] },
        gfx2::Vertex { pos: [x2, y1], color, uv: [1., 0.] },
        gfx2::Vertex { pos: [x1, y2], color, uv: [0., 1.] },
        gfx2::Vertex { pos: [x2, y2], color, uv: [1., 1.] },
    ];
    let vertex_buffer2 = render_api.new_vertex_buffer(verts).await.unwrap();

    let dc = gfx2::DrawCall {
        instrs: vec![
            gfx2::DrawInstruction::ApplyMatrix(model),
            gfx2::DrawInstruction::Draw(gfx2::DrawMesh {
                vertex_buffer: vertex_buffer2,
                index_buffer,
                texture: None,
                num_elements: 6
            })
        ],
        dcs: vec![]
    };
    render_api.replace_draw_call(vec![0], dc).await;
    //render_api.delete_buffer(vertex_buffer);

    println!("hello!");
}
*/

fn main() {
    // [x] event pub should be a Publisher
    // [ ] properties should have post-modify hook used to redraw widgets

    let ex = Arc::new(smol::Executor::new());
    let sg = Arc::new(Mutex::new(SceneGraph::new()));

    let sg2 = sg.clone();
    let ex2 = ex.clone();
    let zmq_task = ex.spawn(async {
        let mut zmq_rpc = ZeroMQAdapter::new(sg2, ex2).await;
        zmq_rpc.run().await;
    });

    let (method_req, method_rep) = mpsc::channel();
    let render_api = gfx2::RenderApi::new(method_req);
    let event_pub = pubsub::Publisher::new();

    let app = app::App::new(sg.clone(), ex.clone(), render_api.clone(), event_pub.clone());
    let app_task = ex.spawn(app.clone().start());

    // Nice to see which events exist
    let ev_sub = event_pub.clone().subscribe();
    let ev_relay_task = ex.spawn(async move {
        loop {
            let Ok(ev) = ev_sub.receive().await else {
                debug!("Event relayer closed");
                break
            };
            // Ignore keys which get stuck
            match &ev {
                gfx2::GraphicsEvent::KeyDown((miniquad::KeyCode::LeftShift, _, _)) |
                gfx2::GraphicsEvent::KeyDown((miniquad::KeyCode::LeftSuper, _, _)) => continue,
                _ => {}
            }
            debug!("event: {:?}", ev);
        }
    });
    // End debug code

    let n_threads = std::thread::available_parallelism().unwrap().get();
    let (signal, shutdown) = smol::channel::unbounded::<()>();
    let exec_threadpool = thread::spawn(move || {
        easy_parallel::Parallel::new()
            // N executor threads
            .each(0..n_threads, |_| smol::future::block_on(ex.run(shutdown.recv())))
            .run();
    });

    gfx2::run_gui(method_rep, event_pub);

    // Close all tasks
    smol::future::block_on(async {
        // Perform cleanup code
        // If not finished in certain amount of time, then just exit

        let mut futures = FuturesUnordered::new();
        futures.push(zmq_task.cancel());
        futures.push(ev_relay_task.cancel());
        futures.push(app_task.cancel());
        let _: Vec<_> = futures.collect().await;

        app.stop().await;
    });

    drop(signal);
    exec_threadpool.join();
    debug!("Application closed");
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
