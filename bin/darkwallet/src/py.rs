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
