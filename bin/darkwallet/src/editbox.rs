use miniquad::{KeyMods, UniformType, MouseButton};
use std::{io::Cursor, sync::{Arc, Mutex}};
use darkfi_serial::Decodable;
use freetype as ft;

use crate::{error::{Error, Result}, prop::{
    PropertyBool, PropertyFloat32, PropertyUint32, PropertyStr, PropertyColor,
    Property}, scene::{SceneGraph, SceneNode, SceneNodeId, Pimpl, Slot}, gfx::{Rectangle, RenderContext, COLOR_WHITE, COLOR_BLUE, COLOR_RED, COLOR_GREEN, FreetypeFace, Point}, text::{Glyph, TextShaper}, keysym::{MouseButtonAsU8, KeyCodeAsU16}};

const CURSOR_WIDTH: f32 = 4.;

pub type EditBoxPtr = Arc<EditBox>;

pub struct EditBox {
    is_active: PropertyBool,
    debug: PropertyBool,
    baseline: PropertyFloat32,
    scroll: PropertyFloat32,
    cursor_pos: PropertyUint32,
    selected: Arc<Property>,
    text: PropertyStr,
    font_size: PropertyFloat32,
    text_color: PropertyColor,
    hi_bg_color: PropertyColor,
    // Used for mouse clicks
    world_rect: Mutex<Rectangle<f32>>,
    glyphs: Mutex<Vec<Glyph>>,
    text_shaper: TextShaper,
}

impl EditBox {
    pub fn new(scene_graph: &mut SceneGraph, node_id: SceneNodeId, font_faces: Vec<FreetypeFace>) -> Result<Pimpl> {
        let node = scene_graph.get_node(node_id).unwrap();
        let is_active = PropertyBool::wrap(node, "is_active", 0)?;
        let debug = PropertyBool::wrap(node, "debug", 0)?;
        let baseline = PropertyFloat32::wrap(node, "baseline", 0)?;
        let scroll = PropertyFloat32::wrap(node, "scroll", 0)?;
        let cursor_pos = PropertyUint32::wrap(node, "cursor_pos", 0)?;
        let selected = node.get_property("selected").ok_or(Error::PropertyNotFound)?;
        let text = PropertyStr::wrap(node, "text", 0)?;
        let font_size = PropertyFloat32::wrap(node, "font_size", 0)?;
        let text_color = PropertyColor::wrap(node, "text_color")?;
        let hi_bg_color = PropertyColor::wrap(node, "hi_bg_color")?;

        let text_shaper = TextShaper {
            font_faces
        };

        println!("EditBox::new()");
        let self_ = Arc::new(Self{
            is_active,
            debug,
            baseline,
            scroll,
            cursor_pos,
            selected,
            text,
            font_size,
            text_color,
            hi_bg_color,
            world_rect: Mutex::new(Rectangle { x: 0., y: 0., w: 0., h: 0. }),
            glyphs: Mutex::new(vec![]),
            text_shaper,
        });
        self_.regen_glyphs().unwrap();

        let weak_self = Arc::downgrade(&self_);
        let slot = Slot {
            name: "editbox::key_down".to_string(),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let keymods = KeyMods {
                    shift: Decodable::decode(&mut cur).unwrap(),
                    ctrl: Decodable::decode(&mut cur).unwrap(),
                    alt: Decodable::decode(&mut cur).unwrap(),
                    logo: Decodable::decode(&mut cur).unwrap(),
                };
                let repeat = bool::decode(&mut cur).unwrap();
                let key = String::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    self_.key_press(key, keymods, repeat);
                }
            }),
        };

        let keyb_node = 
            scene_graph
            .lookup_node_mut("/window/input/keyboard")
            .expect("no keyboard attached!");
        keyb_node.register("key_down", slot);

        let weak_self = Arc::downgrade(&self_);
        let slot_btn_down = Slot {
            name: "editbox::mouse_button_down".to_string(),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let button = MouseButton::from_u8(u8::decode(&mut cur).unwrap());
                let x = f32::decode(&mut cur).unwrap();
                let y = f32::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    self_.mouse_button_down(button, x, y);
                }
            }),
        };

        let weak_self = Arc::downgrade(&self_);
        let slot_btn_up = Slot {
            name: "editbox::mouse_button_up".to_string(),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let button = MouseButton::from_u8(u8::decode(&mut cur).unwrap());
                let x = f32::decode(&mut cur).unwrap();
                let y = f32::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    self_.mouse_button_up(button, x, y);
                }
            }),
        };

        let weak_self = Arc::downgrade(&self_);
        let slot_move = Slot {
            name: "editbox::mouse_move".to_string(),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let x = f32::decode(&mut cur).unwrap();
                let y = f32::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    self_.mouse_move(x, y);
                }
            }),
        };

        let mouse_node = 
            scene_graph
            .lookup_node_mut("/window/input/mouse")
            .expect("no mouse attached!");
        mouse_node.register("button_down", slot_btn_down);
        mouse_node.register("button_up", slot_btn_up);
        mouse_node.register("move", slot_move);

        // Save any properties we use
        Ok(Pimpl::EditBox(self_))
    }

    pub fn render<'a>(&self, render: &mut RenderContext<'a>, node_id: SceneNodeId, layer_rect: &Rectangle<f32>) -> Result<()> {
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
        //let model = glam::Mat4::IDENTITY;
        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        let mut uniforms_data = [0u8; 128];
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&render.proj) };
        uniforms_data[0..64].copy_from_slice(&data);
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&model) };
        uniforms_data[64..].copy_from_slice(&data);
        assert_eq!(128, 2 * UniformType::Mat4.size());

        render.ctx.apply_uniforms_from_bytes(uniforms_data.as_ptr(), uniforms_data.len());

        self.apply_cursor_scrolling(&rect);

        let node = render.scene_graph.get_node(node_id).unwrap();
        let debug = self.debug.get();
        let baseline = self.baseline.get();
        let scroll = self.scroll.get();
        let cursor_pos = self.cursor_pos.get() as usize;

        let color = node.get_property("text_color").ok_or(Error::PropertyNotFound)?;
        let text_color = self.text_color.get();

        if !self.selected.is_null(0)? && !self.selected.is_null(1)? {
            self.render_selected(render, &rect)?;
        }

        let mut rhs = 0.;
        for (glyph_idx, glyph) in self.glyphs.lock().unwrap().iter().enumerate() {
            let texture = render.ctx.new_texture_from_rgba8(glyph.bmp_width, glyph.bmp_height, &glyph.bmp);

            let x1 = glyph.pos.x + scroll;
            let y1 = glyph.pos.y + baseline;
            let x2 = x1 + glyph.pos.w;
            let y2 = y1 + glyph.pos.h;

            let bound = Rectangle {
                x: 0.,
                y: 0.,
                w: rect.w,
                h: rect.h,
            };
            render.render_clipped_box_with_texture(&bound, x1, y1, x2, y2, COLOR_WHITE, texture);
            //render.render_box_with_texture(x1, y1, x2, y2, COLOR_WHITE, texture);
            render.ctx.delete_texture(texture);

            if debug {
                render.outline(x1, y1, x2, y2, COLOR_BLUE, 1.);
            }

            if cursor_pos == glyph_idx + 1 {
                let cursor_color = [1., 0.5, 0.5, 1.];
                render.render_box(x2, 0., x2 + CURSOR_WIDTH, rect.h, cursor_color);
            }

            rhs = x2;
        }

        if cursor_pos == 0 {
            let cursor_color = [1., 0.5, 0.5, 1.];
            render.render_box(0., 0., CURSOR_WIDTH, rect.h, cursor_color);
        }

        if debug {
            render.hline(0., rhs, 0., COLOR_RED, 1.);
            render.outline(0., 0., rect.w, rect.h, COLOR_GREEN, 1.);
        }

        Ok(())
    }

    pub fn render_selected<'a>(&self, render: &mut RenderContext<'a>, rect: &Rectangle<f32>) -> Result<()> {
        let sel_start = self.selected.get_u32(0)? as usize;
        let sel_end = self.selected.get_u32(1)? as usize;
        assert!(sel_start <= sel_end);
        let scroll = self.scroll.get();
        let hi_bg_color = self.hi_bg_color.get();

        let mut start_x = 0.;
        let mut end_x = 0.;

        for (glyph_idx, glyph) in self.glyphs.lock().unwrap().iter().enumerate() {
            let x1 = glyph.pos.x + scroll;
            let x2 = x1 + glyph.pos.w;

            if glyph_idx == sel_start {
                start_x = x1;
            }
            if glyph_idx == sel_end {
                end_x = x2;
            }
        }

        render.render_box(start_x, 0., end_x, rect.h, hi_bg_color);
        Ok(())
    }

    fn apply_cursor_scrolling(&self, rect: &Rectangle<f32>) {
        let cursor_pos = self.cursor_pos.get() as usize;
        let mut scroll = self.scroll.get();

        let cursor_x = {
            let glyphs = &*self.glyphs.lock().unwrap();
            if cursor_pos == 0 {
                0.
            } else {
                assert!(cursor_pos < glyphs.len() + 1);
                let glyph = &glyphs[cursor_pos - 1];
                glyph.pos.x + glyph.pos.w
            }
        };

        if cursor_x + CURSOR_WIDTH + scroll > rect.w {
            scroll = rect.w - cursor_x - CURSOR_WIDTH;
        } else if cursor_x + scroll < 0. {
            scroll = -cursor_x;
        }

        self.scroll.set(scroll);
    }

    fn regen_glyphs(&self) -> Result<()> {
        let glyphs = self.text_shaper.shape(self.text.get(), self.font_size.get(), 
                self.text_color.get());
        *self.glyphs.lock().unwrap() = glyphs;
        Ok(())
    }

    fn key_press(self: Arc<Self>, key: String, mods: KeyMods, repeat: bool) {
        if repeat {
            return;
        }
        if !self.is_active.get() {
            return
        }
        match key.as_str() {
            "PageUp" => {
                println!("pageup!");
            }
            "PageDown" => {
                println!("pagedown!");
            }
            "Left" => {
                let mut cursor_pos = self.cursor_pos.get();
                if cursor_pos > 0 {
                    cursor_pos -= 1;
                    self.cursor_pos.set(cursor_pos);
                }

                if !mods.shift {
                    self.selected.set_null(0).unwrap();
                    self.selected.set_null(1).unwrap();
                } else {
                    if self.selected.is_null(0).unwrap() {
                        assert!(self.selected.is_null(1).unwrap());
                        self.selected.set_u32(0, cursor_pos).unwrap();
                    }
                    self.selected.set_u32(1, cursor_pos).unwrap();
                }
            }
            "Right" => {
                let mut cursor_pos = self.cursor_pos.get();
                let glyphs_len = self.glyphs.lock().unwrap().len() as u32;
                if cursor_pos < glyphs_len {
                    cursor_pos += 1;
                    self.cursor_pos.set(cursor_pos);
                }

                if !mods.shift {
                    self.selected.set_null(0).unwrap();
                    self.selected.set_null(1).unwrap();
                } else {
                    if self.selected.is_null(0).unwrap() {
                        assert!(self.selected.is_null(1).unwrap());
                        self.selected.set_u32(1, cursor_pos).unwrap();
                    }
                    self.selected.set_u32(1, cursor_pos).unwrap();
                }
            }
            "Delete" => {
                let cursor_pos = self.cursor_pos.get();
                if cursor_pos == 0 {
                    return;
                }
                let mut text = String::new();
                for (i, glyph) in self.glyphs.lock().unwrap().iter().enumerate() {
                    let mut substr = glyph.substr.clone();
                    if cursor_pos as usize == i {
                        // Lmk if anyone knows a better way to do substr.pop_front()
                        let mut chars = substr.chars();
                        chars.next();
                        substr = chars.as_str().to_string();
                    }
                    text.push_str(&substr);
                }
                self.text.set(text);
                self.regen_glyphs().unwrap();
            }
            "Backspace" => {
                let cursor_pos = self.cursor_pos.get();
                if cursor_pos == 0 {
                    return;
                }
                let mut text = String::new();
                for (i, glyph) in self.glyphs.lock().unwrap().iter().enumerate() {
                    let mut substr = glyph.substr.clone();
                    if cursor_pos as usize - 1 == i {
                        substr.pop().unwrap();
                    }
                    text.push_str(&substr);
                }
                self.cursor_pos.set(cursor_pos - 1);
                self.text.set(text);
                self.regen_glyphs().unwrap();
            }
            _ => {}
        }
    }

    fn mouse_button_down(self: Arc<Self>, button: MouseButton, x: f32, y: f32) {
        let mouse_pos = Point { x, y };
        let rect = self.world_rect.lock().unwrap();

        // clicking inside box will:
        // 1. make it active
        // 2. begin selection
        if rect.contains(&mouse_pos) {
            if !self.is_active.get() {
                self.is_active.set(true);
                println!("inside!");
                // Send signal
            }

            // set cursor pos
            // begin selection
        }
        // click outside the box will:
        // 1. make it inactive
        else {
            if self.is_active.get() {
                self.is_active.set(false);
                // Send signal
            }
        }
    }
    fn mouse_button_up(self: Arc<Self>, button: MouseButton, x: f32, y: f32) {
        // releasing mouse button will:
        // 1. end selection
    }
    fn mouse_move(self: Arc<Self>, x: f32, y: f32) {
        // if active and selection_active, then use x to modify the selection.
        // also implement scrolling when cursor is to the left or right
        // just scroll to the end
        // also set cursor_pos too
    }
}

