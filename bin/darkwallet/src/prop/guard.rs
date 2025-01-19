/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use super::{ModifyAction, ModifyPublisher, PropertyPtr, Role};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "prop", $($arg)*); } }

/// This schedules all property updates to happen at the end of the scope.
/// We can therefore have fine-grained control about when property updates are
/// propagated to the rest of the scenegraph.
///
/// This way we avoid triggering draw updates mid draw, and changes are atomic.
/// For example resizing the content view, will trigger the editbox background to
/// redraw while a current window wide draw is in progress. Since the window draw
/// triggered the change when we submit the draw update, it will be discarded by
/// the editbox bg triggered update. However this update won't have the current
/// rect and will be stale.
///
/// 1. Content draw starts
/// 2. Dependent property triggers and submits editbox bg redraw.
/// 3. Content draw continues and now draws editbox bg with updated rect.
/// 4. Finished content draw's editbox bg update is discarded in favour of #2.
///    However #2 used the pre-updated rect and is now stale.
///
/// We solve the above issue by batching all updates until after the draw call is finished.
/// This also has the unintended side-effect of making draws much faster since they aren't
/// interrupted halfway through by extra compute.
pub struct PropertyAtomicGuard {
    updates: Vec<(PropertyPtr, Role, ModifyAction)>,
}

impl PropertyAtomicGuard {
    pub fn new() -> Self {
        Self { updates: vec![] }
    }

    pub(super) fn add(&mut self, prop: PropertyPtr, role: Role, action: ModifyAction) {
        self.updates.push((prop, role, action));
    }
}

impl Drop for PropertyAtomicGuard {
    fn drop(&mut self) {
        for (prop, role, action) in std::mem::take(&mut self.updates) {
            prop.on_modify.notify((role, action));
        }
    }
}
