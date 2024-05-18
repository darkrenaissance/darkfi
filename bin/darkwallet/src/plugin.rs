use crate::{
    error::{Error, Result},
scene::{SceneGraph, SceneGraphPtr}
};

enum Category {
    Null,
}

enum SubCategory {
    Null,
}

struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: String,
    pub build: String,
}

struct PluginMetadata {
    pub name: String,
    pub title: String,
    pub desc: String,
    pub author: String,
    pub version: SemVer,

    pub cat: Category,
    pub subcat: SubCategory,

    // icon

    // Permissions
    // whitelisted nodes + props/methods (use * for all)
    // /window/input/*
}

enum PluginEvent {
    // (signal_data, user_data)
    RecvSignal((Vec<u8>, Vec<u8>)),
}

trait Plugin {
    fn metadata(&self) -> PluginMetadata;
    fn init(&mut self) -> Result<()>;
    fn update(&mut self, event: PluginEvent) -> Result<()>;
}

type InstanceId = u32;

pub struct Sentinel {
 scene_graph: SceneGraphPtr
}

impl Sentinel {
    pub fn new(scene_graph: SceneGraphPtr) -> Self {
        // Create /plugin in scene graph
        //
        // Methods provided in SceneGraph under /plugin:
        //
        // * import_plugin(pycode)

        Self {
            scene_graph
        }
    }

    pub fn run(&mut self) {
        // loop {
            // Monitor all running plugins
            // Check last update times
            // Kill any slowpokes

            // Check any SceneGraph method requests
        // }
    }

    fn import_plugin(&mut self, plugin: Box<dyn Plugin>) -> Result<()> {
        // Create /plugin/foo
        // Add a method called start()
        Ok(())
    }

    fn start_plugin(&mut self, plugin_name: &str) -> Result<InstanceId> {
        // Lookup plugin by name
        // Call init()
        // Spawn a new thread, allocate it an ID
        // Thread waits for events from the scene_graph and calls update() when they occur.
        // See src/net.rs:81 for an example
        Ok(0)
    }
}

