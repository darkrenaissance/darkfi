/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use async_trait::async_trait;
use darkfi_money_contract::model::{DARK_TOKEN_ID, TokenId};
use darkfi_serial::{Decodable, Encodable, SerialEncodable};
use miniquad::{MouseButton, TouchPhase};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::sync::{Arc, Weak};

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, Point, Rectangle, RenderApi, Renderer},
    mesh::MeshBuilder,
    prop::{
        BatchGuardId, PropertyAtomicGuard, PropertyColor, PropertyFloat32, PropertyRect,
        PropertyUint32, Role,
    },
    scene::SceneNodeWeak,
    text,
    ui::Pimpl,
    ExecutorPtr,
};

use super::{DrawUpdate, UIObject};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::tokentable", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::tokentable", $($arg)*); } }

#[derive(Clone, Debug, SerialEncodable)]
pub struct TokenRow {
    pub id: TokenId,
    pub symbol: String,
    pub balance: String,
}

impl Decodable for TokenRow {
    fn decode<R: std::io::Read>(r: &mut R) -> Result<Self, std::io::Error> {
        let id = TokenId::decode(r)?;
        let symbol = String::decode(r)?;
        let balance = String::decode(r)?;
        Ok(Self { id, symbol, balance })
    }
}

pub type TokenTablePtr = Arc<TokenTable>;

pub struct TokenTable {
    node: SceneNodeWeak,
    renderer: Renderer,
    mouse_btn_token: SyncMutex<Option<TokenId>>,

    rows: SyncMutex<Vec<TokenRow>>,
    dc_key: u64,

    rect: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,

    font_size: PropertyFloat32,
    text_color: PropertyColor,
    separator_color: PropertyColor,
    column_spacing: PropertyFloat32,
    padding_x: PropertyFloat32,
    padding_y: PropertyFloat32,

    parent_rect: SyncMutex<Option<Rectangle>>,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
}

impl TokenTable {
    pub async fn new(
        node: SceneNodeWeak,
        renderer: Renderer,
    ) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let separator_color = PropertyColor::wrap(node_ref, Role::Internal, "separator_color").unwrap();
        let column_spacing = PropertyFloat32::wrap(node_ref, Role::Internal, "column_spacing", 0).unwrap();
        let padding_x = PropertyFloat32::wrap(node_ref, Role::Internal, "padding_x", 0).unwrap();
        let padding_y = PropertyFloat32::wrap(node_ref, Role::Internal, "padding_y", 0).unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();

        let self_ = Arc::new(Self {
            node: node.clone(),
            renderer: renderer.clone(),
            mouse_btn_token: SyncMutex::new(None),
            rows: SyncMutex::new(vec![]),
            dc_key: OsRng.gen(),
            rect,
            z_index,
            priority,
            font_size,
            text_color,
            separator_color,
            column_spacing,
            padding_x,
            padding_y,
            parent_rect: SyncMutex::new(None),
            tasks: SyncMutex::new(vec![]),
        });
        Pimpl::TokenTable(self_)
    }

    async fn process_set_tokens_method(me: &Weak<Self>, data: Vec<u8>) -> bool {
        fn decode_data(data: &[u8]) -> std::io::Result<Vec<TokenRow>> {
            let mut cur = std::io::Cursor::new(data);
            let mut rows = vec![];
            while cur.position() < data.len() as u64 {
                let row = TokenRow::decode(&mut cur)?;
                rows.push(row);
            }
            Ok(rows)
        }

        let Ok(rows) = decode_data(&data) else {
            error!(target: "ui::tokentable", "set_tokens() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            error!(target: "ui::tokentable", "self destroyed before set_tokens method task was stopped!");
            return false
        };

        self_.set_tokens(rows).await;
        true
    }

    /// Replace all rows in the token table
    pub async fn set_tokens(&self, rows: Vec<TokenRow>) {
        // Ensure DRK token is always shown first (balance is set to 0 if not present)
        let rows = if rows.iter().any(|row| row.id == *DARK_TOKEN_ID) {
            let mut drk_row = None;
            let mut other_rows = vec![];
            for row in rows {
                if row.id == *DARK_TOKEN_ID {
                    drk_row = Some(row);
                } else {
                    other_rows.push(row);
                }
            }
            match drk_row {
                Some(drk) => {
                    let mut result = vec![drk];
                    result.extend(other_rows);
                    result
                },
                None => vec![TokenRow {
                    id: *DARK_TOKEN_ID,
                    symbol: "DRK".to_string(),
                    balance: "0".to_string(),
                }],
            }
        } else {
            let mut result = vec![TokenRow {
                id: *DARK_TOKEN_ID,
                symbol: "DRK".to_string(),
                balance: "0".to_string(),
            }];
            result.extend(rows);
            result
        };

        *self.rows.lock() = rows;

        let atom = self.renderer.make_guard(gfxtag!("TokenTable::set_tokens"));
        self.redraw_cached(atom.batch_id).await;
    }

    /// Get row at specific screen y position
    fn get_row_at_y(&self, mouse_y: f32) -> Option<TokenRow> {
        let rect = self.rect.get();
        let padding_y = self.padding_y.get();
        let font_size = self.font_size.get();
        let row_height = padding_y * 2. + font_size + 1.;

        let y = mouse_y - rect.y;

        let rows = self.rows.lock();
        if y < 0. || y > rows.len() as f32 * row_height {
            return None
        }

        let row_index = (y / row_height).floor() as usize;
        if row_index < rows.len() {
            Some(rows[row_index].clone())
        } else {
            None
        }
    }

    /// Invalidates cache and redraws everything
    async fn redraw_all(&self, atom: &mut PropertyAtomicGuard) {
        let parent_rect = self.parent_rect.lock().unwrap().clone();
        self.rect.eval(atom, &parent_rect).expect("unable to eval rect");
        self.redraw_cached(atom.batch_id).await;
    }

    async fn redraw_cached(&self, batch_id: BatchGuardId) {
        let rect = self.rect.get();

        let mut mesh_instrs = self.get_meshes(&rect).await;

        let mut instrs = vec![DrawInstruction::ApplyView(rect)];
        instrs.append(&mut mesh_instrs);

        let draw_calls =
            vec![(self.dc_key, DrawCall::new(instrs, vec![], self.z_index.get(), "tokentable"))];

        self.renderer.replace_draw_calls(Some(batch_id), draw_calls);
    }

    async fn get_meshes(&self, rect: &Rectangle) -> Vec<DrawInstruction> {
        let rows = self.rows.lock();
        let font_size = self.font_size.get();
        let text_color = self.text_color.get();
        let separator_color = self.separator_color.get();
        let padding_x = self.padding_x.get();
        let padding_y = self.padding_y.get();

        let mut instrs = vec![];

        for (i, row) in rows.iter().enumerate() {
            let row_height = padding_y*2.+font_size+1.;
            let y_pos = (i as f32) * row_height;

            // Render symbol
            let symbol_layout = text::make_layout(
                &row.symbol,
                text_color,
                font_size,
                1.0,
                1.0,
                None,
                &[],
            );
            instrs.push(DrawInstruction::SetPos(Point::new(
                padding_x,
                y_pos+padding_y,
            )));
            let symbol_instrs = text::render_layout(
                &symbol_layout,
                &self.renderer,
                gfxtag!("tokentable_symbol"),
            );
            instrs.extend(symbol_instrs);

            // Render balance (aligned to right)
            let balance_layout =
                text::make_layout(&row.balance, text_color, font_size, 1.0, 1.0, None, &[]);
            let balance_width = balance_layout.width();
            instrs.push(DrawInstruction::SetPos(Point::new(
                rect.w-balance_width-padding_x,
                y_pos+padding_y,
            )));
            let balance_instrs =
                text::render_layout(&balance_layout, &self.renderer, gfxtag!("tokentable_balance"));
            instrs.extend(balance_instrs);

            // Draw separator line at bottom of row
            instrs.push(DrawInstruction::SetPos(Point::new(
                0.,
                y_pos+row_height,
            )));
            let mut mesh = MeshBuilder::new(gfxtag!("tokentable_separator"));
            mesh.draw_line(
                Point::new(0., 0.),
                Point::new(rect.w+1., 0.),
                separator_color,
                1.,
            );
            let mesh = mesh.alloc(&self.renderer);
            instrs.push(DrawInstruction::Draw(mesh.draw_untextured()));
        }

        instrs
    }
}

#[async_trait]
impl UIObject for TokenTable {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let node_ref = &self.node.upgrade().unwrap();

        let method_sub = node_ref.subscribe_method_call("set_tokens").unwrap();
        let me2 = me.clone();
        let set_tokens_method_task = ex.spawn(async move {
            loop {
                let Ok(method_call) = method_sub.receive().await else {
                    d!("Event relayer closed");
                    return
                };

                t!("method called: set_tokens({method_call:?})");
                assert!(method_call.send_res.is_none());

                if !Self::process_set_tokens_method(&me2, method_call.data).await {
                    return
                };
            }
        });

        *self.tasks.lock() = vec![set_tokens_method_task];
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        *self.parent_rect.lock() = None;
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        *self.parent_rect.lock() = Some(parent_rect);
        self.rect.eval(atom, &parent_rect).ok()?;
        let rect = self.rect.get();

        let mut mesh_instrs = self.get_meshes(&rect).await;

        let mut instrs = vec![DrawInstruction::ApplyView(rect)];
        instrs.append(&mut mesh_instrs);

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(instrs, vec![], self.z_index.get(), "tokentable"),
            )],
        })
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }

        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }

        if let Some(row) = self.get_row_at_y(mouse_pos.y) {
            *self.mouse_btn_token.lock() = Some(row.id);
            return true
        }

        false
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }

        let token_held = {
            let mut mouse_lock = self.mouse_btn_token.lock();
            let token_held = *mouse_lock;
            *mouse_lock = None;
            token_held
        };

        let Some(token_held) = token_held else {
            return false
        };

        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }

        let Some(row) = self.get_row_at_y(mouse_pos.y) else {
            return false
        };

        if row.id != token_held {
            return false
        }

        let mut data = vec![];
        if let Err(e) = row.encode(&mut data) {
            error!(target: "ui::tokentable", "Failed to encode row: {e}");
            return false
        }

        let node_ref = self.node.upgrade().unwrap();
        let _ = node_ref.trigger("row_click", data).await;

        true
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        // Ignore multi-touch
        if id != 0 {
            return false
        }

        let rect = self.rect.get();
        if !rect.contains(touch_pos) {
            return false
        }

        // Simulate mouse events
        match phase {
            TouchPhase::Started => self.handle_mouse_btn_down(MouseButton::Left, touch_pos).await,
            TouchPhase::Moved => false,
            TouchPhase::Ended => self.handle_mouse_btn_up(MouseButton::Left, touch_pos).await,
            TouchPhase::Cancelled => false,
        }
    }
}

impl Drop for TokenTable {
    fn drop(&mut self) {
        let atom = self.renderer.make_guard(gfxtag!("TokenTable::drop"));
        self.renderer
            .replace_draw_calls(Some(atom.batch_id), vec![(self.dc_key, Default::default())]);
    }
}

impl std::fmt::Debug for TokenTable {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}
