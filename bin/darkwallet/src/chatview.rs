use atomic_float::AtomicF32;
use miniquad::{KeyMods, UniformType, MouseButton, window};
use std::{
    collections::HashMap,
    path::Path,
    fs::File,
    io::{BufRead, BufReader, Cursor}, sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex}, time::{Instant, Duration}};
use darkfi_serial::Decodable;

use crate::{error::{Error, Result}, prop::{
    PropertyBool, PropertyFloat32, PropertyUint32, PropertyStr, PropertyColor,
    Property}, scene::{SceneGraph, SceneNode, SceneNodeId, Pimpl, Slot}, gfx::{Rectangle, RenderContext, COLOR_WHITE, COLOR_BLUE, COLOR_RED, COLOR_GREEN, FreetypeFace, COLOR_DARKGREY, Point}, text::{Glyph, TextShaper}, keysym::{MouseButtonAsU8, KeyCodeAsU16}};

fn read_lines<P>(filename: P) -> Vec<String>
where P: AsRef<Path>, {
    let file = File::open(filename).unwrap();
    BufReader::new(file).lines().map(|l| l.unwrap()).collect()
}

pub type ChatViewPtr = Arc<ChatView>;

pub struct ChatView {
    node_name: String,
    debug: PropertyBool,
    // Used for mouse interaction
    world_rect: Mutex<Rectangle<f32>>,
    mouse_pos: Mutex<Point<f32>>,
    text_shaper: TextShaper,
    scroll: AtomicF32,
    lines: Vec<String>,
    glyph_lines: Mutex<Vec<Vec<Glyph>>>,
}

impl ChatView {
    pub fn new(scene_graph: &mut SceneGraph, node_id: SceneNodeId, font_faces: Vec<FreetypeFace>) -> Result<Pimpl> {
        let node = scene_graph.get_node(node_id).unwrap();
        let node_name = node.name.clone();
        let debug = PropertyBool::wrap(node, "debug", 0)?;

        let text_shaper = TextShaper {
            font_faces
        };
        
        let lines = read_lines("chat.txt");
        let mut glyph_lines = vec![];
        glyph_lines.resize(lines.len(), vec![]);

        let self_ = Arc::new(Self{
            node_name: node_name.clone(),
            debug,
            world_rect: Mutex::new(Rectangle { x: 0., y: 0., w: 0., h: 0. }),
            mouse_pos: Mutex::new(Point { x: 0., y: 0. }),
            text_shaper,
            scroll: AtomicF32::new(0.),
            lines,
            glyph_lines: Mutex::new(glyph_lines),
        });

        let weak_self = Arc::downgrade(&self_);
        let slot_move = Slot {
            name: format!("{}::mouse_move", node_name),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let x = f32::decode(&mut cur).unwrap();
                let y = f32::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    let pos = &mut *self_.mouse_pos.lock().unwrap();
                    pos.x = x;
                    pos.y = y;
                }
            }),
        };

        let weak_self = Arc::downgrade(&self_);
        let slot_wheel = Slot {
            name: format!("{}::mouse_wheel", node_name),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let x = f32::decode(&mut cur).unwrap();
                let y = f32::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    self_.mouse_scroll(x, y);
                }
            }),
        };

        let mouse_node = 
            scene_graph
            .lookup_node_mut("/window/input/mouse")
            .expect("no mouse attached!");
        mouse_node.register("wheel", slot_wheel);
        mouse_node.register("move", slot_move);

        // Save any properties we use
        Ok(Pimpl::ChatView(self_))
    }

    pub fn render<'a>(&self, render: &mut RenderContext<'a>, node_id: SceneNodeId, layer_rect: &Rectangle<f32>) -> Result<()> {
        let debug = self.debug.get();

        let node = render.scene_graph.get_node(node_id).unwrap();
        let rect = RenderContext::get_dim(node, layer_rect)?;

        // Used for detecting mouse clicks
        let mut world_rect = rect.clone();
        world_rect.x += layer_rect.x as f32;
        world_rect.y += layer_rect.y as f32;
        *self.world_rect.lock().unwrap() = world_rect;

        let layer_w = layer_rect.w as f32;
        let layer_h = layer_rect.h as f32;
        let off_x = rect.x / layer_w;
        let off_y = rect.y / layer_h;
        // Use absolute pixel scale
        let scale_x = 1. / layer_w;
        let scale_y = 1. / layer_h;
        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        let mut uniforms_data = [0u8; 128];
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&render.proj) };
        uniforms_data[0..64].copy_from_slice(&data);
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&model) };
        uniforms_data[64..].copy_from_slice(&data);
        assert_eq!(128, 2 * UniformType::Mat4.size());

        render.ctx.apply_uniforms_from_bytes(uniforms_data.as_ptr(), uniforms_data.len());

        let bound = Rectangle {
            x: 0.,
            y: 0.,
            w: rect.w,
            h: rect.h,
        };

        let glyph_lines = &mut self.glyph_lines.lock().unwrap();

        let scroll = self.scroll.load(Ordering::Relaxed);
        for (i, (line, glyph_line)) in self.lines.iter().zip(glyph_lines.iter_mut()).enumerate() {
            //println!("line: {}", line);
            if glyph_line.is_empty() {
                *glyph_line = self.text_shaper.shape(line.clone(), 30., COLOR_WHITE);
            }
            let off_y = 50. * i as f32 + scroll;
            if off_y + 50. < 0. || off_y - 50. > rect.h {
                continue;
            }
            for glyph in glyph_line {
                let x1 = glyph.pos.x;
                let y1 = glyph.pos.y + off_y;
                let x2 = x1 + glyph.pos.w;
                let y2 = y1 + glyph.pos.h;

                let texture = render.ctx.new_texture_from_rgba8(glyph.bmp_width, glyph.bmp_height, &glyph.bmp);
                render.render_clipped_box_with_texture(&bound, x1, y1, x2, y2, COLOR_WHITE, texture);
                render.ctx.delete_texture(texture);
            }
        }

        if debug {
            render.outline(0., 0., rect.w, rect.h, COLOR_RED, 1.);
        }

        Ok(())
    }

    fn mouse_scroll(self: Arc<Self>, _x: f32, y: f32) {
        let mouse_pos = &*self.mouse_pos.lock().unwrap();
        if !self.world_rect.lock().unwrap().contains(mouse_pos) {
            return;
        }
        drop(mouse_pos);

        self.scroll.fetch_add(y * 10., Ordering::Relaxed);
        // y =  1 for scroll up
        // y = -1 for scroll down
        //println!("{}", y);
    }
}

