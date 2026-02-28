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

use crate::{
    gfx::{gfxtag, Renderer},
    scene::SceneNodePtr,
    util::ExecutorPtr,
};

/// Prolly misplaced for now lel.
/// Only allow one editor to be active in the list at any one time.
pub fn edit_switch(
    tasks: &mut Vec<smol::Task<()>>,
    edit_nodes: &[SceneNodePtr],
    renderer: Renderer,
    ex: ExecutorPtr,
) {
    for (i, edit_node) in edit_nodes.iter().enumerate() {
        let others: Vec<SceneNodePtr> =
            edit_nodes[..i].iter().chain(edit_nodes[i + 1..].iter()).cloned().collect();

        let is_focused = edit_node.get_property("is_focused").unwrap();
        let is_focused_sub = is_focused.subscribe_modify();
        let renderer = renderer.clone();
        let is_focused_task = ex.spawn(async move {
            while let Ok(_) = is_focused_sub.receive().await {
                // Is this edit focused?
                if !is_focused.get_bool(0).unwrap() {
                    continue
                }

                // Unfocus everything else in the list
                let atom = &mut renderer.make_guard(gfxtag!("write_click"));
                for other_node in others.clone() {
                    other_node.call_method("unfocus", vec![]).await.unwrap();
                }
            }
        });
        tasks.push(is_focused_task);
    }
}
