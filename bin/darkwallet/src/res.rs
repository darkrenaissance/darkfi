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

use crate::error::{Error, Result};

pub type ResourceId = u32;

pub struct ResourceManager<T> {
    resources: Vec<(ResourceId, Option<T>)>,
    freed: Vec<usize>,
    id_counter: ResourceId,
}

impl<T> ResourceManager<T> {
    pub fn new() -> Self {
        Self { resources: vec![], freed: vec![], id_counter: 0 }
    }

    pub fn alloc(&mut self, rsrc: T) -> ResourceId {
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

    pub fn get(&self, id: ResourceId) -> Option<&T> {
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

    pub fn free(&mut self, id: ResourceId) -> Result<()> {
        for (idx, (rsrc_id, rsrc)) in self.resources.iter_mut().enumerate() {
            if self.freed.contains(&idx) {
                return Err(Error::ResourceNotFound)
            }
            if *rsrc_id == id {
                *rsrc = None;
                self.freed.push(idx);
                return Ok(())
            }
        }
        Err(Error::ResourceNotFound)
    }
}
