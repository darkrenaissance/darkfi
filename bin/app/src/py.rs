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

use std::sync::{Arc, Mutex};

use crate::{
    error::Result,
    plugin::{
        Category, Plugin, PluginEvent, PluginInstance, PluginInstancePtr, PluginMetadata, SemVer,
        SubCategory,
    },
    scene::SceneGraphPtr,
};

pub struct PythonPlugin {
    scene_graph: SceneGraphPtr,
}

impl PythonPlugin {
    pub fn new(scene_graph: SceneGraphPtr, sourcecode: String) -> Self {
        Self { scene_graph }
    }
}

impl Plugin for PythonPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "myplugin".to_string(),
            title: "My Plugin - Very Good A++".to_string(),
            desc: "This is the best plugin ever made. You should use it.".to_string(),
            author: "Tyler Durden".to_string(),
            version: SemVer {
                major: 0,
                minor: 0,
                patch: 1,
                pre: "alpha".to_string(),
                build: "".to_string(),
            },
            cat: Category::Null,
            subcat: SubCategory::Null,
        }
    }

    fn start(&self) -> Result<PluginInstancePtr> {
        let mut inst = PythonPluginInstance { scene_graph: self.scene_graph.clone() };
        Ok(Arc::new(Mutex::new(Box::new(inst))))
    }
}

struct PythonPluginInstance {
    scene_graph: SceneGraphPtr,
}

impl PluginInstance for PythonPluginInstance {
    fn update(&mut self, event: PluginEvent) -> Result<()> {
        Ok(())
    }
}
