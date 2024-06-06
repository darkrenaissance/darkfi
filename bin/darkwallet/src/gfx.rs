use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use freetype as ft;
use log::LevelFilter;
use miniquad::{
    conf, window, Backend, Bindings, BlendFactor, BlendState, BlendValue, BufferId, BufferLayout,
    BufferSource, BufferType, BufferUsage, Equation, EventHandler, KeyCode, KeyMods, MouseButton,
    PassAction, Pipeline, PipelineParams, RenderingBackend, ShaderMeta, ShaderSource, TextureId,
    UniformDesc, UniformType, VertexAttribute, VertexFormat,
};
use std::{
    array::IntoIter,
    fmt,
    io::Cursor,
    sync::{mpsc, Arc, MutexGuard},
    time::{Duration, Instant},
};

use crate::{
    error::{Error, Result},
    chatview,
    editbox,
    expr::{SExprMachine, SExprVal},
    keysym::{KeyCodeAsStr, MouseButtonAsU8},
    prop::{Property, PropertySubType, PropertyType},
    res::{ResourceId, ResourceManager},
    scene::{
        MethodResponseFn, SceneGraph, SceneGraphPtr, SceneNode, SceneNodeId, SceneNodeInfo,
        SceneNodeType, Pimpl
    },
    shader,
};

type Color = [f32; 4];

pub const COLOR_RED: Color = [1., 0., 0., 1.];
pub const COLOR_DARKGREY: Color = [0.2, 0.2, 0.2, 1.];
pub const COLOR_GREEN: Color = [0., 1., 0., 1.];
pub const COLOR_BLUE: Color = [0., 0., 1., 1.];
pub const COLOR_WHITE: Color = [1., 1., 1., 1.];

#[derive(Debug, SerialEncodable, SerialDecodable)]
#[repr(C)]
struct Vertex {
    pos: [f32; 2],
    color: [f32; 4],
    uv: [f32; 2],
}

#[derive(SerialEncodable, SerialDecodable)]
#[repr(C)]
struct Face {
    idxs: [u32; 3],
}

impl fmt::Debug for Face {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.idxs)
    }
}

struct Mesh {
    pub verts: Vec<Vertex>,
    pub faces: Vec<Face>,
    pub vertex_buffer: BufferId,
    pub index_buffer: BufferId,
}

pub struct Point<T> {
    pub x: T,
    pub y: T,
}

#[derive(Debug, Clone)]
pub struct Rectangle<T: Copy + std::ops::Add<Output=T> + std::ops::Sub<Output=T> + std::cmp::PartialOrd> {
    pub x: T,
    pub y: T,
    pub w: T,
    pub h: T,
}

impl<T: Copy + std::ops::Add<Output=T> + std::ops::Sub<Output=T> + std::ops::AddAssign + std::cmp::PartialOrd> Rectangle<T> {
    fn from_array(arr: [T; 4]) -> Self {
        let mut iter = IntoIter::new(arr);
        Self {
            x: iter.next().unwrap(),
            y: iter.next().unwrap(),
            w: iter.next().unwrap(),
            h: iter.next().unwrap(),
        }
    }

    pub fn clip(&self, other: &Self) -> Option<Self> {
        if other.x + other.w < self.x ||
            other.x > self.x + self.w ||
            other.y + other.h < self.y ||
            other.y > self.y + self.h
        {
            return None
        }

        let mut clipped = other.clone();
        if clipped.x < self.x {
            clipped.x = self.x;
            clipped.w = other.x + other.w - clipped.x;
        }
        if clipped.y < self.y {
            clipped.y = self.y;
            clipped.h = other.y + other.h - clipped.y;
        }
        if clipped.x + clipped.w > self.x + self.w {
            clipped.w = self.x + self.w - clipped.x;
        }
        if clipped.y + clipped.h > self.y + self.h {
            clipped.h = self.y + self.h - clipped.y;
        }
        Some(clipped)
    }

    pub fn contains(&self, point: &Point<T>) -> bool {
        self.x < point.x && point.x < self.x + self.w &&
            self.y < point.y && point.y < self.y + self.h
    }
}

pub type FreetypeFace = ft::Face<&'static [u8]>;

#[derive(Debug)]
enum GraphicsMethodEvent {
    LoadTexture,
    DeleteTexture,
    CreateChatView,
    CreateEditBox,
}

struct Stage {
    ctx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,

    scene_graph: SceneGraphPtr,

    textures: ResourceManager<TextureId>,
    font_faces: Vec<FreetypeFace>,

    method_recvr: mpsc::Receiver<(GraphicsMethodEvent, SceneNodeId, Vec<u8>, MethodResponseFn)>,
    method_sender: mpsc::SyncSender<(GraphicsMethodEvent, SceneNodeId, Vec<u8>, MethodResponseFn)>,

    last_draw_time: Option<Instant>,
}

impl Stage {
    const WHITE_TEXTURE_ID: ResourceId = 0;

    pub fn new(scene_graph: SceneGraphPtr) -> Self {
        let mut ctx: Box<dyn RenderingBackend> = window::new_rendering_backend();

        let white_texture = ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]);
        let mut textures = ResourceManager::new();
        let white_texture_id = textures.alloc(white_texture);
        assert_eq!(white_texture_id, Self::WHITE_TEXTURE_ID);

        let mut shader_meta: ShaderMeta = shader::meta();
        shader_meta.uniforms.uniforms.push(UniformDesc::new("Projection", UniformType::Mat4));
        shader_meta.uniforms.uniforms.push(UniformDesc::new("Model", UniformType::Mat4));

        let shader = ctx
            .new_shader(
                match ctx.info().backend {
                    Backend::OpenGl => ShaderSource::Glsl {
                        vertex: shader::GL_VERTEX,
                        fragment: shader::GL_FRAGMENT,
                    },
                    Backend::Metal => ShaderSource::Msl { program: shader::METAL },
                },
                shader_meta,
            )
            .unwrap();

        let params = PipelineParams {
            color_blend: Some(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
            ..Default::default()
        };

        let pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("in_pos", VertexFormat::Float2),
                VertexAttribute::new("in_color", VertexFormat::Float4),
                VertexAttribute::new("in_uv", VertexFormat::Float2),
            ],
            shader,
            params,
        );

        let (method_sender, method_recvr) = mpsc::sync_channel(100);

        let ftlib = ft::Library::init().unwrap();
        let mut font_faces = vec![];

        let font_data = include_bytes!("../ibm-plex-mono-light.otf") as &[u8];
        let ft_face = ftlib.new_memory_face2(font_data, 0).unwrap();
        font_faces.push(ft_face);

        let font_data = include_bytes!("../NotoColorEmoji.ttf") as &[u8];
        let ft_face = ftlib.new_memory_face2(font_data, 0).unwrap();
        font_faces.push(ft_face);

        let mut stage = Stage {
            ctx,
            pipeline,
            scene_graph,
            textures,
            font_faces,
            method_recvr,
            method_sender,
            last_draw_time: None,
        };
        stage.setup_scene_graph_window();

        stage.setup_scene();

        debug!("Finished loading GUI");
        stage
    }

    fn setup_scene_graph_window(&mut self) {
        let mut scene_graph = self.scene_graph.lock().unwrap();

        let window = scene_graph.add_node("window", SceneNodeType::Window);
        let (screen_width, screen_height) = window::screen_size();

        let mut prop = Property::new("screen_size", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_array_len(2);
        prop.set_f32(0, screen_width);
        prop.set_f32(1, screen_height);
        window.add_property(prop).unwrap();

        let mut prop = Property::new("scale", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_defaults_f32(vec![1.]).unwrap();
        window.add_property(prop).unwrap();

        window
            .add_signal(
                "resize",
                "Screen resize event",
                vec![
                    ("screen_width", "", PropertyType::Float32),
                    ("screen_height", "", PropertyType::Float32),
                ],
            )
            .unwrap();

        let window_id = window.id;

        let sender = self.method_sender.clone();
        let method_fn = Box::new(move |arg_data, response_fn| {
            sender.send((GraphicsMethodEvent::LoadTexture, window_id, arg_data, response_fn));
        });
        window
            .add_method(
                "load_texture",
                vec![("node_name", "", PropertyType::Str), ("path", "", PropertyType::Str)],
                vec![("node_id", "", PropertyType::SceneNodeId)],
                method_fn,
            )
            .unwrap();

        let sender = self.method_sender.clone();
        let method_fn = Box::new(move |arg_data, response_fn| {
            sender.send((GraphicsMethodEvent::CreateChatView, window_id, arg_data, response_fn));
        });
        window
            .add_method(
                "create_chat_view",
                vec![("node_id", "", PropertyType::SceneNodeId)],
                vec![],
                method_fn,
            )
            .unwrap();

        let sender = self.method_sender.clone();
        let method_fn = Box::new(move |arg_data, response_fn| {
            sender.send((GraphicsMethodEvent::CreateEditBox, window_id, arg_data, response_fn));
        });
        window
            .add_method(
                "create_edit_box",
                vec![("node_id", "", PropertyType::SceneNodeId)],
                vec![],
                method_fn,
            )
            .unwrap();

        scene_graph.link(window_id, SceneGraph::ROOT_ID).unwrap();

        let input = scene_graph.add_node("input", SceneNodeType::WindowInput);
        let input_id = input.id;
        scene_graph.link(input_id, window_id).unwrap();

        let keyb = scene_graph.add_node("keyboard", SceneNodeType::Keyboard);
        keyb.add_signal(
            "key_down",
            "Key press down event",
            vec![
                ("shift", "", PropertyType::Bool),
                ("ctrl", "", PropertyType::Bool),
                ("alt", "", PropertyType::Bool),
                ("logo", "", PropertyType::Bool),
                ("repeat", "", PropertyType::Bool),
                ("keycode", "", PropertyType::Enum),
            ],
        )
        .unwrap();
        keyb.add_signal(
            "key_up",
            "Key press up event",
            vec![
                ("shift", "", PropertyType::Bool),
                ("ctrl", "", PropertyType::Bool),
                ("alt", "", PropertyType::Bool),
                ("logo", "", PropertyType::Bool),
                ("repeat", "", PropertyType::Bool),
                ("keycode", "", PropertyType::Enum),
            ],
        )
        .unwrap();
        let keyb_id = keyb.id;
        scene_graph.link(keyb_id, input_id).unwrap();

        let mouse = scene_graph.add_node("mouse", SceneNodeType::Mouse);
        mouse
            .add_signal(
                "button_up",
                "Mouse button up event",
                vec![
                    ("button", "", PropertyType::Enum),
                    ("x", "", PropertyType::Float32),
                    ("y", "", PropertyType::Float32),
                ],
            )
            .unwrap();
        mouse
            .add_signal(
                "button_down",
                "Mouse button down event",
                vec![
                    ("button", "", PropertyType::Enum),
                    ("x", "", PropertyType::Float32),
                    ("y", "", PropertyType::Float32),
                ],
            )
            .unwrap();
        mouse
            .add_signal(
                "wheel",
                "Mouse wheel scroll event",
                vec![("x", "", PropertyType::Float32), ("y", "", PropertyType::Float32)],
            )
            .unwrap();
        mouse
            .add_signal(
                "move",
                "Mouse cursor move event",
                vec![("x", "", PropertyType::Float32), ("y", "", PropertyType::Float32)],
            )
            .unwrap();
        let mouse_id = mouse.id;
        scene_graph.link(mouse_id, input_id).unwrap();
    }

    fn method_load_texture(&mut self, node_id: SceneNodeId, arg_data: Vec<u8>) -> Result<Vec<u8>> {
        let mut cur = Cursor::new(&arg_data);
        let node_name = String::decode(&mut cur).unwrap();
        let filepath = String::decode(&mut cur).unwrap();

        let Ok(img) = image::open(filepath) else { return Err(Error::FileNotFound) };

        let img = img.to_rgba8();
        let width = img.width();
        let height = img.height();
        let bmp = img.into_raw();

        let texture = self.ctx.new_texture_from_rgba8(width as u16, height as u16, &bmp);
        let id = self.textures.alloc(texture);

        let mut scene_graph = self.scene_graph.lock().unwrap();
        let img_node = scene_graph.add_node(node_name, SceneNodeType::RenderTexture);

        let mut prop = Property::new("size", PropertyType::Uint32, PropertySubType::Pixel);
        prop.set_array_len(2);
        prop.set_u32(0, width).unwrap();
        prop.set_u32(1, height).unwrap();
        img_node.add_property(prop)?;

        let mut prop =
            Property::new("texture_rid", PropertyType::Uint32, PropertySubType::ResourceId);
        prop.set_u32(0, id).unwrap();
        img_node.add_property(prop)?;

        let mut reply = vec![];
        img_node.id.encode(&mut reply).unwrap();

        Ok(reply)
    }
    fn method_delete_texture(
        &mut self,
        node_id: SceneNodeId,
        arg_data: Vec<u8>,
    ) -> Result<Vec<u8>> {
        let mut cur = Cursor::new(&arg_data);
        let texture_id = ResourceId::decode(&mut cur).unwrap();
        let texture = self.textures.get(texture_id).ok_or(Error::ResourceNotFound)?;
        self.ctx.delete_texture(*texture);
        self.textures.free(texture_id);
        Ok(vec![])
    }

    fn method_create_chatview(
        &mut self,
        _: SceneNodeId,
        arg_data: Vec<u8>,
    ) -> Result<Vec<u8>> {
        debug!("gfx::create_chatview()");
        let mut cur = Cursor::new(&arg_data);
        let node_id = SceneNodeId::decode(&mut cur).unwrap();

        let mut scene_graph = self.scene_graph.lock().unwrap();
        let editbox = chatview::ChatView::new(&mut scene_graph, node_id, self.font_faces.clone())?;

        let node = scene_graph.get_node_mut(node_id).ok_or(Error::NodeNotFound)?;
        node.pimpl = editbox;

        let mut reply = vec![];
        Ok(reply)
    }

    fn method_create_editbox(
        &mut self,
        _: SceneNodeId,
        arg_data: Vec<u8>,
    ) -> Result<Vec<u8>> {
        debug!("gfx::create_editbox()");
        let mut cur = Cursor::new(&arg_data);
        let node_id = SceneNodeId::decode(&mut cur).unwrap();

        let mut scene_graph = self.scene_graph.lock().unwrap();
        let editbox = editbox::EditBox::new(&mut scene_graph, node_id, self.font_faces.clone())?;

        let node = scene_graph.get_node_mut(node_id).ok_or(Error::NodeNotFound)?;
        node.pimpl = editbox;

        let mut reply = vec![];
        Ok(reply)
    }

    fn setup_scene(&mut self) {
        let mut sg = self.scene_graph.lock().unwrap();
        crate::chatapp::setup(&mut sg);
    }
}

pub struct RenderContext<'a> {
    pub scene_graph: MutexGuard<'a, SceneGraph>,
    pub ctx: &'a mut Box<dyn RenderingBackend>,
    pub pipeline: &'a Pipeline,
    pub proj: glam::Mat4,
    pub textures: &'a ResourceManager<TextureId>,
    pub font_faces: &'a Vec<FreetypeFace>,
}

impl<'a> RenderContext<'a> {
    fn render_window(&mut self) {
        for layer in self
            .scene_graph
            .lookup_node("/window")
            .expect("no window attached!")
            .get_children(&[SceneNodeType::RenderLayer])
        {
            if let Err(err) = self.render_layer(layer.id) {
                error!("error rendering layer '{}': {}", layer.name, err)
            }
        }

        self.ctx.commit_frame();
    }

    fn get_rect(layer: &SceneNode) -> Result<Rectangle<i32>> {
        let prop = layer.get_property("rect").ok_or(Error::PropertyNotFound)?;
        if prop.array_len != 4 {
            return Err(Error::PropertyWrongLen)
        }

        let mut rect = [0; 4];
        for i in 0..4 {
            if prop.is_expr(i)? {
                let (screen_width, screen_height) = window::screen_size();

                let expr = prop.get_expr(i).unwrap();

                let machine = SExprMachine {
                    globals: vec![
                        ("sw".to_string(), SExprVal::Float32(screen_width)),
                        ("sh".to_string(), SExprVal::Float32(screen_height)),
                    ],
                    stmts: &expr,
                };

                rect[i] = machine.call()?.as_u32()? as i32;
            } else {
                rect[i] = prop.get_u32(i)? as i32;
            }
        }
        Ok(Rectangle::from_array(rect))
    }

    fn render_layer(
        &mut self,
        layer_id: SceneNodeId,
        // parent rect
    ) -> Result<()> {
        let layer = self.scene_graph.get_node(layer_id).unwrap();

        if !layer.get_property_bool("is_visible")? {
            return Ok(())
        }

        self.ctx.begin_default_pass(PassAction::Nothing);
        self.ctx.apply_pipeline(&self.pipeline);

        let (_, screen_height) = window::screen_size();

        let rect = Self::get_rect(&layer)?;

        let mut view = rect.clone();
        view.y = screen_height as i32 - (rect.y + rect.h);
        self.ctx.apply_viewport(view.x, view.y, view.w, view.h);
        self.ctx.apply_scissor_rect(view.x, view.y, view.w, view.h);

        let window = self
            .scene_graph
            .lookup_node("/window")
            .expect("no window attached!");
        let window_scale = window.get_property_f32("scale")?;

        let rect = Rectangle {
            x: (rect.x as f32) / window_scale,
            y: (rect.y as f32) / window_scale,
            w: (rect.w as f32) / window_scale,
            h: (rect.h as f32) / window_scale,
        };

        let layer_children =
            layer.get_children(&[SceneNodeType::RenderMesh, SceneNodeType::RenderText, SceneNodeType::EditBox, SceneNodeType::ChatView]);
        let layer_children = self.order_by_z_index(layer_children);

        // get the rectangle
        // make sure it's inside the parent's rect
        for child in layer_children {
            // x, y, w, h as pixels

            // note that (x, y) is offset by layer rect so it is the pos within layer
            // layer coords are (0, 0) -> (1, 1)

            // optionally evaluated using sexpr

            // mesh data is (0, 0) to (1, 1)
            // so scale by (w, h)

            match child.typ {
                SceneNodeType::RenderMesh => {
                    if let Err(err) = self.render_mesh(child.id, &rect) {
                        error!("error rendering mesh '{}': {}", child.name, err);
                    }
                }
                SceneNodeType::RenderText => {
                    if let Err(err) = self.render_text(child.id, &rect) {
                        error!("error rendering text '{}': {}", child.name, err);
                    }
                }
                SceneNodeType::ChatView => {
                    let node = self.scene_graph.get_node(child.id).unwrap();
                    let chatview = match &node.pimpl {
                        Pimpl::Null => {
                            // Maybe the chatview isn't initialized fully yet
                            continue;
                        }
                        Pimpl::ChatView(e) => e.clone(),
                        _ => panic!("wrong pimpl for editbox")
                    };
                    if let Err(err) = chatview.render(self, child.id, &rect) {
                        error!("error rendering chatview '{}': {}", child.name, err);
                    }
                }
                SceneNodeType::EditBox => {
                    let node = self.scene_graph.get_node(child.id).unwrap();
                    let editbox = match &node.pimpl {
                        Pimpl::Null => {
                            // Maybe the editbox isn't initialized fully yet
                            continue;
                        }
                        Pimpl::EditBox(e) => e.clone(),
                        _ => panic!("wrong pimpl for editbox")
                    };
                    if let Err(err) = editbox.render(self, child.id, &rect) {
                        error!("error rendering editbox '{}': {}", child.name, err);
                    }
                }
                _ => panic!("render_layer(): unknown type"),
            }
        }

        Ok(())
    }

    fn order_by_z_index(&self, nodes: Vec<SceneNodeInfo>) -> Vec<SceneNodeInfo> {
        let mut nodes: Vec<_> = nodes
            .into_iter()
            .filter_map(|node_inf| {
                let node = self.scene_graph.get_node(node_inf.id).unwrap();
                //if !node.get_property_bool("is_visible").ok()? {
                //    return None
                //}
                let z_index = node.get_property_u32("z_index").ok()?;
                Some((z_index, node_inf))
            })
            .collect();
        nodes.sort_unstable_by_key(|(z_index, node_inf)| *z_index);
        nodes.into_iter().map(|(z_index, node_inf)| node_inf).collect()
    }

    pub fn get_dim(mesh: &SceneNode, layer_rect: &Rectangle<f32>) -> Result<Rectangle<f32>> {
        let prop = mesh.get_property("rect").ok_or(Error::PropertyNotFound)?;
        if prop.array_len != 4 {
            return Err(Error::PropertyWrongLen)
        }

        let mut rect = [0.; 4];
        for i in 0..4 {
            if prop.is_expr(i)? {
                let expr = prop.get_expr(i).unwrap();

                let machine = SExprMachine {
                    globals: vec![
                        ("lw".to_string(), SExprVal::Uint32(layer_rect.w as u32)),
                        ("lh".to_string(), SExprVal::Uint32(layer_rect.h as u32)),
                    ],
                    stmts: &expr,
                };

                rect[i] = machine.call()?.coerce_f32()?;
            } else {
                rect[i] = prop.get_f32(i)?;
            }
        }
        Ok(Rectangle::from_array(rect))
    }

    fn render_mesh(&mut self, node_id: SceneNodeId, layer_rect: &Rectangle<f32>) -> Result<()> {
        let mesh = self.scene_graph.get_node(node_id).unwrap();

        let z_index = mesh.get_property_u32("z_index")?;

        let data = mesh.get_property("data").ok_or(Error::PropertyNotFound)?;
        let verts = data.get_buf(0)?;
        let faces = data.get_buf(1)?;

        let vertex_buffer = self.ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&verts),
        );

        let bufsrc = unsafe {
            BufferSource::pointer(
                faces.as_ptr() as _,
                std::mem::size_of_val(&faces[..]),
                std::mem::size_of::<u32>(),
            )
        };

        let index_buffer =
            self.ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, bufsrc);

        // temp
        let texture = self.textures.get(Stage::WHITE_TEXTURE_ID).unwrap();

        let rect = Self::get_dim(mesh, layer_rect)?;
        //debug!("mesh rect: {:?}", rect);

        let layer_w = layer_rect.w as f32;
        let layer_h = layer_rect.h as f32;
        let off_x = rect.x / layer_w;
        let off_y = rect.y / layer_h;
        let scale_x = rect.w / layer_w;
        let scale_y = rect.h / layer_h;
        //let model = glam::Mat4::IDENTITY;
        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        let mut uniforms_data = [0u8; 128];
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&self.proj) };
        uniforms_data[0..64].copy_from_slice(&data);
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&model) };
        uniforms_data[64..].copy_from_slice(&data);
        assert_eq!(128, 2 * UniformType::Mat4.size());

        let bindings =
            Bindings { vertex_buffers: vec![vertex_buffer], index_buffer, images: vec![*texture] };

        self.ctx.apply_bindings(&bindings);

        self.ctx.apply_uniforms_from_bytes(uniforms_data.as_ptr(), uniforms_data.len());

        self.ctx.draw(0, 3 * faces.len() as i32, 1);

        self.ctx.delete_buffer(index_buffer);
        self.ctx.delete_buffer(vertex_buffer);

        Ok(())
    }

    pub fn render_clipped_box_with_texture2(
        &mut self,
        bound: &Rectangle<f32>,
        obj: &Rectangle<f32>,
        color: Color,
        texture: TextureId,
    ) {
        let Some(clipped) = bound.clip(&obj) else {
            return
        };

        let x1 = clipped.x;
        let y1 = clipped.y;
        let x2 = clipped.x + clipped.w;
        let y2 = clipped.y + clipped.h;

        let u1 = (clipped.x - obj.x) / obj.w;
        let u2 = (clipped.x + clipped.w - obj.x) / obj.w;
        let v1 = (clipped.y - obj.y) / obj.h;
        let v2 = (clipped.y + clipped.h - obj.y) / obj.h;

        let vertices: [Vertex; 4] = [
            // top left
            Vertex { pos: [x1, y1], color, uv: [u1, v1] },
            // top right
            Vertex { pos: [x2, y1], color, uv: [u2, v1] },
            // bottom left
            Vertex { pos: [x1, y2], color, uv: [u1, v2] },
            // bottom right
            Vertex { pos: [x2, y2], color, uv: [u2, v2] },
        ];

        let vertex_buffer = self.ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&vertices),
        );

        let indices: [u16; 6] = [0, 2, 1, 1, 2, 3];
        let index_buffer = self.ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );

        let bindings =
            Bindings { vertex_buffers: vec![vertex_buffer], index_buffer, images: vec![texture] };

        self.ctx.apply_bindings(&bindings);
        self.ctx.draw(0, 6, 1);
    }

    pub fn render_clipped_box_with_texture(
        &mut self,
        bound_rect: &Rectangle<f32>,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: Color,
        texture: TextureId,
    ) {
        let obj = Rectangle {
            x: x1,
            y: y1,
            w: x2 - x1,
            h: y2 - y1
        };
        if obj.w == 0. || obj.h == 0. {
            return
        }
        let Some(clipped) = bound_rect.clip(&obj) else {
            return
        };

        let x1 = clipped.x;
        let y1 = clipped.y;
        let x2 = clipped.x + clipped.w;
        let y2 = clipped.y + clipped.h;

        let u1 = (clipped.x - obj.x) / obj.w;
        let u2 = (clipped.x + clipped.w - obj.x) / obj.w;
        let v1 = (clipped.y - obj.y) / obj.h;
        let v2 = (clipped.y + clipped.h - obj.y) / obj.h;

        let vertices: [Vertex; 4] = [
            // top left
            Vertex { pos: [x1, y1], color, uv: [u1, v1] },
            // top right
            Vertex { pos: [x2, y1], color, uv: [u2, v1] },
            // bottom left
            Vertex { pos: [x1, y2], color, uv: [u1, v2] },
            // bottom right
            Vertex { pos: [x2, y2], color, uv: [u2, v2] },
        ];

        let vertex_buffer = self.ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&vertices),
        );

        let indices: [u16; 6] = [0, 2, 1, 1, 2, 3];
        let index_buffer = self.ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );

        let bindings =
            Bindings { vertex_buffers: vec![vertex_buffer], index_buffer, images: vec![texture] };

        self.ctx.apply_bindings(&bindings);
        self.ctx.draw(0, 6, 1);
    }

    pub fn render_box_with_texture(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: Color,
        texture: TextureId,
    ) {
        let vertices: [Vertex; 4] = [
            // top left
            Vertex { pos: [x1, y1], color, uv: [0., 0.] },
            // top right
            Vertex { pos: [x2, y1], color, uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x1, y2], color, uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x2, y2], color, uv: [1., 1.] },
        ];

        let vertex_buffer = self.ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&vertices),
        );

        let indices: [u16; 6] = [0, 2, 1, 1, 2, 3];
        let index_buffer = self.ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );

        let bindings =
            Bindings { vertex_buffers: vec![vertex_buffer], index_buffer, images: vec![texture] };

        self.ctx.apply_bindings(&bindings);
        self.ctx.draw(0, 6, 1);
    }

    pub fn render_box(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: Color) {
        let texture = self.textures.get(Stage::WHITE_TEXTURE_ID).unwrap();
        self.render_box_with_texture(x1, y1, x2, y2, color, *texture)
    }

    pub fn hline(&mut self, min_x: f32, max_x: f32, y: f32, color: Color, w: f32) {
        self.render_box(min_x, y - w / 2., max_x, y + w / 2., color)
    }

    pub fn vline(&mut self, x: f32, min_y: f32, max_y: f32, color: Color, w: f32) {
        self.render_box(x - w / 2., min_y, x + w / 2., max_y, color)
    }

    pub fn outline(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: Color, w: f32) {
        // top
        self.render_box(x1, y1, x2, y1 + w, color);
        // left
        self.render_box(x1, y1, x1 + w, y2, color);
        // right
        self.render_box(x2 - w, y1, x2, y2, color);
        // bottom
        self.render_box(x1, y2 - w, x2, y2, color);
    }

    fn render_text(&mut self, node_id: SceneNodeId, layer_rect: &Rectangle<f32>) -> Result<()> {
        let node = self.scene_graph.get_node(node_id).unwrap();

        let text = node.get_property_str("text")?;
        let font_size = node.get_property_f32("font_size")?;
        let debug = node.get_property_bool("debug")?;
        let rect = Self::get_dim(node, layer_rect)?;
        let baseline = node.get_property_f32("baseline")?;

        let color_prop = node.get_property("color").ok_or(Error::PropertyNotFound)?;
        let color_r = color_prop.get_f32(0)?;
        let color_g = color_prop.get_f32(1)?;
        let color_b = color_prop.get_f32(2)?;
        let color_a = color_prop.get_f32(3)?;

        let layer_w = layer_rect.w as f32;
        let layer_h = layer_rect.h as f32;
        let off_x = rect.x / layer_w;
        let off_y = rect.y / layer_h;
        // Use absolute pixel scale
        let scale_x = 1. / layer_w;
        let scale_y = 1. / layer_h;
        //let model = glam::Mat4::IDENTITY;
        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        let mut uniforms_data = [0u8; 128];
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&self.proj) };
        uniforms_data[0..64].copy_from_slice(&data);
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&model) };
        uniforms_data[64..].copy_from_slice(&data);
        assert_eq!(128, 2 * UniformType::Mat4.size());

        self.ctx.apply_uniforms_from_bytes(uniforms_data.as_ptr(), uniforms_data.len());

        // Used for scaling the font size
        let window = self
            .scene_graph
            .lookup_node("/window")
            .expect("no window attached!");
        let window_scale = window.get_property_f32("scale")?;
        let font_size = window_scale * font_size;

        //let mut strings = vec![];
        //let mut current_str = String::new();
        //let mut current_idx = 0;
        //for chr in text.chars() {
        //    let ft_face = self.font_faces[current_idx];
        //    if ft_face.get_char_index(chr as usize).is_some() {
        //    }
        //}

        let mut current_idx = 0;
        let mut current_str = String::new();
        let mut substrs = vec![];
        'next_char: for chr in text.chars() {
            let idx = 'get_idx: {
                for i in 0..self.font_faces.len() {
                    let ft_face = &self.font_faces[i];
                    if ft_face.get_char_index(chr as usize).is_some() {
                        break 'get_idx i
                    }
                }

                warn!("no font fallback for char: {}", chr);
                // Skip this char
                continue 'next_char
            };
            if current_idx != idx {
                if !current_str.is_empty() {
                    // Push
                    substrs.push((current_idx, current_str.clone()));
                }

                current_str.clear();
                current_idx = idx;
            }
            current_str.push(chr);
        }
        if !current_str.is_empty() {
            // Push
            substrs.push((current_idx, current_str));
        }

        let mut current_x = 0.;
        let mut current_y = baseline;

        for (face_idx, text) in substrs {
            let face = &self.font_faces[face_idx];
            if face.has_fixed_sizes() {
                // emojis required a fixed size
                //face.set_char_size(109 * 64, 0, 72, 72).unwrap();
                face.select_size(0).unwrap();
            } else {
                face.set_char_size(font_size as isize * 64, 0, 72, 72).unwrap();
            }

            let hb_font = harfbuzz_rs::Font::from_freetype_face(face.clone());
            let buffer = harfbuzz_rs::UnicodeBuffer::new().add_str(&text);
            let output = harfbuzz_rs::shape(&hb_font, buffer, &[]);

            let positions = output.get_glyph_positions();
            let infos = output.get_glyph_infos();

            for (position, info) in positions.iter().zip(infos) {
                let gid = info.codepoint;
                // Index within this substr
                // let cluster = info.cluster;

                let mut flags = ft::face::LoadFlag::DEFAULT;
                if face.has_color() {
                    flags |= ft::face::LoadFlag::COLOR;
                }
                face.load_glyph(gid, flags).unwrap();

                let glyph = face.glyph();
                glyph.render_glyph(ft::RenderMode::Normal).unwrap();

                // https://gist.github.com/jokertarot/7583938?permalink_comment_id=3327566#gistcomment-3327566

                let bmp = glyph.bitmap();
                let buffer = bmp.buffer();
                let bmp_width = bmp.width() as usize;
                let bmp_height = bmp.rows() as usize;
                let bearing_x = glyph.bitmap_left() as f32;
                let bearing_y = glyph.bitmap_top() as f32;

                //assert_eq!(bmp.pixel_mode().unwrap(), ft::bitmap::PixelMode::Bgra);
                //assert_eq!(bmp.pixel_mode().unwrap(), ft::bitmap::PixelMode::Lcd);
                //assert_eq!(bmp.pixel_mode().unwrap(), ft::bitmap::PixelMode::Gray);

                let pixel_mode = bmp.pixel_mode().unwrap();
                let tdata = match pixel_mode {
                    ft::bitmap::PixelMode::Bgra => {
                        let mut tdata = vec![];
                        tdata.resize(4 * bmp_width * bmp_height, 0);
                        // Convert from BGRA to RGBA
                        for i in 0..bmp_width*bmp_height as usize {
                            let idx = i*4;
                            let b = buffer[idx];
                            let g = buffer[idx + 1];
                            let r = buffer[idx + 2];
                            let a = buffer[idx + 3];
                            tdata[idx] = r;
                            tdata[idx + 1] = g;
                            tdata[idx + 2] = b;
                            tdata[idx + 3] = a;
                        }
                        tdata
                    }
                    ft::bitmap::PixelMode::Gray => {
                        // Convert from greyscale to RGBA8
                        let tdata: Vec<_> = buffer
                            .iter()
                            .flat_map(|coverage| {
                                let r = (255. * color_r) as u8;
                                let g = (255. * color_g) as u8;
                                let b = (255. * color_b) as u8;
                                let α = ((*coverage as f32) * color_a) as u8;
                                vec![r, g, b, α]
                            })
                            .collect();
                        tdata
                    }
                    _ => panic!("unsupport pixel mode: {:?}", pixel_mode)
                };

                let (x1, y1, x2, y2) = if face.has_fixed_sizes() {
                    // Downscale by height
                    let width = (bmp_width as f32 * font_size) / bmp_height as f32;
                    let height = font_size;

                    let x1 = current_x;
                    let y1 = current_y - height;

                    let x2 = current_x + width;
                    let y2 = current_y;

                    current_x += width;

                    (x1, y1, x2, y2)
                } else {
                    let (width, height) = (bmp_width as f32, bmp_height as f32);

                    let off_x = position.x_offset as f32 / 64.;
                    let off_y = position.y_offset as f32 / 64.;

                    let x1 = current_x + off_x + bearing_x;
                    let y1 = current_y - off_y - bearing_y;
                    let x2 = x1 + width as f32;
                    let y2 = y1 + height as f32;

                    let x_advance = position.x_advance as f32 / 64.;
                    let y_advance = position.y_advance as f32 / 64.;
                    current_x += x_advance;
                    current_y += y_advance;

                    (x1, y1, x2, y2)
                };

                let texture = self.ctx.new_texture_from_rgba8(bmp_width as u16, bmp_height as u16, &tdata);
                self.render_box_with_texture(x1, y1, x2, y2, COLOR_WHITE, texture);
                self.ctx.delete_texture(texture);

                if debug {
                    self.outline(x1, y1, x2, y2, COLOR_BLUE, 1.);
                }
            }
            if debug {
                self.hline(0., current_x, 0., COLOR_RED, 1.);
            }
        }

        Ok(())
    }

    fn render_glyph(&mut self, glyph_id: u32, font_size: f32, x: f32, y: f32) -> Result<()> {
        Ok(())
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {
        if self.last_draw_time.is_none() {
            return
        }

        // Only allow 20 ms, process as much as we can during that time
        let elapsed_since_draw = self.last_draw_time.unwrap().elapsed();
        // We're long overdue a redraw. Exit for now
        if elapsed_since_draw > Duration::from_millis(20) {
            return
        }
        // The next redraw must happen 20ms since its last one.
        // Calculate how much time is remaining until then.
        let allowed_time = Duration::from_millis(20) - elapsed_since_draw;
        let deadline = Instant::now() + allowed_time;

        loop {
            let Ok((event, node_id, arg_data, response_fn)) =
                self.method_recvr.recv_deadline(deadline)
            else {
                break
            };
            let res = match event {
                GraphicsMethodEvent::LoadTexture => self.method_load_texture(node_id, arg_data),
                GraphicsMethodEvent::DeleteTexture => self.method_delete_texture(node_id, arg_data),
                GraphicsMethodEvent::CreateChatView => self.method_create_chatview(node_id, arg_data),
                GraphicsMethodEvent::CreateEditBox => self.method_create_editbox(node_id, arg_data),
            };
            response_fn(res);
        }
    }

    // Only do drawing here. Apps might not call this when minimized.
    fn draw(&mut self) {
        self.last_draw_time = Some(Instant::now());

        // This will make the top left (0, 0) and the bottom right (1, 1)
        // Default is (-1, 1) -> (1, -1)
        let proj = glam::Mat4::from_translation(glam::Vec3::new(-1., 1., 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(2., -2., 1.));
        //let proj = glam::Mat4::IDENTITY;

        let scene_graph = self.scene_graph.lock().unwrap();

        // We need this because scene_graph must remain locked for the duration of the rendering
        let mut render_context = RenderContext {
            scene_graph,
            ctx: &mut self.ctx,
            pipeline: &self.pipeline,
            proj,
            textures: &self.textures,
            font_faces: &self.font_faces,
        };

        render_context.render_window();

        drop(render_context);
    }

    fn key_down_event(&mut self, keycode: KeyCode, modifiers: KeyMods, repeat: bool) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let win = scene_graph.lookup_node_mut("/window/input/keyboard").unwrap();

        let key = keycode.to_str();

        let mut data = vec![];
        modifiers.shift.encode(&mut data).unwrap();
        modifiers.ctrl.encode(&mut data).unwrap();
        modifiers.alt.encode(&mut data).unwrap();
        modifiers.logo.encode(&mut data).unwrap();
        repeat.encode(&mut data).unwrap();
        key.encode(&mut data).unwrap();
        win.trigger("key_down", data).unwrap();
    }
    fn key_up_event(&mut self, keycode: KeyCode, modifiers: KeyMods) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let win = scene_graph.lookup_node_mut("/window/input/keyboard").unwrap();

        let key = keycode.to_str();

        let mut data = vec![];
        modifiers.shift.encode(&mut data).unwrap();
        modifiers.ctrl.encode(&mut data).unwrap();
        modifiers.alt.encode(&mut data).unwrap();
        modifiers.logo.encode(&mut data).unwrap();
        key.encode(&mut data).unwrap();
        win.trigger("key_up", data).unwrap();
    }
    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mut data = vec![];
        x.encode(&mut data).unwrap();
        y.encode(&mut data).unwrap();
        let mouse = scene_graph.lookup_node_mut("/window/input/mouse").unwrap();
        mouse.trigger("move", data).unwrap();
    }
    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mut data = vec![];
        x.encode(&mut data).unwrap();
        y.encode(&mut data).unwrap();
        let mouse = scene_graph.lookup_node_mut("/window/input/mouse").unwrap();
        mouse.trigger("wheel", data).unwrap();
    }
    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mut data = vec![];
        button.to_u8().encode(&mut data).unwrap();
        x.encode(&mut data).unwrap();
        y.encode(&mut data).unwrap();
        let mouse = scene_graph.lookup_node_mut("/window/input/mouse").unwrap();
        mouse.trigger("button_down", data).unwrap();
    }
    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mut data = vec![];
        button.to_u8().encode(&mut data).unwrap();
        x.encode(&mut data).unwrap();
        y.encode(&mut data).unwrap();
        let mouse = scene_graph.lookup_node_mut("/window/input/mouse").unwrap();
        mouse.trigger("button_up", data).unwrap();
    }

    fn resize_event(&mut self, width: f32, height: f32) {
        let mut data = vec![];
        width.encode(&mut data).unwrap();
        height.encode(&mut data).unwrap();

        let mut scene_graph = self.scene_graph.lock().unwrap();
        let win = scene_graph.lookup_node_mut("/window").unwrap();
        let prop = win.get_property("screen_size").unwrap();
        prop.set_f32(0, width).unwrap();
        prop.set_f32(1, height).unwrap();
        win.trigger("resize", data).unwrap();
    }
}

pub fn run_gui(scene_graph: SceneGraphPtr) {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default().with_max_level(LevelFilter::Debug).with_tag("darkfi"),
        );
    }

    #[cfg(target_os = "linux")]
    {
        let term_logger = simplelog::TermLogger::new(
            simplelog::LevelFilter::Debug,
            simplelog::Config::default(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        );
        simplelog::CombinedLogger::init(vec![term_logger]).expect("logger");
    }

    let mut conf = miniquad::conf::Conf {
        high_dpi: true,
        window_resizable: true,
        platform: miniquad::conf::Platform {
            linux_backend: miniquad::conf::LinuxBackend::WaylandWithX11Fallback,
            wayland_use_fallback_decorations: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let metal = std::env::args().nth(1).as_deref() == Some("metal");
    conf.platform.apple_gfx_api =
        if metal { conf::AppleGfxApi::Metal } else { conf::AppleGfxApi::OpenGl };

    miniquad::start(conf, || Box::new(Stage::new(scene_graph)));
}
