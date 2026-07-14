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
    scene::{SceneNodePtr, Slot},
    util::ExecutorPtr,
};

/// Prolly misplaced for now lel.
/// Only allow one editor to be active in the list at any one time.
pub fn edit_switch(
    tasks: &mut Vec<smol::Task<()>>,
    edit_nodes: &[SceneNodePtr],
    ex: ExecutorPtr,
) {
    for (i, edit_node) in edit_nodes.iter().enumerate() {
        let others: Vec<SceneNodePtr> =
            edit_nodes[..i].iter().chain(edit_nodes[i + 1..].iter()).cloned().collect();

        let (slot, recvr) = Slot::new("editswitch_focus_changed");
        edit_node.register("focus_request", slot).unwrap();
        let edit_node = edit_node.clone();
        let is_focused_task = ex.spawn(async move {
            while let Ok(_) = recvr.recv().await {
                // Hide cursors on siblings
                for other_node in &others {
                    other_node.call_method("unfocus", vec![]).await.unwrap();
                }

                edit_node.call_method("focus", vec![]).await.unwrap();
            }
        });
        tasks.push(is_focused_task);
    }
}