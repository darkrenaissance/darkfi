use async_lock::Mutex;
use async_recursion::async_recursion;
use futures::{stream::FuturesUnordered, StreamExt};
use rand::{rngs::OsRng, Rng};
use std::{
    sync::{mpsc, Arc, Weak},
    thread,
};

use crate::{
    chatapp,
    error::{Error, Result},
    expr::{Op, SExprMachine, SExprVal},
    gfx::Rectangle,
    gfx2::{DrawCall, DrawInstruction, DrawMesh, GraphicsEvent, RenderApiPtr, Vertex},
    prop::{
        Property, PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyStr,
        PropertySubType, PropertyType, PropertyUint32,
    },
    pubsub::PublisherPtr,
    scene::{
        MethodResponseFn, Pimpl, SceneGraph, SceneGraphPtr2, SceneNode, SceneNodeId, SceneNodeInfo,
        SceneNodeType,
    },
};

trait Stoppable {
    async fn stop(self);
}

pub struct App {
    sg: SceneGraphPtr2,
    ex: Arc<smol::Executor<'static>>,
    render_api: RenderApiPtr,
    event_pub: PublisherPtr<GraphicsEvent>,
}

impl App {
    pub fn new(
        sg: SceneGraphPtr2,
        ex: Arc<smol::Executor<'static>>,
        render_api: RenderApiPtr,
        event_pub: PublisherPtr<GraphicsEvent>,
    ) -> Arc<Self> {
        debug!("App::new()");
        Arc::new(Self { sg, ex, render_api, event_pub })
    }

    pub async fn start(self: Arc<Self>) {
        debug!("App::start()");
        // Setup UI
        let mut sg = self.sg.lock().await;

        let window = sg.add_node("window", SceneNodeType::Window);

        let mut prop = Property::new("screen_size", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_array_len(2);
        // Window not yet initialized so we can't set these.
        //prop.set_f32(0, screen_width);
        //prop.set_f32(1, screen_height);
        window.add_property(prop).unwrap();

        let mut prop = Property::new("scale", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_defaults_f32(vec![1.]).unwrap();
        window.add_property(prop).unwrap();

        let window_id = window.id;

        // Create Window
        // Window::new(window, weak sg)
        drop(sg);
        let pimpl = Window::new(
            self.sg.clone(),
            window_id,
            self.ex.clone(),
            self.render_api.clone(),
            self.event_pub.clone(),
        )
        .await;
        // -> reads any props it needs
        // -> starts procs
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(window_id).unwrap();
        node.pimpl = pimpl;

        sg.link(window_id, SceneGraph::ROOT_ID).unwrap();

        // Testing
        let node = sg.get_node(window_id).unwrap();
        node.set_property_f32("scale", 2.).unwrap();

        drop(sg);

        self.make_me_a_schema_plox().await;

        // Access drawable in window node and call draw()
        self.trigger_redraw().await;
    }

    async fn make_me_a_schema_plox(&self) {
        // Create a layer called view
        let mut sg = self.sg.lock().await;
        let layer_node_id = chatapp::create_layer(&mut sg, "view");

        // Customize our layer
        let node = sg.get_node(layer_node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(0, 0.).unwrap();
        prop.set_f32(1, 0.).unwrap();
        let code = vec![Op::LoadVar("sw".to_string())];
        prop.set_expr(2, code).unwrap();
        let code = vec![Op::LoadVar("sh".to_string())];
        prop.set_expr(3, code).unwrap();
        node.set_property_bool("is_visible", true).unwrap();

        // Setup the pimpl
        let node_id = node.id;
        drop(sg);
        let pimpl = RenderLayer::new(self.sg.clone(), node_id).await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        let window_id = sg.lookup_node("/window").unwrap().id;
        sg.link(node_id, window_id).unwrap();

        // Create a mesh
        let node_id = chatapp::create_mesh(&mut sg, "bg");

        let node = sg.get_node_mut(node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(0, 0.).unwrap();
        prop.set_f32(1, 0.).unwrap();
        let code = vec![Op::LoadVar("lw".to_string())];
        prop.set_expr(2, code).unwrap();
        let code = vec![Op::LoadVar("lh".to_string())];
        prop.set_expr(3, code).unwrap();

        // Setup the pimpl
        let node_id = node.id;
        let (x1, y1) = (0., 0.);
        let (x2, y2) = (1., 1.);
        let verts = vec![
            // top left
            Vertex { pos: [x1, y1], color: [1., 0., 0., 1.], uv: [0., 0.] },
            // top right
            Vertex { pos: [x2, y1], color: [1., 0., 1., 1.], uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x1, y2], color: [0., 0., 1., 1.], uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x2, y2], color: [1., 1., 0., 1.], uv: [1., 1.] },
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];
        drop(sg);
        let pimpl =
            Mesh::new(self.sg.clone(), node_id, self.render_api.clone(), verts, indices).await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();
    }

    async fn trigger_redraw(&self) {
        let sg = self.sg.lock().await;
        let window_node = sg.lookup_node("/window").expect("no window attached!");
        match &window_node.pimpl {
            Pimpl::Window(win) => win.draw(&sg).await,
            _ => panic!("wrong pimpl"),
        }
    }

    pub async fn stop(&self) {
        // Go through event graph and call stop on everything
        // Depth first
        debug!("Stopping app...");
    }
}

fn print_type_of<T>(_: &T) {
    println!("{}", std::any::type_name::<T>())
}

pub type WindowPtr = Arc<Window>;

pub struct Window {
    node_id: SceneNodeId,
    resize_task: smol::Task<()>,
    modify_task: smol::Task<()>,
    screen_size_prop: PropertyPtr,
    render_api: RenderApiPtr,
}

impl Window {
    pub async fn new(
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        ex: Arc<smol::Executor<'static>>,
        render_api: RenderApiPtr,
        event_pub: PublisherPtr<GraphicsEvent>,
    ) -> Pimpl {
        debug!("Window::new()");

        let screen_size_prop = {
            let sg = sg.lock().await;
            let node = sg.get_node(node_id).unwrap();
            node.get_property("screen_size").unwrap()
        };

        // Start a task monitoring for window resize events
        // which updates screen_size
        let ev_sub = event_pub.subscribe();
        let screen_size_prop2 = screen_size_prop.clone();
        let resize_task = ex.spawn(async move {
            loop {
                let Ok(ev) = ev_sub.receive().await else {
                    debug!("Event relayer closed");
                    break
                };
                let (w, h) = match ev {
                    GraphicsEvent::Resize((w, h)) => (w, h),
                    _ => continue,
                };

                // Now update the properties
                screen_size_prop2.set_f32(0, w);
                screen_size_prop2.set_f32(1, h);
            }
        });

        // Monitor for changes to screen_size or scale properties
        // If so then trigger draw
        let scale_sub = {
            let sg = sg.lock().await;
            let node = sg.get_node(node_id).unwrap();
            let prop = node.get_property("scale").unwrap();
            prop.subscribe_modify()
        };
        let screen_size_sub = screen_size_prop.subscribe_modify();

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            // Modify task needs a Weak<Self>
            let me2 = me.clone();
            let modify_task = ex.spawn(async move {
                loop {
                    let mut futures = FuturesUnordered::new();
                    futures.push(scale_sub.receive());
                    futures.push(screen_size_sub.receive());

                    while let Some(ev) = futures.next().await {
                        let Ok(_) = ev else {
                            debug!("prop sub closed");
                            break
                        };

                        let Some(self_) = me2.upgrade() else {
                            // Should not happen
                            panic!("self destroyed before modify_task was stopped!");
                        };

                        let sg = sg.lock().await;
                        self_.draw(&sg).await;
                    }
                }
            });

            Self { node_id, resize_task, modify_task, screen_size_prop, render_api }
        });

        Pimpl::Window(self_)
    }

    async fn draw(&self, sg: &SceneGraph) {
        debug!("Window::draw()");
        // SceneGraph should remain locked for the entire draw
        let self_node = sg.get_node(self.node_id).unwrap();

        let screen_width = self.screen_size_prop.get_f32(0).unwrap();
        let screen_height = self.screen_size_prop.get_f32(1).unwrap();

        let parent_rect = Rectangle { x: 0., y: 0., w: screen_width, h: screen_height };

        let mut draw_calls = vec![];
        let mut child_calls = vec![];
        for child_inf in self_node.get_children2() {
            let node = sg.get_node(child_inf.id).unwrap();
            debug!("Window::draw() calling draw() for node '{}':{}", node.name, node.id);

            let dcs = match &node.pimpl {
                Pimpl::RenderLayer(layer) => layer.draw(sg, &parent_rect).await,
                _ => {
                    error!("unhandled pimpl type");
                    continue
                }
            };
            let Some((dc_key, mut dcs)) = dcs else { continue };
            draw_calls.append(&mut dcs);
            child_calls.push(dc_key);
        }

        let root_dc = DrawCall { instrs: vec![], dcs: child_calls };
        draw_calls.push((0, root_dc));
        println!("{:?}", draw_calls);

        self.render_api.replace_draw_calls(draw_calls).await;
        debug!("Window::draw() - replaced draw call");
    }
}

// Nodes should be stopped before being removed
impl Stoppable for Window {
    async fn stop(self) {
        self.resize_task.cancel().await;
    }
}

pub type RenderLayerPtr = Arc<RenderLayer>;

pub struct RenderLayer {
    sg: SceneGraphPtr2,
    node_id: SceneNodeId,

    dc_key: u64,

    is_visible: PropertyBool,
    rect: PropertyPtr,
}

impl RenderLayer {
    pub async fn new(sg_ptr: SceneGraphPtr2, node_id: SceneNodeId) -> Pimpl {
        let sg_ptr2 = sg_ptr.clone();
        let sg = sg_ptr2.lock().await;
        let node = sg.get_node(node_id).unwrap();

        let is_visible =
            PropertyBool::wrap(node, "is_visible", 0).expect("RenderLayer::is_visible");
        let rect = node.get_property("rect").expect("RenderLayer::rect");

        let self_ = Arc::new(Self { sg: sg_ptr, node_id, dc_key: OsRng.gen(), is_visible, rect });

        Pimpl::RenderLayer(self_)
    }

    fn get_rect(&self, parent_rect: &Rectangle<f32>) -> Result<Rectangle<f32>> {
        if self.rect.array_len != 4 {
            return Err(Error::PropertyWrongLen)
        }

        let mut rect = [0.; 4];
        for i in 0..4 {
            if self.rect.is_expr(i)? {
                let expr = self.rect.get_expr(i).unwrap();

                let machine = SExprMachine {
                    globals: vec![
                        ("sw".to_string(), SExprVal::Float32(parent_rect.w)),
                        ("sh".to_string(), SExprVal::Float32(parent_rect.h)),
                    ],
                    stmts: &expr,
                };

                rect[i] = machine.call()?.as_f32()?;
            } else {
                rect[i] = self.rect.get_f32(i)?;
            }
        }
        Ok(Rectangle::from_array(rect))
    }

    #[async_recursion]
    pub async fn draw(
        &self,
        sg: &SceneGraph,
        parent_rect: &Rectangle<f32>,
    ) -> Option<(u64, Vec<(u64, DrawCall)>)> {
        debug!("RenderLayer::draw()");
        let node = sg.get_node(self.node_id).unwrap();

        if !self.is_visible.get() {
            debug!("invisible layer node '{}':{}", node.name, node.id);
            return None
        }

        let Ok(rect) = self.get_rect(parent_rect) else {
            panic!("malformed rect property for node '{}':{}", node.name, node.id)
        };

        if !parent_rect.includes(&rect) {
            error!(
                "layer '{}':{} rect {:?} is not inside parent {:?}",
                node.name, node.id, rect, parent_rect
            );
            return None
        }

        // Apply viewport

        let mut draw_calls = vec![];
        let mut child_calls = vec![];
        for child_inf in node.get_children2() {
            let node = sg.get_node(child_inf.id).unwrap();

            let dcs = match &node.pimpl {
                Pimpl::RenderLayer(layer) => layer.draw(&sg, &rect).await,
                Pimpl::Mesh(mesh) => mesh.draw(&sg, &rect),
                _ => {
                    error!("unhandled pimpl type");
                    continue
                }
            };
            let Some((dc_key, mut dcs)) = dcs else { continue };
            draw_calls.append(&mut dcs);
            child_calls.push(dc_key);
        }

        let dc = DrawCall { instrs: vec![DrawInstruction::ApplyViewport(rect)], dcs: child_calls };
        draw_calls.push((self.dc_key, dc));
        Some((self.dc_key, draw_calls))
    }
}

pub struct Mesh {
    render_api: RenderApiPtr,
    vertex_buffer: miniquad::BufferId,
    index_buffer: miniquad::BufferId,
    // Texture
    num_elements: i32,

    dc_key: u64,

    node_id: SceneNodeId,
    rect: PropertyPtr,
}

impl Mesh {
    pub async fn new(
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        verts: Vec<Vertex>,
        indices: Vec<u16>,
    ) -> Pimpl {
        let num_elements = indices.len() as i32;
        let vertex_buffer = render_api.new_vertex_buffer(verts).await.unwrap();
        let index_buffer = render_api.new_index_buffer(indices).await.unwrap();

        let mut sg = sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        let rect = node.get_property("rect").expect("RenderLayer::rect");

        Pimpl::Mesh(Self {
            render_api,
            vertex_buffer,
            index_buffer,
            num_elements,
            dc_key: OsRng.gen(),
            node_id,
            rect,
        })
    }

    // Merge with RenderLayer::get_rect()
    fn get_rect(&self, parent_rect: &Rectangle<f32>) -> Result<Rectangle<f32>> {
        if self.rect.array_len != 4 {
            return Err(Error::PropertyWrongLen)
        }

        let mut rect = [0.; 4];
        for i in 0..4 {
            if self.rect.is_expr(i)? {
                let expr = self.rect.get_expr(i).unwrap();

                let machine = SExprMachine {
                    globals: vec![
                        ("lw".to_string(), SExprVal::Float32(parent_rect.w)),
                        ("lh".to_string(), SExprVal::Float32(parent_rect.h)),
                    ],
                    stmts: &expr,
                };

                rect[i] = machine.call()?.as_f32()?;
            } else {
                rect[i] = self.rect.get_f32(i)?;
            }
        }
        Ok(Rectangle::from_array(rect))
    }

    pub fn draw(
        &self,
        sg: &SceneGraph,
        parent_rect: &Rectangle<f32>,
    ) -> Option<(u64, Vec<(u64, DrawCall)>)> {
        // Only used for debug messages
        let node = sg.get_node(self.node_id).unwrap();

        let mesh = DrawMesh {
            vertex_buffer: self.vertex_buffer,
            index_buffer: self.index_buffer,
            texture: None,
            num_elements: self.num_elements,
        };

        let Ok(rect) = self.get_rect(parent_rect) else {
            panic!("malformed rect property for node '{}':{}", node.name, node.id)
        };

        // FIXME: all these rects must be aggregated down the tree
        let scale_x = rect.w / parent_rect.w;
        let scale_y = rect.h / parent_rect.h;
        let model = glam::Mat4::from_translation(glam::Vec3::new(rect.x, rect.y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        Some((
            self.dc_key,
            vec![(
                self.dc_key,
                DrawCall {
                    instrs: vec![
                        DrawInstruction::ApplyMatrix(glam::Mat4::IDENTITY),
                        DrawInstruction::Draw(mesh),
                    ],
                    dcs: vec![],
                },
            )],
        ))
    }
}

impl Stoppable for Mesh {
    async fn stop(self) {
        // TODO: Delete own draw call

        // Free buffers
        // Should this be in drop?
        self.render_api.delete_buffer(self.vertex_buffer);
        self.render_api.delete_buffer(self.index_buffer);
    }
}
