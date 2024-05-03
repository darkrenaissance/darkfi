use fontdue::{Font, FontSettings, layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle, GlyphPosition}};
use darkfi_serial::{Decodable, Encodable, SerialEncodable};
use std::{fmt, io::Cursor};
use miniquad::*;

use crate::{error::{Error, Result},
scene::{
    SceneNode,
    PropertyType,
    SceneGraph,
    SceneGraphPtr,
    SceneNodeType,
    SceneNodeId,
}, shader};

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

#[derive(Debug, SerialEncodable)]
#[repr(C)]
struct Vertex {
    pos: [f32; 2],
    color: [f32; 4],
    uv: [f32; 2],
}

#[derive(SerialEncodable)]
#[repr(C)]
struct Face {
    idxs: [u32; 3],
}

impl fmt::Debug for Face {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.idxs)
    }
}

struct Stage {
    ctx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,

    scene_graph: SceneGraphPtr,

    white_texture: TextureId,
    font: Font,
}

impl Stage {
    pub fn new(scene_graph: SceneGraphPtr) -> Stage {
        let mut ctx: Box<dyn RenderingBackend> = window::new_rendering_backend();

        let white_texture = ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]);

        let mut shader_meta: ShaderMeta = shader::meta();
        shader_meta
            .uniforms
            .uniforms
            .push(UniformDesc::new("Projection", UniformType::Mat4));
        shader_meta
            .uniforms
            .uniforms
            .push(UniformDesc::new("Model", UniformType::Mat4));

        let shader = ctx
            .new_shader(
                match ctx.info().backend {
                    Backend::OpenGl => ShaderSource::Glsl {
                        vertex: shader::GL_VERTEX,
                        fragment: shader::GL_FRAGMENT,
                    },
                    Backend::Metal => ShaderSource::Msl {
                        program: shader::METAL,
                    },
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

        let font = {
            let mut scene_graph = scene_graph.lock().unwrap();

            let font = scene_graph.add_node("font", SceneNodeType::Fonts);
            let font_id = font.id;
            scene_graph.link(font_id, SceneGraph::ROOT_ID).unwrap();

            let inter_regular = scene_graph.add_node("inter-regular", SceneNodeType::Font);

            let font = include_bytes!("../Inter-Regular.ttf") as &[u8];
            let font = Font::from_bytes(font, FontSettings::default()).unwrap();
            let line_metrics = font.horizontal_line_metrics(1.).unwrap();
            inter_regular
                .add_property("new_line_size", PropertyType::Float32)
                .unwrap()
                .set_f32(line_metrics.new_line_size)
                .unwrap();

            inter_regular.add_method("create_text",
                vec![
                    ("text", PropertyType::Str),
                    ("font_size", PropertyType::Float32),
                ],
                vec![
                    ("node_id", PropertyType::SceneNodeId)
                ],
            );

            let inter_regular_id = inter_regular.id;
            scene_graph.link(inter_regular_id, font_id).unwrap();
            font
        };

        let mut stage = Stage {
            ctx,
            pipeline,
            scene_graph,
            white_texture,
            font,
        };
        stage.setup_scene_graph_window();
        stage
    }

    fn setup_scene_graph_window(&mut self) {
        let mut scene_graph = self.scene_graph.lock().unwrap();

        let window = scene_graph.add_node("window", SceneNodeType::Window);
        let (screen_width, screen_height) = window::screen_size();
        window
            .add_property("width", PropertyType::Float32)
            .unwrap()
            .set_f32(screen_width)
            .unwrap();
        window
            .add_property("height", PropertyType::Float32)
            .unwrap()
            .set_f32(screen_height)
            .unwrap();
        window.add_signal("resize").unwrap();
        window.add_method("load_texture",
            vec![
                ("node_name", PropertyType::Str),
                ("path", PropertyType::Str),
            ],
            vec![
                ("node_id", PropertyType::SceneNodeId)
            ],
        );
        let window_id = window.id;
        scene_graph.link(window_id, SceneGraph::ROOT_ID).unwrap();

        let input = scene_graph.add_node("input", SceneNodeType::WindowInput);
        let input_id = input.id;
        scene_graph.link(input_id, window_id).unwrap();

        let keyb = scene_graph.add_node("keyboard", SceneNodeType::Keyboard);
        keyb.add_property("shift", PropertyType::Bool).unwrap();
        keyb.add_property("ctrl", PropertyType::Bool).unwrap();
        keyb.add_property("alt", PropertyType::Bool).unwrap();
        keyb.add_property("logo", PropertyType::Bool).unwrap();
        keyb.add_property("repeat", PropertyType::Bool).unwrap();
        keyb.add_property("keycode", PropertyType::Str).unwrap();
        keyb.add_signal("key_down").unwrap();
        let keyb_id = keyb.id;
        scene_graph.link(keyb_id, input_id).unwrap();

        let mouse = scene_graph.add_node("mouse", SceneNodeType::Mouse);
        mouse.add_property("button", PropertyType::Uint32).unwrap();
        mouse.add_property("x", PropertyType::Float32).unwrap();
        mouse.add_property("y", PropertyType::Float32).unwrap();
        mouse.add_signal("button_up").unwrap();
        mouse.add_signal("button_down").unwrap();
        mouse.add_signal("wheel").unwrap();
        mouse.add_signal("move").unwrap();
        let mouse_id = mouse.id;
        scene_graph.link(mouse_id, input_id).unwrap();

        let layer1 = scene_graph.add_node("layer1", SceneNodeType::RenderLayer);
        let is_visible = layer1
            .add_property("is_visible", PropertyType::Bool)
            .unwrap();
        is_visible.set_bool(true).unwrap();
        layer1
            .add_property("rect_x", PropertyType::Uint32)
            .unwrap()
            .set_u32(0)
            .unwrap();
        layer1
            .add_property("rect_y", PropertyType::Uint32)
            .unwrap()
            .set_u32(0)
            .unwrap();
        layer1
            .add_property("rect_w", PropertyType::Uint32)
            .unwrap()
            .set_u32(0)
            .unwrap();
        layer1
            .add_property("rect_h", PropertyType::Uint32)
            .unwrap()
            .set_u32(0)
            .unwrap();
        let layer1_id = layer1.id;
        scene_graph.link(layer1_id, window_id).unwrap();

        let layer2 = scene_graph.add_node("layer2", SceneNodeType::RenderLayer);
        let is_visible = layer2
            .add_property("is_visible", PropertyType::Bool)
            .unwrap();
        is_visible.set_bool(true).unwrap();
        layer2
            .add_property("rect_x", PropertyType::Uint32)
            .unwrap()
            .set_u32(0)
            .unwrap();
        layer2
            .add_property("rect_y", PropertyType::Uint32)
            .unwrap()
            .set_u32(0)
            .unwrap();
        layer2
            .add_property("rect_w", PropertyType::Uint32)
            .unwrap()
            .set_u32(0)
            .unwrap();
        layer2
            .add_property("rect_h", PropertyType::Uint32)
            .unwrap()
            .set_u32(0)
            .unwrap();
        let layer2_id = layer2.id;
        scene_graph.link(layer2_id, window_id).unwrap();

        let funky_square = scene_graph.add_node("funky_square", SceneNodeType::RenderObject);
        funky_square
            .add_property("x", PropertyType::Float32)
            .unwrap()
            .set_f32(0.)
            .unwrap();
        funky_square
            .add_property("y", PropertyType::Float32)
            .unwrap()
            .set_f32(0.)
            .unwrap();
        funky_square
            .add_property("scale", PropertyType::Float32)
            .unwrap()
            .set_f32(0.)
            .unwrap();
        let funky_square_id = funky_square.id;
        scene_graph.link(funky_square_id, layer1_id).unwrap();

        let funky_mesh = scene_graph.add_node("mesh", SceneNodeType::RenderMesh);
        let verts = funky_mesh
            .add_property("verts", PropertyType::Buffer)
            .unwrap();
        let faces = funky_mesh
            .add_property("faces", PropertyType::Buffer)
            .unwrap();
        let mut buf = vec![];
        // top left
        Vertex {
            pos: [0., 0.],
            color: [1., 0., 1., 1.],
            uv: [0., 0.],
        }
        .encode(&mut buf)
        .unwrap();
        // top right
        Vertex {
            pos: [0.5, 0.],
            color: [1., 1., 0., 1.],
            uv: [1., 0.],
        }
        .encode(&mut buf)
        .unwrap();
        // bottom left
        Vertex {
            pos: [0., 0.5],
            color: [0., 0., 0.8, 1.],
            uv: [0., 1.],
        }
        .encode(&mut buf)
        .unwrap();
        // bottom right
        Vertex {
            pos: [0.5, 0.5],
            color: [1., 1., 0., 1.],
            uv: [1., 1.],
        }
        .encode(&mut buf)
        .unwrap();
        verts.set_buf(buf).unwrap();

        let mut buf = vec![];
        Face { idxs: [0, 2, 1] }.encode(&mut buf).unwrap();
        Face { idxs: [1, 2, 3] }.encode(&mut buf).unwrap();
        faces.set_buf(buf).unwrap();
        let funky_mesh_id = funky_mesh.id;
        scene_graph.link(funky_mesh_id, funky_square_id).unwrap();
    }

    fn draw_glyph(ctx: &mut Box<dyn RenderingBackend>, proj: &glam::Mat4, model: &glam::Mat4, font: &Font, glyph_pos: &GlyphPosition, color: [f32; 4]) {
        let (screen_width, screen_height) = window::screen_size();
        //let proj =
        //    glam::Mat4::from_translation(glam::Vec3::new(-1., 1., 0.)) *
        //    glam::Mat4::from_scale(glam::Vec3::new(2./screen_width, -2./screen_height, 1.));
        //let model = glam::Mat4::IDENTITY;
        let model = *model * glam::Mat4::from_scale(glam::Vec3::new(1./screen_width, 1./screen_height, 1.));

        let mut uniforms_data = [0u8; 128];
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(proj) };
        uniforms_data[0..64].copy_from_slice(&data);
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&model) };
        uniforms_data[64..].copy_from_slice(&data);
        assert_eq!(128, 2 * UniformType::Mat4.size());

        ctx
            .apply_uniforms_from_bytes(uniforms_data.as_ptr(), uniforms_data.len());

        let (font_metrics, text_bitmap) = font.rasterize(glyph_pos.parent, glyph_pos.key.px);
        let text_bitmap: Vec<_> = text_bitmap
            .iter()
            .flat_map(|coverage| vec![255, 255, 255, *coverage])
            .collect();

        let (x, y) = (glyph_pos.x, glyph_pos.y);
        let (w, h) = (glyph_pos.width as f32, glyph_pos.height as f32);
        //    0             1
        // (-1, 1) ----- (1, 1)
        //    |          /  |
        //    |        /    |
        //    |      /      |
        //    |    /        |
        // (-1, -1) ---- (1, -1)
        //    2             3
        //
        // faces: 021, 123
        let vertices: [Vertex; 4] = 
            [
                // top left
                Vertex {
                    pos: [x, y],
                    color,
                    uv: [0., 0.],
                },
                // top right
                Vertex {
                    pos: [x + w, y],
                    color,
                    uv: [1., 0.],
                },
                // bottom left
                Vertex {
                    pos: [x, y + h],
                    color,
                    uv: [0., 1.],
                },
                // bottom right
                Vertex {
                    pos: [x + w, y + h],
                    color,
                    uv: [1., 1.],
                },
            ];

        //debug!("screen size: {:?}", window::screen_size());
        let vertex_buffer = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&vertices),
        );

        let indices: [u16; 6] = [0, 2, 1, 1, 2, 3];
        let index_buffer = ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );

        let texture = 
            //self.king_texture;
            ctx.new_texture_from_rgba8(
                font_metrics.width as u16,
                font_metrics.height as u16,
                &text_bitmap,
            );

        let bindings = Bindings {
            vertex_buffers: vec![vertex_buffer],
            index_buffer: index_buffer,
            images: vec![texture],
        };

        ctx.apply_bindings(&bindings);
        ctx.draw(0, 6, 1);
    }

    fn create_text_node(&self, font_node_id: SceneNodeId, font_name: &str, node_name: String, text: String, font_size: f32) -> Result<SceneNodeId> {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let text_node = scene_graph.add_node(node_name, SceneNodeType::RenderText);
        text_node
            .add_property("text", PropertyType::Str)
            .unwrap()
            .set_str(text.clone())
            .unwrap();
        text_node
            .add_property("font_size", PropertyType::Float32)
            .unwrap()
            .set_f32(font_size)
            .unwrap();
        text_node
            .add_property("r", PropertyType::Float32)
            .unwrap()
            .set_f32(0.)
            .unwrap();
        text_node
            .add_property("g", PropertyType::Float32)
            .unwrap()
            .set_f32(0.)
            .unwrap();
        text_node
            .add_property("b", PropertyType::Float32)
            .unwrap()
            .set_f32(0.)
            .unwrap();
        text_node
            .add_property("a", PropertyType::Float32)
            .unwrap()
            .set_f32(0.)
            .unwrap();

        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            ..LayoutSettings::default()
        });
        let font = match font_name {
            "inter-regular" => {
                &self.font
            }
            _ => panic!("unknown font name!")
        };
        let fonts = [font];
        layout.append(&fonts, &TextStyle::new(&text, font_size, 0));

        text_node
            .add_property("height", PropertyType::Float32)
            .unwrap()
            .set_f32(layout.height())
            .unwrap();

        // Calculate the text width
        // std::cmp::max() not impl for f32
        let max_f32 = |x: f32, y: f32| {
            if x > y { x } else { y }
        };

        // TODO: this calc isn't multiline, we should add width property to each line
        let mut total_width = 0.;
        for glyph_pos in layout.glyphs() {
            let right = glyph_pos.x + glyph_pos.width as f32;
            total_width = max_f32(total_width, right);
        }

        text_node
            .add_property("width", PropertyType::Float32)
            .unwrap()
            .set_f32(total_width)
            .unwrap();

        let text_node_id = text_node.id;

        for (idx, line) in layout.lines().unwrap().into_iter().enumerate() {
            let line_node_name = format!("line.{}", idx);
            let line_node = scene_graph.add_node(line_node_name, SceneNodeType::LinePosition);
            line_node
                .add_property("idx", PropertyType::Uint32)
                .unwrap()
                .set_u32(idx as u32)
                .unwrap();
            line_node
                .add_property("baseline_y", PropertyType::Float32)
                .unwrap()
                .set_f32(line.baseline_y)
                .unwrap();
            line_node
                .add_property("padding", PropertyType::Float32)
                .unwrap()
                .set_f32(line.padding)
                .unwrap();
            line_node
                .add_property("max_ascent", PropertyType::Float32)
                .unwrap()
                .set_f32(line.max_ascent)
                .unwrap();
            line_node
                .add_property("min_descent", PropertyType::Float32)
                .unwrap()
                .set_f32(line.min_descent)
                .unwrap();
            line_node
                .add_property("max_line_gap", PropertyType::Float32)
                .unwrap()
                .set_f32(line.max_line_gap)
                .unwrap();
            line_node
                .add_property("glyph_start", PropertyType::Uint32)
                .unwrap()
                .set_u32(line.glyph_start as u32)
                .unwrap();
            line_node
                .add_property("glyph_end", PropertyType::Uint32)
                .unwrap()
                .set_u32(line.glyph_end as u32)
                .unwrap();

            let line_node_id = line_node.id;
            scene_graph.link(line_node_id, text_node_id)?;
        }

        scene_graph.link(font_node_id, text_node_id)?;
        Ok(text_node_id)
    }

    fn load_texture(&self, node_name: String, filepath: String) -> Result<SceneNodeId> {
        let Ok(img) = image::open(filepath) else {
            return Err(Error::FileNotFound)
        };

        let img = img.to_rgba8();
        let width = img.width();
        let height = img.height();
        let bmp = img.into_raw();

        let mut scene_graph = self.scene_graph.lock().unwrap();
        let img_node = scene_graph.add_node(node_name, SceneNodeType::RenderTexture);
        img_node
            .add_property("width", PropertyType::Uint32)
            .unwrap()
            .set_u32(width)
            .unwrap();
        img_node
            .add_property("height", PropertyType::Uint32)
            .unwrap()
            .set_u32(height)
            .unwrap();
        img_node
            .add_property("bmp", PropertyType::Buffer)
            .unwrap()
            .set_buf(bmp)
            .unwrap();
        //let king_texture = ctx.new_texture_from_rgba8(width, height, &king_bitmap);
        Ok(img_node.id)
    }
}

fn get_obj_props(obj: &SceneNode) -> Result<(f32, f32, f32)> {
    let x = obj.get_property("x").ok_or(Error::PropertyNotFound)?.get_f32()?;
    let y = obj.get_property("y").ok_or(Error::PropertyNotFound)?.get_f32()?;
    let scale = obj.get_property("scale").ok_or(Error::PropertyNotFound)?.get_f32()?;
    Ok((x, y, scale))
}

fn get_text_props(render_text: &SceneNode) -> Result<(String, f32, [f32; 4])> {
    let text = render_text.get_property("text").ok_or(Error::PropertyNotFound)?.get_str()?;
    let font_size = render_text.get_property("font_size").ok_or(Error::PropertyNotFound)?.get_f32()?;
    let r = render_text.get_property("r").ok_or(Error::PropertyNotFound)?.get_f32()?;
    let g = render_text.get_property("g").ok_or(Error::PropertyNotFound)?.get_f32()?;
    let b = render_text.get_property("b").ok_or(Error::PropertyNotFound)?.get_f32()?;
    let a = render_text.get_property("a").ok_or(Error::PropertyNotFound)?.get_f32()?;
    let color = [r, g, b, a];
    Ok((text, font_size, color))
}

impl EventHandler for Stage {
    fn update(&mut self) {
        // check /font:create_text() queue

        let mut scene_graph = self.scene_graph.lock().unwrap();
        let font_root = 
            scene_graph
            .lookup_node("/font")
            .unwrap();
        let font_ids: Vec<_> = font_root.iter_children(&scene_graph, SceneNodeType::Font).map(|node| node.id).collect();

        let mut calls = vec![];
        for font_id in font_ids
        {
            let font_node = scene_graph.get_node_mut(font_id).unwrap();
            for method in &mut font_node.methods {
                for (arg_data, response_fn) in std::mem::take(&mut method.queue) {
                    calls.push((font_node.id, font_node.name.clone(), method.name.clone(), arg_data, response_fn));
                }
            }
        }
        drop(scene_graph);

        for (font_node_id, font_node_name, method_name, arg_data, response_fn) in calls {
            assert_eq!(method_name, "create_text");

            let mut cur = Cursor::new(&arg_data);
            let mut reply = vec![];
            let node_name = String::decode(&mut cur).unwrap();
            let text = String::decode(&mut cur).unwrap();
            let font_size = f32::decode(&mut cur).unwrap();
            debug!(target: "win", "/font:{}({}, {})", method_name, text, font_size);

            let node_id_result = self.create_text_node(font_node_id, &font_node_name, node_name, text, font_size);
            let node_id_result = node_id_result.map(|node_id| {
                node_id.encode(&mut reply).unwrap();
                reply
            });
            response_fn(node_id_result)
        }

        // check /window:load_texture() queue

        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mut calls = vec![];
        let window = 
            scene_graph
            .lookup_node_mut("/window")
            .unwrap();
        for method in &mut window.methods {
            for (arg_data, response_fn) in std::mem::take(&mut method.queue) {
                calls.push((method.name.clone(), arg_data, response_fn));
            }
        }
        drop(scene_graph);

        for (method_name, arg_data, response_fn) in calls {
            assert_eq!(method_name, "load_texture");

            let mut cur = Cursor::new(&arg_data);
            let mut reply = vec![];
            let node_name = String::decode(&mut cur).unwrap();
            let filepath = String::decode(&mut cur).unwrap();
            debug!(target: "win", "/window:{}({}, {})", method_name, node_name, filepath);

            let node_id_result = self.load_texture(node_name, filepath);
            let node_id_result = node_id_result.map(|node_id| {
                node_id.encode(&mut reply).unwrap();
                reply
            });
            response_fn(node_id_result)
        }
    }

    // Only do drawing here. Apps might not call this when minimized.
    fn draw(&mut self) {
        let clear = PassAction::clear_color(0., 0., 0., 1.);
        self.ctx.begin_default_pass(clear);
        self.ctx.end_render_pass();

        // This will make the top left (0, 0) and the bottom right (1, 1)
        // Default is (-1, 1) -> (1, -1)
        let proj =
            glam::Mat4::from_translation(glam::Vec3::new(-1., 1., 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(2., -2., 1.));

        // Reusable text layout
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);

        let scene_graph = self.scene_graph.lock().unwrap();

        for layer in 
            scene_graph
            .lookup_node("/window")
            .expect("no window attached!")
            .iter_children(&scene_graph, SceneNodeType::RenderLayer)
        {
            let is_visible = layer
                .get_property("is_visible")
                .unwrap()
                .get_bool()
                .unwrap();
            if !is_visible {
                continue;
            }

            //self.ctx.begin_default_pass(Default::default());
            self.ctx.begin_default_pass(PassAction::Nothing);
            self.ctx.apply_pipeline(&self.pipeline);

            let rect_x = layer
                .get_property("rect_x")
                .unwrap()
                .get_u32()
                .unwrap();
            let rect_y = layer
                .get_property("rect_y")
                .unwrap()
                .get_u32()
                .unwrap();
            let rect_w = layer
                .get_property("rect_w")
                .unwrap()
                .get_u32()
                .unwrap();
            let rect_h = layer
                .get_property("rect_h")
                .unwrap()
                .get_u32()
                .unwrap();

            self.ctx
                .apply_viewport(rect_x as i32, rect_y as i32, rect_w as i32, rect_h as i32);
            self.ctx
                .apply_scissor_rect(rect_x as i32, rect_y as i32, rect_w as i32, rect_h as i32);

            'outer: for obj in layer.iter_children(&scene_graph, SceneNodeType::RenderObject) {
                let Ok((x, y, scale)) = get_obj_props(obj) else {
                    error!("obj '{}':{} has a property error", obj.name, obj.id);
                    continue
                };

                let model =
                    glam::Mat4::from_translation(glam::Vec3::new(x, y, 0.)) *
                    glam::Mat4::from_scale(glam::Vec3::new(scale, scale, 1.));

                let texture_id = 'texture: {
                    let Some(texture_node) = obj.iter_children(&scene_graph, SceneNodeType::RenderTexture).next() else {
                        break 'texture self.white_texture
                    };

                    let Some(width_prop) = texture_node.get_property("width") else {
                        error!("texture '{}':{} missing property width", texture_node.name, texture_node.id);
                        continue 'outer
                    };
                    let Ok(width) = width_prop.get_u32() else {
                        error!("texture '{}':{} width property has wrong type", texture_node.name, texture_node.id);
                        continue 'outer
                    };

                    let Some(height_prop) = texture_node.get_property("height") else {
                        error!("texture '{}':{} missing property height", texture_node.name, texture_node.id);
                        continue 'outer
                    };
                    let Ok(height) = height_prop.get_u32() else {
                        error!("texture '{}':{} height property has wrong type", texture_node.name, texture_node.id);
                        continue 'outer
                    };

                    let Some(bmp_prop) = texture_node.get_property("bmp") else {
                        error!("texture '{}':{} missing property bmp", texture_node.name, texture_node.id);
                        continue 'outer
                    };
                    let Ok(bmp) = bmp_prop.get_buf() else {
                        error!("texture '{}':{} bmp property has wrong type", texture_node.name, texture_node.id);
                        continue 'outer
                    };

                    let texture_id = self.ctx.new_texture_from_rgba8(width as u16, height as u16, &bmp);
                    texture_id
                };

                for mesh in obj.iter_children(&scene_graph, SceneNodeType::RenderMesh) {
                    let Some(verts_prop) = mesh.get_property("verts") else {
                        error!("mesh '{}':{} missing property verts", mesh.name, mesh.id);
                        continue
                    };
                    let Ok(verts) = verts_prop.get_buf() else {
                        error!("mesh '{}':{} verts property has wrong type", mesh.name, mesh.id);
                        continue
                    };

                    let Some(faces_prop) = mesh.get_property("faces") else {
                        error!("mesh '{}':{} missing property faces", mesh.name, mesh.id);
                        continue
                    };
                    let Ok(faces) = faces_prop.get_buf() else {
                        error!("mesh '{}':{} faces property has wrong type", mesh.name, mesh.id);
                        continue
                    };

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

                    let index_buffer = self.ctx.new_buffer(
                        BufferType::IndexBuffer,
                        BufferUsage::Immutable,
                        bufsrc,
                    );

                    let bindings = Bindings {
                        vertex_buffers: vec![vertex_buffer],
                        index_buffer: index_buffer,
                        images: vec![texture_id],
                    };

                    self.ctx.apply_bindings(&bindings);

                    let mut uniforms_data = [0u8; 128];
                    let data: [u8; 64] = unsafe { std::mem::transmute_copy(&proj) };
                    uniforms_data[0..64].copy_from_slice(&data);
                    let data: [u8; 64] = unsafe { std::mem::transmute_copy(&model) };
                    uniforms_data[64..].copy_from_slice(&data);
                    assert_eq!(128, 2 * UniformType::Mat4.size());

                    self.ctx
                        .apply_uniforms_from_bytes(uniforms_data.as_ptr(), uniforms_data.len());

                    self.ctx.draw(0, 3 * faces.len() as i32, 1);
                }

                for render_text in obj.iter_children(&scene_graph, SceneNodeType::RenderText) {
                    let Ok((text, font_size, color)) = get_text_props(render_text) else {
                        error!("text '{}':{} has a property error", render_text.name, render_text.id);
                        continue
                    };

                    let Some(font_node) = render_text.iter_children(&scene_graph, SceneNodeType::Font).next() else {
                        error!("text '{}':{} missing a font node", render_text.name, render_text.id);
                        continue
                    };
                    // No other fonts supported yet
                    assert_eq!(font_node.name, "inter-regular");

                    layout.reset(&LayoutSettings {
                        ..LayoutSettings::default()
                    });
                    let fonts = [&self.font];
                    layout.append(&fonts, &TextStyle::new(&text, font_size, 0));

                    for glyph_pos in layout.glyphs() {
                        Self::draw_glyph(&mut self.ctx, &proj, &model, &self.font, glyph_pos, color);
                    }
                }
            }

            self.ctx.end_render_pass();
        }
        self.ctx.commit_frame();
    }

    fn key_down_event(&mut self, keycode: KeyCode, modifiers: KeyMods, repeat: bool) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let win = 
            scene_graph
            .lookup_node_mut("/window/input/keyboard")
            .unwrap();

        win.get_property("shift")
            .unwrap()
            .set_bool(modifiers.shift)
            .unwrap();
        win.get_property("ctrl")
            .unwrap()
            .set_bool(modifiers.ctrl)
            .unwrap();
        win.get_property("alt")
            .unwrap()
            .set_bool(modifiers.alt)
            .unwrap();
        win.get_property("logo")
            .unwrap()
            .set_bool(modifiers.logo)
            .unwrap();
        win.get_property("repeat")
            .unwrap()
            .set_bool(repeat)
            .unwrap();

        let send_key_down = |key: &str| {
            win.get_property("keycode").unwrap().set_str(key).unwrap();
            win.trigger("key_down").unwrap();
        };

        match keycode {
            KeyCode::Space => send_key_down("Space"),
            KeyCode::Apostrophe => send_key_down("Apostrophe"),
            KeyCode::Comma => send_key_down("Comma"),
            KeyCode::Minus => send_key_down("Minus"),
            KeyCode::Period => send_key_down("Period"),
            KeyCode::Slash => send_key_down("Slash"),
            KeyCode::Key0 => send_key_down("Key0"),
            KeyCode::Key1 => send_key_down("Key1"),
            KeyCode::Key2 => send_key_down("Key2"),
            KeyCode::Key3 => send_key_down("Key3"),
            KeyCode::Key4 => send_key_down("Key4"),
            KeyCode::Key5 => send_key_down("Key5"),
            KeyCode::Key6 => send_key_down("Key6"),
            KeyCode::Key7 => send_key_down("Key7"),
            KeyCode::Key8 => send_key_down("Key8"),
            KeyCode::Key9 => send_key_down("Key9"),
            KeyCode::Semicolon => send_key_down("Semicolon"),
            KeyCode::Equal => send_key_down("Equal"),
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
            KeyCode::LeftBracket => send_key_down("LeftBracket"),
            KeyCode::Backslash => send_key_down("Backslash"),
            KeyCode::RightBracket => send_key_down("RightBracket"),
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
        let mouse = 
            scene_graph
            .lookup_node_mut("/window/input/mouse")
            .unwrap();
        mouse.get_property("x").unwrap().set_f32(x).unwrap();
        mouse.get_property("y").unwrap().set_f32(y).unwrap();
        mouse.trigger("move").unwrap();
    }
    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mouse = 
            scene_graph
            .lookup_node_mut("/window/input/mouse")
            .unwrap();
        mouse.get_property("x").unwrap().set_f32(x).unwrap();
        mouse.get_property("y").unwrap().set_f32(y).unwrap();
        mouse.trigger("wheel").unwrap();
    }
    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mouse = 
            scene_graph
            .lookup_node_mut("/window/input/mouse")
            .unwrap();
        mouse
            .get_property("button")
            .unwrap()
            .set_u32(button.to_u8() as u32)
            .unwrap();
        mouse.get_property("x").unwrap().set_f32(x).unwrap();
        mouse.get_property("y").unwrap().set_f32(y).unwrap();
        mouse.trigger("button_down").unwrap();
    }
    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mouse = 
            scene_graph
            .lookup_node_mut("/window/input/mouse")
            .unwrap();
        mouse
            .get_property("button")
            .unwrap()
            .set_u32(button.to_u8() as u32)
            .unwrap();
        mouse.get_property("x").unwrap().set_f32(x).unwrap();
        mouse.get_property("y").unwrap().set_f32(y).unwrap();
        mouse.trigger("button_up").unwrap();
    }

    fn resize_event(&mut self, width: f32, height: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let win = scene_graph.lookup_node_mut("/window").unwrap();
        win.get_property("width").unwrap().set_f32(width).unwrap();
        win.get_property("height").unwrap().set_f32(height).unwrap();
        win.trigger("resize").unwrap();
    }
}

pub fn init_gui(scene_graph: SceneGraphPtr) {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(LevelFilter::Debug)
                .with_tag("fagman"),
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
    conf.platform.apple_gfx_api = if metal {
        conf::AppleGfxApi::Metal
    } else {
        conf::AppleGfxApi::OpenGl
    };

    miniquad::start(conf, || Box::new(Stage::new(scene_graph)));
}

