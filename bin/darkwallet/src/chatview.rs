use atomic_float::AtomicF32;
use miniquad::{KeyMods, UniformType, MouseButton, window, TextureId};
use log::debug;
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
    //let file = File::open(filename).unwrap();
    //BufReader::new(file).lines().map(|l| l.unwrap()).collect()
    // Just so we can package for android easily
    // Later this will be all replaced anyway
    let file = include_bytes!("../chat.txt");
    BufReader::new(&file[..]).lines().map(|l| l.unwrap()).collect()
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
    atlas: Mutex<HashMap<(u32, [u8; 4]), TextureId>>,
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
            atlas: Mutex::new(HashMap::new()),
        });

        /*
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
        */

        let mouse_node = 
            scene_graph
            .lookup_node_mut("/window/input/mouse")
            .expect("no mouse attached!");
        //mouse_node.register("wheel", slot_wheel);
        //mouse_node.register("move", slot_move);

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

        // Used for scaling the font size
        let window = render
            .scene_graph
            .lookup_node("/window")
            .expect("no window attached!");
        let window_scale = window.get_property_f32("scale")?;
        let font_size = window_scale * 20.;

        let bound = Rectangle {
            x: 0.,
            y: 0.,
            w: rect.w,
            h: rect.h,
        };

        let glyph_lines = &mut self.glyph_lines.lock().unwrap();
        let atlas = &mut self.atlas.lock().unwrap();

        let scroll = self.scroll.load(Ordering::Relaxed);
        for (i, (line, glyph_line)) in self.lines.iter().zip(glyph_lines.iter_mut()).enumerate() {
            let line = line.replace("\t", "    ");

            // Split time and nick from the line
            let mut iter = line.split_whitespace();
            let Some(time) = iter.next() else {
                error!("line missing time");
                continue
            };
            let Some(nick) = iter.next() else {
                error!("line missing nick");
                continue
            };
            let Some(line) = iter.remainder() else {
                error!("line missing remainder");
                continue
            };

            let linespacing = window_scale*30.;
            let off_y = linespacing * i as f32 + scroll;
            if off_y + linespacing < 0. || off_y - linespacing > rect.h {
                continue;
            }

            if glyph_line.is_empty() {
                *glyph_line = self.text_shaper.shape(line.to_string(), font_size, COLOR_WHITE);
            }

            let times_color = [0.4, 0.4, 0.4, 1.];
            let times_color_u8 = [(255. * 0.4) as u8, (255. * 0.4) as u8, (255. * 0.4) as u8, (255. * 1.) as u8];
            let glyphs_time = self.text_shaper.shape(time.to_string(), font_size, times_color);
            let mut rhs = 0.;
            for glyph in glyphs_time {
                let mut pos = glyph.pos.clone();
                pos.y += off_y;
                rhs = pos.x + pos.w;

                assert_eq!(glyph.bmp.len() as u16, glyph.bmp_width*glyph.bmp_height*4);
                //debug!("gly {} {}", glyph.substr, glyph.bmp.len());
                let texture = if atlas.contains_key(&(glyph.id, times_color_u8.clone())) {
                    *atlas.get(&(glyph.id, times_color_u8.clone())).unwrap()
                } else {
                    let texture = render.ctx.new_texture_from_rgba8(glyph.bmp_width, glyph.bmp_height, &glyph.bmp);
                    atlas.insert((glyph.id, times_color_u8.clone()), texture);
                    texture
                };
                render.render_clipped_box_with_texture2(&bound, &pos, COLOR_WHITE, texture);
                //render.ctx.delete_texture(texture);
            }

            let nick_colors = [
                [0.00, 0.94, 1.00, 1.],
                [0.36, 1.00, 0.69, 1.],
                [0.29, 1.00, 0.45, 1.],
                [0.00, 0.73, 0.38, 1.],
                [0.21, 0.67, 0.67, 1.],
                [0.56, 0.61, 1.00, 1.],
                [0.84, 0.48, 1.00, 1.],
                [1.00, 0.61, 0.94, 1.],
                [1.00, 0.36, 0.48, 1.],
                [1.00, 0.30, 0.00, 1.]
            ];
            let nick_colors_u8 = [
                [(255. * 0.00) as u8, (255. * 0.94) as u8, (255. * 1.00) as u8, (255. * 1.) as u8],
                [(255. * 0.36) as u8, (255. * 1.00) as u8, (255. * 0.69) as u8, (255. * 1.) as u8],
                [(255. * 0.29) as u8, (255. * 1.00) as u8, (255. * 0.45) as u8, (255. * 1. ) as u8],
                [(255. * 0.00) as u8, (255. * 0.73) as u8, (255. * 0.38) as u8, (255. * 1. ) as u8],
                [(255. * 0.21) as u8, (255. * 0.67) as u8, (255. * 0.67) as u8, (255. * 1. ) as u8],
                [(255. * 0.56) as u8, (255. * 0.61) as u8, (255. * 1.00) as u8, (255. * 1. ) as u8],
                [(255. * 0.84) as u8, (255. * 0.48) as u8, (255. * 1.00) as u8, (255. * 1. ) as u8],
                [(255. * 1.00) as u8, (255. * 0.61) as u8, (255. * 0.94) as u8, (255. * 1. ) as u8],
                [(255. * 1.00) as u8, (255. * 0.36) as u8, (255. * 0.48) as u8, (255. * 1. ) as u8],
                [(255. * 1.00) as u8, (255. * 0.30) as u8, (255. * 0.00) as u8, (255. * 1. ) as u8]
            ];

            let nick_color = nick_colors[nick.len() % nick_colors.len()];
            let nick_color_u8 = nick_colors_u8[nick.len() % nick_colors.len()];
            let glyphs_nick = self.text_shaper.shape(nick.to_string(), font_size, nick_color);
            let off_x = rhs + window_scale*20.;
            for glyph in glyphs_nick {
                let mut pos = glyph.pos.clone();
                pos.x += off_x;
                pos.y += off_y;
                rhs = pos.x + pos.w;

                assert_eq!(glyph.bmp.len() as u16, glyph.bmp_width*glyph.bmp_height*4);
                //debug!("gly {} {}", glyph.substr, glyph.bmp.len());
                //let texture = render.ctx.new_texture_from_rgba8(glyph.bmp_width, glyph.bmp_height, &glyph.bmp);
                let texture = if atlas.contains_key(&(glyph.id, nick_color_u8.clone())) {
                    *atlas.get(&(glyph.id, nick_color_u8.clone())).unwrap()
                } else {
                    let texture = render.ctx.new_texture_from_rgba8(glyph.bmp_width, glyph.bmp_height, &glyph.bmp);
                    atlas.insert((glyph.id, nick_color_u8.clone()), texture);
                    texture
                };
                render.render_clipped_box_with_texture2(&bound, &pos, COLOR_WHITE, texture);
                //render.ctx.delete_texture(texture);
            }

            let off_x = rhs + window_scale*20.;
            for glyph in glyph_line {
                let mut pos = glyph.pos.clone();
                pos.x += off_x;
                pos.y += off_y;

                assert_eq!(glyph.bmp.len() as u16, glyph.bmp_width*glyph.bmp_height*4);
                //debug!("gly {} {}", glyph.substr, glyph.bmp.len());
                //let texture = render.ctx.new_texture_from_rgba8(glyph.bmp_width, glyph.bmp_height, &glyph.bmp);
                let texture = if atlas.contains_key(&(glyph.id, [255, 255, 255, 255])) {
                    *atlas.get(&(glyph.id, [255, 255, 255, 255])).unwrap()
                } else {
                    let texture = render.ctx.new_texture_from_rgba8(glyph.bmp_width, glyph.bmp_height, &glyph.bmp);
                    atlas.insert((glyph.id, [255, 255, 255, 255]), texture);
                    texture
                };
                render.render_clipped_box_with_texture2(&bound, &pos, COLOR_WHITE, texture);
                //render.ctx.delete_texture(texture);
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

