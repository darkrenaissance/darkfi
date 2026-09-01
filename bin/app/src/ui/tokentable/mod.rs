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
use darkfi_money_contract::model::{TokenId, DARK_TOKEN_ID};
use darkfi_serial::{Decodable, Encodable, SerialEncodable};
use miniquad::MouseButton;
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::sync::{Arc, Weak};

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, EpochCache, Point, Rectangle, RenderApi, Renderer},
    mesh::MeshBuilder,
    prop::{
        PropertyAtomicGuard, PropertyColor, PropertyFloat32, PropertyRect, PropertyUint32, Role,
    },
    scene::SceneNodeWeak,
    text,
    ui::Pimpl,
    ExecutorPtr,
};

use super::{DrawUpdate, GestureAction, GestureSet, OnModify, RedrawTrigger, UIObject};

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
    redraw: RedrawTrigger,
    mouse_btn_token: SyncMutex<Option<TokenId>>,

    rows: SyncMutex<Vec<TokenRow>>,
    dc_key: u64,

    rect: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,

    font_size: PropertyFloat32,
    text_color: PropertyColor,
    separator_color: PropertyColor,
    padding_x: PropertyFloat32,
    padding_y: PropertyFloat32,

    /// Cached draw instructions. Empty means stale. Entries from a dead
    /// UI epoch are evicted automatically.
    draw_cache: EpochCache<Vec<DrawInstruction>>,

    parent_rect: SyncMutex<Option<Rectangle>>,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
}

impl TokenTable {
    pub async fn new(node: SceneNodeWeak, renderer: Renderer, redraw: RedrawTrigger) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let separator_color =
            PropertyColor::wrap(node_ref, Role::Internal, "separator_color").unwrap();
        let padding_x = PropertyFloat32::wrap(node_ref, Role::Internal, "padding_x", 0).unwrap();
        let padding_y = PropertyFloat32::wrap(node_ref, Role::Internal, "padding_y", 0).unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();

        let draw_cache = EpochCache::new(&renderer);

        let self_ = Arc::new(Self {
            node: node.clone(),
            renderer: renderer.clone(),
            redraw,
            mouse_btn_token: SyncMutex::new(None),
            rows: SyncMutex::new(vec![]),
            dc_key: OsRng.gen(),
            rect,
            z_index,
            priority,
            font_size,
            text_color,
            separator_color,
            padding_x,
            padding_y,
            draw_cache,
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

        self_.set_tokens(rows);
        true
    }

    /// Replace all rows in the token table
    pub fn set_tokens(&self, rows: Vec<TokenRow>) {
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
                }
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

        self.draw_cache.clear();
        self.redraw.trigger();
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

    /// Emit the `row_click` signal for a tapped row.
    async fn trigger_row_click(&self, row: TokenRow) {
        let mut data = vec![];
        if let Err(e) = row.encode(&mut data) {
            error!(target: "ui::tokentable", "Failed to encode row: {e}");
            return
        }

        let node_ref = self.node.upgrade().unwrap();
        let _ = node_ref.trigger("row_click", data).await;
    }

    fn get_meshes(&self, rect: &Rectangle) -> Vec<DrawInstruction> {
        let rows = self.rows.lock();
        let font_size = self.font_size.get();
        let text_color = self.text_color.get();
        let separator_color = self.separator_color.get();
        let padding_x = self.padding_x.get();
        let padding_y = self.padding_y.get();

        let mut instrs = vec![];

        for (i, row) in rows.iter().enumerate() {
            let row_height = padding_y * 2. + font_size + 1.;
            let y_pos = (i as f32) * row_height;

            // Render symbol
            let symbol_layout =
                text::make_layout(&row.symbol, text_color, font_size, 1.0, 1.0, None, &[]);
            instrs.push(DrawInstruction::SetPos(Point::new(padding_x, y_pos + padding_y)));
            let symbol_instrs =
                text::render_layout(&symbol_layout, &self.renderer, gfxtag!("tokentable_symbol"));
            instrs.extend(symbol_instrs);

            // Render balance (aligned to right)
            let balance_layout =
                text::make_layout(&row.balance, text_color, font_size, 1.0, 1.0, None, &[]);
            let balance_width = balance_layout.width();
            instrs.push(DrawInstruction::SetPos(Point::new(
                rect.w - balance_width - padding_x,
                y_pos + padding_y,
            )));
            let balance_instrs =
                text::render_layout(&balance_layout, &self.renderer, gfxtag!("tokentable_balance"));
            instrs.extend(balance_instrs);

            // Draw separator line at bottom of row
            instrs.push(DrawInstruction::SetPos(Point::new(0., y_pos + row_height)));
            let mut mesh = MeshBuilder::new(gfxtag!("tokentable_separator"));
            mesh.draw_line(Point::new(0., 0.), Point::new(rect.w + 1., 0.), separator_color, 1.);
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

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());

        on_modify.when_change_external(self.rect.prop(), |self_, _| async move {
            self_.draw_cache.clear();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.font_size.prop(), |self_, _| async move {
            self_.draw_cache.clear();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.text_color.prop(), |self_, _| async move {
            self_.draw_cache.clear();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.separator_color.prop(), |self_, _| async move {
            self_.draw_cache.clear();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.padding_x.prop(), |self_, _| async move {
            self_.draw_cache.clear();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.padding_y.prop(), |self_, _| async move {
            self_.draw_cache.clear();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.z_index.prop(), |self_, _| async move {
            self_.draw_cache.clear();
            self_.redraw.trigger();
        });

        let mut tasks = vec![set_tokens_method_task];
        tasks.append(&mut on_modify.tasks);
        *self.tasks.lock() = tasks;
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        *self.parent_rect.lock() = None;
        self.draw_cache.clear();
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        *self.parent_rect.lock() = Some(parent_rect);

        // Rect property is its own memo: compare before/after eval.
        let prev_rect = self.rect.get();
        self.rect.eval(atom, &parent_rect).ok()?;
        let rect = self.rect.get();
        let rect_changed = rect != prev_rect;

        // Compute under the cache lock so a concurrent invalidation lands
        // before or after, never between.
        if rect_changed {
            self.draw_cache.clear();
        }
        let instrs = self.draw_cache.get_or_insert_with(|| {
            let mut mesh_instrs = self.get_meshes(&rect);
            let mut instrs = vec![DrawInstruction::ApplyView(rect)];
            instrs.append(&mut mesh_instrs);
            instrs
        });

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

        let Some(token_held) = token_held else { return false };

        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }

        let Some(row) = self.get_row_at_y(mouse_pos.y) else { return false };

        if row.id != token_held {
            return false
        }

        self.trigger_row_click(row).await;

        true
    }

    fn gesture_set(&self) -> GestureSet {
        GestureSet::TAP
    }

    fn gesture_hit_test(&self, pos: Point) -> bool {
        // Only the rows are tappable. The table's rect spans the rest
        // of the screen below it (it sizes to the layer), so a rect-only
        // hit-test would own touches meant for widgets underneath —
        // the old dispatch fell through to them when no row matched.
        self.rect.get().contains(pos) && self.get_row_at_y(pos.y).is_some()
    }

    async fn handle_gesture(&self, gesture: GestureAction) -> bool {
        let GestureAction::Tap { pos } = gesture else { return false };

        let Some(row) = self.get_row_at_y(pos.y) else { return false };

        self.trigger_row_click(row).await;

        true
    }
}

impl Drop for TokenTable {
    fn drop(&mut self) {
        self.renderer.replace_draw_calls(vec![(self.dc_key, Default::default())]);
    }
}

impl std::fmt::Debug for TokenTable {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        app::node::create_tokentable,
        gfx::Renderer,
        prop::{PropertyAtomicGuard, Role},
        scene::SceneNode,
        ui::RedrawTrigger,
    };

    /// The table's gesture hit-test region is its rows, not its whole
    /// rect: the rect spans the remainder of the layer (widgets like
    /// the wallet chat button live underneath it), and chain
    /// resolution has no per-event sibling fallthrough.
    #[test]
    fn gesture_hit_test_only_passes_on_rows() {
        smol::block_on(async {
            let (redraw_tx, _redraw_rx) = RedrawTrigger::new();
            let (method_tx, _method_rx) = async_channel::unbounded();
            let renderer = Renderer::new(method_tx);

            let node = create_tokentable("tokens_table");
            {
                let atom = &mut PropertyAtomicGuard::none();
                let rect = node.get_property("rect").unwrap();
                rect.set_f32(atom, Role::App, 0, 0.).unwrap();
                rect.set_f32(atom, Role::App, 1, 100.).unwrap();
                rect.set_f32(atom, Role::App, 2, 600.).unwrap();
                rect.set_f32(atom, Role::App, 3, 1000.).unwrap();
                node.set_property_f32(atom, Role::App, "font_size", 18.).unwrap();
                node.set_property_f32(atom, Role::App, "padding_x", 8.).unwrap();
                node.set_property_f32(atom, Role::App, "padding_y", 8.).unwrap();
            }

            let node = node.setup(|me| TokenTable::new(me, renderer, redraw_tx)).await;
            let obj = node.pimpl();
            let Pimpl::TokenTable(table) = obj else { panic!() };

            // No rows yet: nothing passes, even inside the rect
            assert!(!table.gesture_hit_test(Point::new(50., 110.)));

            table.set_tokens(vec![TokenRow {
                id: *DARK_TOKEN_ID,
                symbol: "DRK".to_string(),
                balance: "0".to_string(),
            }]);

            // Row height = padding_y * 2 + font_size + 1 = 35
            // Inside row 0
            assert!(table.gesture_hit_test(Point::new(50., 110.)));
            // Inside the rect but below every row (where the chat
            // button lives)
            assert!(!table.gesture_hit_test(Point::new(50., 900.)));
            // Outside the rect entirely
            assert!(!table.gesture_hit_test(Point::new(50., 50.)));
        });
    }
}
