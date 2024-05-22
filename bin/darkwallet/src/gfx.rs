use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use freetype as ft;
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
    expr::{SExprMachine, SExprVal},
    prop::{Property, PropertySubType, PropertyType},
    res::{ResourceId, ResourceManager},
    scene::{
        MethodResponseFn, SceneGraph, SceneGraphPtr, SceneNode, SceneNodeId, SceneNodeInfo,
        SceneNodeType,
    },
    shader,
};

trait MouseButtonAsU8 {
    fn to_u8(&self) -> u8;
}

impl MouseButtonAsU8 for MouseButton {
    fn to_u8(&self) -> u8 {
        match self {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::Unknown => 3,
        }
    }
}

type Color = [f32; 4];

const COLOR_RED: Color = [1., 0., 0., 1.];
const COLOR_BLUE: Color = [0., 0., 1., 1.];
const COLOR_WHITE: Color = [1., 1., 1., 1.];

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

#[derive(Debug)]
struct Rectangle<T> {
    x: T,
    y: T,
    w: T,
    h: T,
}

impl<T> Rectangle<T> {
    fn from_array(arr: [T; 4]) -> Self {
        let mut iter = IntoIter::new(arr);
        Self {
            x: iter.next().unwrap(),
            y: iter.next().unwrap(),
            w: iter.next().unwrap(),
            h: iter.next().unwrap(),
        }
    }
}

#[derive(Debug)]
enum GraphicsMethodEvent {
    LoadTexture,
    DeleteTexture,
}

struct Stage<'a> {
    ctx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,

    scene_graph: SceneGraphPtr,

    textures: ResourceManager<TextureId>,
    ft_face: ft::Face<&'a [u8]>,

    method_recvr: mpsc::Receiver<(GraphicsMethodEvent, SceneNodeId, Vec<u8>, MethodResponseFn)>,
    method_sender: mpsc::SyncSender<(GraphicsMethodEvent, SceneNodeId, Vec<u8>, MethodResponseFn)>,

    last_draw_time: Option<Instant>,
}

impl<'a> Stage<'a> {
    const WHITE_TEXTURE_ID: ResourceId = 0;

    pub fn new(scene_graph: SceneGraphPtr) -> Stage<'a> {
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

        let font_data = include_bytes!("../Inter-Regular.otf") as &[u8];
        //let font_data = include_bytes!("../NotoColorEmoji.ttf") as &[u8];
        let ftlib = ft::Library::init().unwrap();
        let ft_face = ftlib.new_memory_face2(font_data, 0).unwrap();

        let mut stage = Stage {
            ctx,
            pipeline,
            scene_graph,
            textures,
            ft_face,
            method_recvr,
            method_sender,
            last_draw_time: None,
        };
        stage.setup_scene_graph_window();
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
        let sender = self.method_sender.clone();
        let window_id = window.id;
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
}

struct RenderContext<'a> {
    scene_graph: MutexGuard<'a, SceneGraph>,
    ctx: &'a mut Box<dyn RenderingBackend>,
    pipeline: &'a Pipeline,
    proj: glam::Mat4,
    textures: &'a ResourceManager<TextureId>,
    ft_face: &'a ft::Face<&'a [u8]>,
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

        let mut rect = Self::get_rect(&layer)?;
        rect.y = screen_height as i32 - (rect.y + rect.h);

        self.ctx.apply_viewport(rect.x, rect.y, rect.w, rect.h);
        self.ctx.apply_scissor_rect(rect.x, rect.y, rect.w, rect.h);

        let layer_children =
            layer.get_children(&[SceneNodeType::RenderMesh, SceneNodeType::RenderText]);
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
                let z_index = node.get_property_u32("z_index").ok()?;
                Some((z_index, node_inf))
            })
            .collect();
        nodes.sort_unstable_by_key(|(z_index, node_inf)| *z_index);
        nodes.into_iter().map(|(z_index, node_inf)| node_inf).collect()
    }

    fn get_dim(mesh: &SceneNode, layer_rect: &Rectangle<i32>) -> Result<Rectangle<f32>> {
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

    fn render_mesh(&mut self, node_id: SceneNodeId, layer_rect: &Rectangle<i32>) -> Result<()> {
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

    fn render_box_with_texture(
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

        //debug!("screen size: {:?}", window::screen_size());
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

    fn render_box(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: Color) {
        let texture = self.textures.get(Stage::WHITE_TEXTURE_ID).unwrap();
        self.render_box_with_texture(x1, y1, x2, y2, color, *texture)
    }

    fn hline(&mut self, min_x: f32, max_x: f32, y: f32, color: Color, w: f32) {
        self.render_box(min_x, y - w / 2., max_x, y + w / 2., color)
    }

    fn vline(&mut self, x: f32, min_y: f32, max_y: f32, color: Color, w: f32) {
        self.render_box(x - w / 2., min_y, x + w / 2., max_y, color)
    }

    fn outline(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: Color, w: f32) {
        // top
        self.render_box(x1, y1, x2, y1 + w, color);
        // left
        self.render_box(x1, y1, x1 + w, y2, color);
        // right
        self.render_box(x2 - w, y1, x2, y2, color);
        // bottom
        self.render_box(x1, y2 - w, x2, y2, color);
    }

    fn render_text(&mut self, mesh_id: SceneNodeId, layer_rect: &Rectangle<i32>) -> Result<()> {
        let node = self.scene_graph.get_node(mesh_id).unwrap();

        let z_index = node.get_property_u32("z_index")?;
        let text = node.get_property_str("text")?;
        let overflow = node.get_property_enum("overflow")?;
        let font_size = node.get_property_f32("font_size")?;
        let debug = node.get_property_bool("debug")?;
        let rect = Self::get_dim(node, layer_rect)?;

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

        self.ft_face.set_char_size(font_size as isize * 64, 0, 72, 72).unwrap();
        // emojis required a fixed size
        //self.ft_face.set_char_size(109 * 64, 0, 72, 72).unwrap();

        let hb_font = harfbuzz_rs::Font::from_freetype_face(self.ft_face.clone());
        let buffer = harfbuzz_rs::UnicodeBuffer::new().add_str(&text);
        let output = harfbuzz_rs::shape(&hb_font, buffer, &[]);

        let positions = output.get_glyph_positions();
        let infos = output.get_glyph_infos();

        let mut current_x = 0.;
        let mut current_y = 0.;
        for (position, info) in positions.iter().zip(infos) {
            let gid = info.codepoint;

            self.ft_face.load_glyph(gid, ft::face::LoadFlag::COLOR).unwrap();

            let glyph = self.ft_face.glyph();
            glyph.render_glyph(ft::RenderMode::Normal).unwrap();

            // https://gist.github.com/jokertarot/7583938?permalink_comment_id=3327566#gistcomment-3327566

            let bmp = glyph.bitmap();
            let buffer = bmp.buffer();
            let width = bmp.width();
            let height = bmp.rows();
            let bearing_x = glyph.bitmap_left() as f32;
            let bearing_y = glyph.bitmap_top() as f32;

            //assert_eq!(bmp.pixel_mode().unwrap(), ft::bitmap::PixelMode::Bgra);
            //assert_eq!(bmp.pixel_mode().unwrap(), ft::bitmap::PixelMode::Lcd);
            //assert_eq!(bmp.pixel_mode().unwrap(), ft::bitmap::PixelMode::Gray);

            let pixel_mode = bmp.pixel_mode().unwrap();
            let tdata = match pixel_mode {
                ft::bitmap::PixelMode::Bgra => {
                    let mut tdata = vec![];
                    tdata.resize((4 * width * height) as usize, 0);
                    // Convert from BGRA to RGBA
                    for i in 0..(width*height) as usize {
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

            //println!("(s0, t0) = ({}, {}),  (s1, t1) = ({}, {})", s0, t0, s1, t1);
            //println!("(xa, ya) = ({}, {}),  (xo, yo) = ({}, {})", x_advance, y_advance, off_x, off_y);
            //println!("(bx, by) = ({}, {})", bearing_x, bearing_y);
            //println!("(w, h)   = ({}, {})", width, height);
            //println!("(x1, y1) = ({}, {}),  (x2, y2) = ({}, {})", x1, y1, x2, y2);
            //println!();

            let texture = self.ctx.new_texture_from_rgba8(width as u16, height as u16, &tdata);
            self.render_box_with_texture(x1, y1, x2, y2, COLOR_WHITE, texture);
            self.ctx.delete_texture(texture);

            if debug {
                self.outline(x1, y1, x2, y2, COLOR_BLUE, 1.);
            }
        }
        if debug {
            self.hline(0., current_x, 0., COLOR_RED, 1.);
        }

        Ok(())
    }

    fn render_glyph(&mut self, glyph_id: u32, font_size: f32, x: f32, y: f32) -> Result<()> {
        Ok(())
    }
}

impl<'a> EventHandler for Stage<'a> {
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
            };
            response_fn(res);
        }
    }

    // Only do drawing here. Apps might not call this when minimized.
    fn draw(&mut self) {
        self.last_draw_time = Some(Instant::now());

        let (screen_width, screen_height) = window::screen_size();
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
            ft_face: &self.ft_face,
        };

        render_context.render_window();

        drop(render_context);
    }

    fn key_down_event(&mut self, keycode: KeyCode, modifiers: KeyMods, repeat: bool) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let win = scene_graph.lookup_node_mut("/window/input/keyboard").unwrap();

        let send_key_down = |key: &str| {
            let mut data = vec![];
            modifiers.shift.encode(&mut data).unwrap();
            modifiers.ctrl.encode(&mut data).unwrap();
            modifiers.alt.encode(&mut data).unwrap();
            modifiers.logo.encode(&mut data).unwrap();
            repeat.encode(&mut data).unwrap();
            key.encode(&mut data).unwrap();
            win.trigger("key_down", data).unwrap();
        };

        match keycode {
            KeyCode::Space => send_key_down(" "),
            KeyCode::Apostrophe => send_key_down("'"),
            KeyCode::Comma => send_key_down(","),
            KeyCode::Minus => send_key_down("-"),
            KeyCode::Period => send_key_down("."),
            KeyCode::Slash => send_key_down("/"),
            KeyCode::Key0 => send_key_down("0"),
            KeyCode::Key1 => send_key_down("1"),
            KeyCode::Key2 => send_key_down("2"),
            KeyCode::Key3 => send_key_down("3"),
            KeyCode::Key4 => send_key_down("4"),
            KeyCode::Key5 => send_key_down("5"),
            KeyCode::Key6 => send_key_down("6"),
            KeyCode::Key7 => send_key_down("7"),
            KeyCode::Key8 => send_key_down("8"),
            KeyCode::Key9 => send_key_down("9"),
            KeyCode::Semicolon => send_key_down(":"),
            KeyCode::Equal => send_key_down("="),
            KeyCode::A => send_key_down("A"),
            KeyCode::B => send_key_down("B"),
            KeyCode::C => send_key_down("C"),
            KeyCode::D => send_key_down("D"),
            KeyCode::E => send_key_down("E"),
            KeyCode::F => send_key_down("F"),
            KeyCode::G => send_key_down("G"),
            KeyCode::H => send_key_down("H"),
            KeyCode::I => send_key_down("I"),
            KeyCode::J => send_key_down("J"),
            KeyCode::K => send_key_down("K"),
            KeyCode::L => send_key_down("L"),
            KeyCode::M => send_key_down("M"),
            KeyCode::N => send_key_down("N"),
            KeyCode::O => send_key_down("O"),
            KeyCode::P => send_key_down("P"),
            KeyCode::Q => send_key_down("Q"),
            KeyCode::R => send_key_down("R"),
            KeyCode::S => send_key_down("S"),
            KeyCode::T => send_key_down("T"),
            KeyCode::U => send_key_down("U"),
            KeyCode::V => send_key_down("V"),
            KeyCode::W => send_key_down("W"),
            KeyCode::X => send_key_down("X"),
            KeyCode::Y => send_key_down("Y"),
            KeyCode::Z => send_key_down("Z"),
            KeyCode::LeftBracket => send_key_down("("),
            KeyCode::Backslash => send_key_down("\\"),
            KeyCode::RightBracket => send_key_down(")"),
            KeyCode::GraveAccent => send_key_down("GraveAccent"),
            KeyCode::World1 => send_key_down("World1"),
            KeyCode::World2 => send_key_down("World2"),
            KeyCode::Escape => send_key_down("Escape"),
            KeyCode::Enter => send_key_down("Enter"),
            KeyCode::Tab => send_key_down("Tab"),
            KeyCode::Backspace => send_key_down("Backspace"),
            KeyCode::Insert => send_key_down("Insert"),
            KeyCode::Delete => send_key_down("Delete"),
            KeyCode::Right => send_key_down("Right"),
            KeyCode::Left => send_key_down("Left"),
            KeyCode::Down => send_key_down("Down"),
            KeyCode::Up => send_key_down("Up"),
            KeyCode::PageUp => send_key_down("PageUp"),
            KeyCode::PageDown => send_key_down("PageDown"),
            KeyCode::Home => send_key_down("Home"),
            KeyCode::End => send_key_down("End"),
            KeyCode::CapsLock => send_key_down("CapsLock"),
            KeyCode::ScrollLock => send_key_down("ScrollLock"),
            KeyCode::NumLock => send_key_down("NumLock"),
            KeyCode::PrintScreen => send_key_down("PrintScreen"),
            KeyCode::Pause => send_key_down("Pause"),
            KeyCode::F1 => send_key_down("F1"),
            KeyCode::F2 => send_key_down("F2"),
            KeyCode::F3 => send_key_down("F3"),
            KeyCode::F4 => send_key_down("F4"),
            KeyCode::F5 => send_key_down("F5"),
            KeyCode::F6 => send_key_down("F6"),
            KeyCode::F7 => send_key_down("F7"),
            KeyCode::F8 => send_key_down("F8"),
            KeyCode::F9 => send_key_down("F9"),
            KeyCode::F10 => send_key_down("F10"),
            KeyCode::F11 => send_key_down("F11"),
            KeyCode::F12 => send_key_down("F12"),
            KeyCode::F13 => send_key_down("F13"),
            KeyCode::F14 => send_key_down("F14"),
            KeyCode::F15 => send_key_down("F15"),
            KeyCode::F16 => send_key_down("F16"),
            KeyCode::F17 => send_key_down("F17"),
            KeyCode::F18 => send_key_down("F18"),
            KeyCode::F19 => send_key_down("F19"),
            KeyCode::F20 => send_key_down("F20"),
            KeyCode::F21 => send_key_down("F21"),
            KeyCode::F22 => send_key_down("F22"),
            KeyCode::F23 => send_key_down("F23"),
            KeyCode::F24 => send_key_down("F24"),
            KeyCode::F25 => send_key_down("F25"),
            KeyCode::Kp0 => send_key_down("Kp0"),
            KeyCode::Kp1 => send_key_down("Kp1"),
            KeyCode::Kp2 => send_key_down("Kp2"),
            KeyCode::Kp3 => send_key_down("Kp3"),
            KeyCode::Kp4 => send_key_down("Kp4"),
            KeyCode::Kp5 => send_key_down("Kp5"),
            KeyCode::Kp6 => send_key_down("Kp6"),
            KeyCode::Kp7 => send_key_down("Kp7"),
            KeyCode::Kp8 => send_key_down("Kp8"),
            KeyCode::Kp9 => send_key_down("Kp9"),
            KeyCode::KpDecimal => send_key_down("KpDecimal"),
            KeyCode::KpDivide => send_key_down("KpDivide"),
            KeyCode::KpMultiply => send_key_down("KpMultiply"),
            KeyCode::KpSubtract => send_key_down("KpSubtract"),
            KeyCode::KpAdd => send_key_down("KpAdd"),
            KeyCode::KpEnter => send_key_down("KpEnter"),
            KeyCode::KpEqual => send_key_down("KpEqual"),
            KeyCode::LeftShift => send_key_down("LeftShift"),
            KeyCode::LeftControl => send_key_down("LeftControl"),
            KeyCode::LeftAlt => send_key_down("LeftAlt"),
            KeyCode::LeftSuper => send_key_down("LeftSuper"),
            KeyCode::RightShift => send_key_down("RightShift"),
            KeyCode::RightControl => send_key_down("RightControl"),
            KeyCode::RightAlt => send_key_down("RightAlt"),
            KeyCode::RightSuper => send_key_down("RightSuper"),
            KeyCode::Menu => send_key_down("Menu"),
            KeyCode::Unknown => send_key_down("Unknown"),
        }
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
            android_logger::Config::default().with_max_level(LevelFilter::Debug).with_tag("fagman"),
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
