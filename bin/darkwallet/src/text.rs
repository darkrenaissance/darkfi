use freetype as ft;

use crate::gfx::{FreetypeFace, Rectangle};

#[derive(Clone)]
pub struct Glyph {
    pub id: u32,
    // Substring this glyph corresponds to
    pub substr: String,

    // The texture
    pub bmp: Vec<u8>,
    pub bmp_width: u16,
    pub bmp_height: u16,

    pub pos: Rectangle<f32>,
}

pub struct TextShaper {
    pub font_faces: Vec<FreetypeFace>,
}

unsafe impl Send for TextShaper {}
unsafe impl Sync for TextShaper {}

impl TextShaper {
    fn split_into_substrs(&self, text: String) -> Vec<(usize, String)> {
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

                warn!("no font fallback for char: '{}'", chr);
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
        substrs
    }

    pub fn shape(&self, text: String, font_size: f32, text_color: [f32; 4]) -> Vec<Glyph> {
        let substrs = self.split_into_substrs(text.clone());

        let mut glyphs: Vec<Glyph> = vec![];

        let mut current_x = 0.;
        let mut current_y = 0.;

        for (face_idx, text) in substrs {
            //debug!("substr {}", text);
            let face = &self.font_faces[face_idx];
            if face.has_fixed_sizes() {
                // emojis required a fixed size
                //face.set_char_size(109 * 64, 0, 72, 72).unwrap();
                face.select_size(0).unwrap();
            } else {
                face.set_char_size(font_size as isize * 64, 0, 72, 72).unwrap();
            }

            let hb_font = harfbuzz_rs::Font::from_freetype_face(face.clone());
            let buffer = harfbuzz_rs::UnicodeBuffer::new()
                .set_cluster_level(harfbuzz_rs::ClusterLevel::MonotoneCharacters)
                .add_str(&text);
            let output = harfbuzz_rs::shape(&hb_font, buffer, &[]);

            let positions = output.get_glyph_positions();
            let infos = output.get_glyph_infos();

            let mut prev_cluster = 0;

            for (i, (position, info)) in positions.iter().zip(infos).enumerate() {
                let gid = info.codepoint;
                // Index within this substr
                let curr_cluster = info.cluster as usize;

                // Skip first time
                if i != 0 {
                    let substr = text[prev_cluster..curr_cluster].to_string();
                    glyphs.last_mut().unwrap().substr = substr;
                }

                prev_cluster = curr_cluster;

                let mut flags = ft::face::LoadFlag::DEFAULT;
                if face.has_color() {
                    flags |= ft::face::LoadFlag::COLOR;
                }
                // FIXME: glyph 884 hangs on android
                // For now just avoid using emojis on android
                //debug!("load_glyph {}", gid);
                face.load_glyph(gid, flags).unwrap();
                //debug!("load_glyph {} [done]", gid);

                let glyph = face.glyph();
                glyph.render_glyph(ft::RenderMode::Normal).unwrap();

                let bmp = glyph.bitmap();
                let buffer = bmp.buffer();
                let bmp_width = bmp.width() as usize;
                let bmp_height = bmp.rows() as usize;
                let bearing_x = glyph.bitmap_left() as f32;
                let bearing_y = glyph.bitmap_top() as f32;

                let pixel_mode = bmp.pixel_mode().unwrap();
                let bmp = match pixel_mode {
                    ft::bitmap::PixelMode::Bgra => {
                        let mut tdata = vec![];
                        tdata.resize(4 * bmp_width * bmp_height, 0);
                        // Convert from BGRA to RGBA
                        for i in 0..bmp_width * bmp_height {
                            let idx = i * 4;
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
                                let r = (255. * text_color[0]) as u8;
                                let g = (255. * text_color[1]) as u8;
                                let b = (255. * text_color[2]) as u8;
                                let α = ((*coverage as f32) * text_color[3]) as u8;
                                vec![r, g, b, α]
                            })
                            .collect();
                        tdata
                    }
                    _ => panic!("unsupport pixel mode: {:?}", pixel_mode),
                };

                let pos = if face.has_fixed_sizes() {
                    // Downscale by height
                    let w = (bmp_width as f32 * font_size) / bmp_height as f32;
                    let h = font_size;

                    let x = current_x;
                    let y = current_y - h;

                    current_x += w;

                    Rectangle { x, y, w, h }
                } else {
                    let (w, h) = (bmp_width as f32, bmp_height as f32);

                    let off_x = position.x_offset as f32 / 64.;
                    let off_y = position.y_offset as f32 / 64.;

                    let x = current_x + off_x + bearing_x;
                    let y = current_y - off_y - bearing_y;

                    let x_advance = position.x_advance as f32 / 64.;
                    let y_advance = position.y_advance as f32 / 64.;
                    current_x += x_advance;
                    current_y += y_advance;

                    Rectangle { x, y, w, h }
                };

                let glyph = Glyph {
                    id: gid,
                    substr: String::new(),
                    bmp,
                    bmp_width: bmp_width as u16,
                    bmp_height: bmp_height as u16,
                    pos,
                };

                glyphs.push(glyph);
            }

            let substr = text[prev_cluster..].to_string();
            glyphs.last_mut().unwrap().substr = substr;
        }

        glyphs
    }
}
