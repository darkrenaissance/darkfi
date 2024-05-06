use darkfi_serial::{Decodable, Encodable, SerialEncodable};
use fontdue::{
    layout::{CoordinateSystem, GlyphPosition, Layout, LayoutSettings, TextStyle},
    Font, FontSettings,
};
use miniquad::*;
use std::{fmt, io::Cursor};

use crate::{
    error::{Error, Result},
    prop::{Property, PropertySubType, PropertyType},
    scene::{SceneGraph, SceneGraphPtr, SceneNode, SceneNodeId, SceneNodeType},
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

type ResourceId = u32;

struct ResourceManager<T> {
    resources: Vec<(ResourceId, Option<T>)>,
    freed: Vec<usize>,
    id_counter: ResourceId,
}

impl<T> ResourceManager<T> {
    fn new() -> Self {
        Self { resources: vec![], freed: vec![], id_counter: 0 }
    }

    fn alloc(&mut self, rsrc: T) -> ResourceId {
        let id = self.id_counter;
        self.id_counter += 1;

        if self.freed.is_empty() {
            let idx = self.resources.len();
            self.resources.push((id, Some(rsrc)));
        } else {
            let idx = self.freed.pop().unwrap();
            let _ = std::mem::replace(&mut self.resources[idx], (id, Some(rsrc)));
        }
        id
    }

    fn get(&self, id: ResourceId) -> Option<&T> {
        for (idx, (rsrc_id, rsrc)) in self.resources.iter().enumerate() {
            if self.freed.contains(&idx) {
                continue
            }
            if *rsrc_id == id {
                return rsrc.as_ref()
            }
        }
        None
    }
}

struct Stage {
    ctx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,

    scene_graph: SceneGraphPtr,

    textures: ResourceManager<TextureId>,
    font: Font,
}

impl Stage {
    const WHITE_TEXTURE_ID: ResourceId = 0;

    pub fn new(scene_graph: SceneGraphPtr) -> Stage {
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

        let font = {
            let mut scene_graph = scene_graph.lock().unwrap();

            let font = scene_graph.add_node("font", SceneNodeType::Fonts);
            let font_id = font.id;
            scene_graph.link(font_id, SceneGraph::ROOT_ID).unwrap();

            let inter_regular = scene_graph.add_node("inter-regular", SceneNodeType::Font);

            let font = include_bytes!("../Inter-Regular.ttf") as &[u8];
            let font = Font::from_bytes(font, FontSettings::default()).unwrap();
            let line_metrics = font.horizontal_line_metrics(1.).unwrap();
            //inter_regular.add_property_f32("ascent", line_metrics.ascent).unwrap();
            //inter_regular.add_property_f32("descent", line_metrics.descent).unwrap();
            //inter_regular.add_property_f32("line_gap", line_metrics.line_gap).unwrap();
            //inter_regular.add_property_f32("new_line_size", line_metrics.new_line_size).unwrap();

            inter_regular.add_method(
                "create_text",
                vec![("text", "", PropertyType::Str), ("font_size", "", PropertyType::Float32)],
                vec![("node_id", "", PropertyType::SceneNodeId)],
            );

            let inter_regular_id = inter_regular.id;
            scene_graph.link(inter_regular_id, font_id).unwrap();
            font
        };

        let mut stage = Stage { ctx, pipeline, scene_graph, textures, font };
        stage.setup_scene_graph_window();
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
                vec![
                    ("screen_width", "", PropertyType::Float32),
                    ("screen_height", "", PropertyType::Float32),
                ],
            )
            .unwrap();
        window
            .add_method(
                "load_texture",
                vec![("node_name", "", PropertyType::Str), ("path", "", PropertyType::Str)],
                vec![("node_id", "", PropertyType::SceneNodeId)],
            )
            .unwrap();
        let window_id = window.id;
        scene_graph.link(window_id, SceneGraph::ROOT_ID).unwrap();

        let input = scene_graph.add_node("input", SceneNodeType::WindowInput);
        let input_id = input.id;
        scene_graph.link(input_id, window_id).unwrap();

        let keyb = scene_graph.add_node("keyboard", SceneNodeType::Keyboard);
        keyb.add_signal(
            "key_down",
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
                vec![("x", "", PropertyType::Float32), ("y", "", PropertyType::Float32)],
            )
            .unwrap();
        mouse
            .add_signal(
                "move",
                vec![("x", "", PropertyType::Float32), ("y", "", PropertyType::Float32)],
            )
            .unwrap();
        let mouse_id = mouse.id;
        scene_graph.link(mouse_id, input_id).unwrap();
    }

    fn draw_glyph(
        ctx: &mut Box<dyn RenderingBackend>,
        proj: &glam::Mat4,
        model: &glam::Mat4,
        font: &Font,
        glyph_pos: &GlyphPosition,
        color: [f32; 4],
    ) {
        //let proj =
        //    glam::Mat4::from_translation(glam::Vec3::new(-1., 1., 0.)) *
        //    glam::Mat4::from_scale(glam::Vec3::new(2./screen_width, -2./screen_height, 1.));
        //let model = glam::Mat4::IDENTITY;

        let mut uniforms_data = [0u8; 128];
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(proj) };
        uniforms_data[0..64].copy_from_slice(&data);
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(model) };
        uniforms_data[64..].copy_from_slice(&data);
        assert_eq!(128, 2 * UniformType::Mat4.size());

        ctx.apply_uniforms_from_bytes(uniforms_data.as_ptr(), uniforms_data.len());

        let (font_metrics, text_bitmap) = font.rasterize(glyph_pos.parent, glyph_pos.key.px);
        let text_bitmap: Vec<_> =
            text_bitmap.iter().flat_map(|coverage| vec![255, 255, 255, *coverage]).collect();

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
        let vertices: [Vertex; 4] = [
            // top left
            Vertex { pos: [x, y], color, uv: [0., 0.] },
            // top right
            Vertex { pos: [x + w, y], color, uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x, y + h], color, uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x + w, y + h], color, uv: [1., 1.] },
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

        let texture = ctx.new_texture_from_rgba8(
            font_metrics.width as u16,
            font_metrics.height as u16,
            &text_bitmap,
        );

        let bindings =
            Bindings { vertex_buffers: vec![vertex_buffer], index_buffer, images: vec![texture] };

        ctx.apply_bindings(&bindings);
        ctx.draw(0, 6, 1);

        ctx.delete_texture(texture);
    }

    fn create_text_node(
        &self,
        font_node_id: SceneNodeId,
        font_name: &str,
        node_name: String,
        text: String,
        font_size: f32,
    ) -> Result<SceneNodeId> {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let text_node = scene_graph.add_node(node_name, SceneNodeType::RenderText);

        let mut prop = Property::new("text", PropertyType::Str, PropertySubType::Null);
        text_node.add_property(prop)?;

        let mut prop = Property::new("font_size", PropertyType::Float32, PropertySubType::Pixel);
        text_node.add_property(prop)?;

        let mut prop = Property::new("color", PropertyType::Float32, PropertySubType::Color);
        prop.set_array_len(4);
        text_node.add_property(prop)?;

        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings { ..LayoutSettings::default() });
        let font = match font_name {
            "inter-regular" => &self.font,
            _ => panic!("unknown font name!"),
        };
        let fonts = [font];
        layout.append(&fonts, &TextStyle::new(&text, font_size, 0));

        // Calculate the text width
        // std::cmp::max() not impl for f32
        let max_f32 = |x: f32, y: f32| {
            if x > y {
                x
            } else {
                y
            }
        };

        // TODO: this calc isn't multiline, we should add width property to each line
        let mut total_width = 0.;
        for glyph_pos in layout.glyphs() {
            let right = glyph_pos.x + glyph_pos.width as f32;
            total_width = max_f32(total_width, right);
        }

        let mut prop = Property::new("size", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_array_len(2);
        prop.set_f32(0, total_width).unwrap();
        prop.set_f32(1, layout.height()).unwrap();

        let text_node_id = text_node.id;

        /*
        let lines = layout.lines();
        if lines.is_some() {
            for (idx, line) in lines.unwrap().into_iter().enumerate() {
                let line_node_name = format!("line.{}", idx);
                let line_node = scene_graph.add_node(line_node_name, SceneNodeType::LinePosition);
                //line_node.add_property_u32("idx", idx as u32).unwrap();
                //line_node.add_property_f32("baseline_y", line.baseline_y).unwrap();
                //line_node.add_property_f32("padding", line.padding).unwrap();
                //line_node.add_property_f32("max_ascent", line.max_ascent).unwrap();
                //line_node.add_property_f32("min_descent", line.min_descent).unwrap();
                //line_node.add_property_f32("max_line_gap", line.max_line_gap).unwrap();
                //line_node.add_property_u32("glyph_start", line.glyph_start as u32).unwrap();
                //line_node.add_property_u32("glyph_end", line.glyph_end as u32).unwrap();

                let line_node_id = line_node.id;
                scene_graph.link(line_node_id, text_node_id)?;
            }
        }
        */

        scene_graph.link(font_node_id, text_node_id)?;
        Ok(text_node_id)
    }

    fn load_texture(&mut self, node_name: String, filepath: String) -> Result<SceneNodeId> {
        let Ok(img) = image::open(filepath) else { return Err(Error::FileNotFound) };

        let img = img.to_rgba8();
        let width = img.width();
        let height = img.height();
        let bmp = img.into_raw();

        let texture = self.ctx.new_texture_from_rgba8(width as u16, height as u16, &bmp);
        let id = self.textures.alloc(texture);

        let mut scene_graph = self.scene_graph.lock().unwrap();
        let img_node = scene_graph.add_node(node_name, SceneNodeType::RenderTexture);
        //img_node.add_property_u32("width", width).unwrap();
        //img_node.add_property_u32("height", height).unwrap();
        //img_node.add_property_u32("texture_id", id).unwrap();

        let mut prop = Property::new("size", PropertyType::Uint32, PropertySubType::Pixel);
        prop.set_array_len(2);
        prop.set_u32(0, width).unwrap();
        prop.set_u32(1, height).unwrap();
        img_node.add_property(prop)?;

        let mut prop = Property::new("texture_id", PropertyType::Uint32, PropertySubType::Null);
        prop.set_u32(0, id).unwrap();
        img_node.add_property(prop)?;

        Ok(img_node.id)
    }
}

/*
fn get_obj_props(obj: &SceneNode) -> Result<(f32, f32, f32, f32, bool)> {
    let x = obj.get_property_f32("x")?;
    let y = obj.get_property_f32("y")?;
    let scale_x = obj.get_property_f32("scale_x")?;
    let scale_y = obj.get_property_f32("scale_y")?;
    let is_visible = obj.get_property_bool("is_visible")?;
    Ok((x, y, scale_x, scale_y, is_visible))
}

fn get_text_props(render_text: &SceneNode) -> Result<(String, f32, [f32; 4])> {
    let text = render_text.get_property_str("text")?;
    let font_size = render_text.get_property_f32("font_size")?;
    let r = render_text.get_property_f32("r")?;
    let g = render_text.get_property_f32("g")?;
    let b = render_text.get_property_f32("b")?;
    let a = render_text.get_property_f32("a")?;
    let color = [r, g, b, a];
    Ok((text, font_size, color))
}
*/

impl EventHandler for Stage {
    fn update(&mut self) {
        // check /font:create_text() queue

        let mut scene_graph = self.scene_graph.lock().unwrap();
        let font_root = scene_graph.lookup_node("/font").unwrap();
        let font_ids: Vec<_> = font_root
            .iter_children(&scene_graph, SceneNodeType::Font)
            .map(|node| node.id)
            .collect();

        let mut calls = vec![];
        for font_id in font_ids {
            let font_node = scene_graph.get_node_mut(font_id).unwrap();
            for method in &mut font_node.methods {
                for (arg_data, response_fn) in std::mem::take(&mut method.queue) {
                    calls.push((
                        font_node.id,
                        font_node.name.clone(),
                        method.name.clone(),
                        arg_data,
                        response_fn,
                    ));
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

            let node_id_result =
                self.create_text_node(font_node_id, &font_node_name, node_name, text, font_size);
            let node_id_result = node_id_result.map(|node_id| {
                node_id.encode(&mut reply).unwrap();
                reply
            });
            response_fn(node_id_result)
        }

        // check /window:load_texture() queue

        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mut calls = vec![];
        let window = scene_graph.lookup_node_mut("/window").unwrap();
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

        let (screen_width, screen_height) = window::screen_size();
        // This will make the top left (0, 0) and the bottom right (1, 1)
        // Default is (-1, 1) -> (1, -1)
        let proj = glam::Mat4::from_translation(glam::Vec3::new(-1., 1., 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(2. / screen_width, -2. / screen_height, 1.));

        // Reusable text layout
        //let mut layout = Layout::new(CoordinateSystem::PositiveYDown);

        let scene_graph = self.scene_graph.lock().unwrap();

        for layer in scene_graph
            .lookup_node("/window")
            .expect("no window attached!")
            .iter_children(&scene_graph, SceneNodeType::RenderLayer)
        {
            let is_visible = layer.get_property_bool("is_visible").unwrap();
            if !is_visible {
                continue;
            }

            //self.ctx.begin_default_pass(Default::default());
            self.ctx.begin_default_pass(PassAction::Nothing);
            self.ctx.apply_pipeline(&self.pipeline);

            /*
            let rect_x = layer.get_property_u32("rect_x").unwrap();
            let rect_y = layer.get_property_u32("rect_y").unwrap();
            let rect_w = layer.get_property_u32("rect_w").unwrap();
            let rect_h = layer.get_property_u32("rect_h").unwrap();

            self.ctx.apply_viewport(rect_x as i32, rect_y as i32, rect_w as i32, rect_h as i32);
            self.ctx.apply_scissor_rect(rect_x as i32, rect_y as i32, rect_w as i32, rect_h as i32);

            'outer: for obj in layer.iter_children(&scene_graph, SceneNodeType::RenderObject) {
                let Ok((x, y, scale_x, scale_y, is_visible)) = get_obj_props(obj) else {
                    error!("obj '{}':{} has a property error", obj.name, obj.id);
                    continue
                };

                if !is_visible {
                    continue
                }

                let model = glam::Mat4::from_translation(glam::Vec3::new(x, y, 0.)) *
                    glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

                let texture = 'texture: {
                    let Some(texture_node) =
                        obj.iter_children(&scene_graph, SceneNodeType::RenderTexture).next()
                    else {
                        break 'texture self.textures.get(Self::WHITE_TEXTURE_ID).unwrap()
                    };

                    let Ok(id) = texture_node.get_property_u32("texture_id") else {
                        error!(
                            "texture '{}':{} missing property texture_id",
                            texture_node.name, texture_node.id
                        );
                        continue 'outer
                    };

                    let Some(texture) = self.textures.get(id) else {
                        error!(
                            "texture '{}':{} texture with id {} is missing!",
                            texture_node.name, texture_node.id, id
                        );
                        continue 'outer
                    };
                    texture
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
                        index_buffer,
                        images: vec![*texture],
                    };

                    self.ctx.apply_bindings(&bindings);

                    let mut uniforms_data = [0u8; 128];
                    let data: [u8; 64] = unsafe { std::mem::transmute_copy(&proj) };
                    uniforms_data[0..64].copy_from_slice(&data);
                    let data: [u8; 64] = unsafe { std::mem::transmute_copy(&model) };
                    uniforms_data[64..].copy_from_slice(&data);
                    assert_eq!(128, 2 * UniformType::Mat4.size());

                    self.ctx.apply_uniforms_from_bytes(uniforms_data.as_ptr(), uniforms_data.len());

                    self.ctx.draw(0, 3 * faces.len() as i32, 1);
                }

                for render_text in obj.iter_children(&scene_graph, SceneNodeType::RenderText) {
                    let Ok((text, font_size, color)) = get_text_props(render_text) else {
                        error!(
                            "text '{}':{} has a property error",
                            render_text.name, render_text.id
                        );
                        continue
                    };

                    let Some(font_node) =
                        render_text.iter_children(&scene_graph, SceneNodeType::Font).next()
                    else {
                        error!(
                            "text '{}':{} missing a font node",
                            render_text.name, render_text.id
                        );
                        continue
                    };
                    // No other fonts supported yet
                    assert_eq!(font_node.name, "inter-regular");

                    layout.reset(&LayoutSettings { ..LayoutSettings::default() });
                    let fonts = [&self.font];
                    layout.append(&fonts, &TextStyle::new(&text, font_size, 0));

                    for glyph_pos in layout.glyphs() {
                        Self::draw_glyph(
                            &mut self.ctx,
                            &proj,
                            &model,
                            &self.font,
                            glyph_pos,
                            color,
                        );
                    }
                }
            }

            self.ctx.end_render_pass();
            */
        }
        self.ctx.commit_frame();
    }

    fn key_down_event(&mut self, keycode: KeyCode, modifiers: KeyMods, repeat: bool) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let win = scene_graph.lookup_node_mut("/window/input/keyboard").unwrap();

        //win.set_property_bool("shift", modifiers.shift).unwrap();
        //win.set_property_bool("ctrl", modifiers.ctrl).unwrap();
        //win.set_property_bool("alt", modifiers.alt).unwrap();
        //win.set_property_bool("logo", modifiers.logo).unwrap();
        //win.set_property_bool("repeat", repeat).unwrap();

        let send_key_down = |key: &str| {
            //win.set_property_str("keycode", key).unwrap();
            win.trigger("key_down").unwrap();
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
        let mouse = scene_graph.lookup_node_mut("/window/input/mouse").unwrap();
        //mouse.set_property_f32("motion_x", x).unwrap();
        //mouse.set_property_f32("motion_y", y).unwrap();
        mouse.trigger("move").unwrap();
    }
    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mouse = scene_graph.lookup_node_mut("/window/input/mouse").unwrap();
        //mouse.set_property_f32("x", x).unwrap();
        //mouse.set_property_f32("wheel_y", y).unwrap();
        mouse.trigger("wheel").unwrap();
    }
    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mouse = scene_graph.lookup_node_mut("/window/input/mouse").unwrap();
        //mouse.set_property_u32("button", button.to_u8() as u32).unwrap();
        //mouse.set_property_f32("click_x", x).unwrap();
        //mouse.set_property_f32("click_y", y).unwrap();
        mouse.trigger("button_down").unwrap();
    }
    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let mouse = scene_graph.lookup_node_mut("/window/input/mouse").unwrap();
        //mouse.set_property_u32("button", button.to_u8() as u32).unwrap();
        //mouse.set_property_f32("click_x", x).unwrap();
        //mouse.set_property_f32("click_y", y).unwrap();
        mouse.trigger("button_up").unwrap();
    }

    fn resize_event(&mut self, width: f32, height: f32) {
        let mut scene_graph = self.scene_graph.lock().unwrap();
        let win = scene_graph.lookup_node_mut("/window").unwrap();
        //win.set_property_f32("width", width).unwrap();
        //win.set_property_f32("height", height).unwrap();
        win.trigger("resize").unwrap();
    }
}

pub fn init_gui(scene_graph: SceneGraphPtr) {
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
