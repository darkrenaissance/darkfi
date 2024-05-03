pub type Result<T> = std::result::Result<T, Error>;

#[repr(u8)]
#[derive(Debug, Copy, Clone, thiserror::Error)]
pub enum Error {
    #[error("Invalid scene path")]
    InvalidScenePath = 2,

    #[error("Node not found")]
    NodeNotFound = 3,

    #[error("Child node not found")]
    ChildNodeNotFound = 4,

    #[error("Parent node not found")]
    ParentNodeNotFound = 5,

    #[error("Property already exists")]
    PropertyAlreadyExists = 6,

    #[error("Property not found")]
    PropertyNotFound = 7,

    #[error("Property has wrong type")]
    PropertyWrongType = 8,

    #[error("Signal already exists")]
    SignalAlreadyExists = 9,

    #[error("Signal not found")]
    SignalNotFound = 10,

    #[error("Slot not found")]
    SlotNotFound = 11,

    #[error("Method not found")]
    MethodNotFound = 12,

    #[error("Nodes are not linked")]
    NodesAreLinked = 13,

    #[error("Nodes are not linked")]
    NodesNotLinked = 14,

    #[error("Node has parents")]
    NodeHasParents = 15,

    #[error("Node has children")]
    NodeHasChildren = 16,

    #[error("Node has a parent with this name")]
    NodeParentNameConflict = 17,

    #[error("Node has a child with this name")]
    NodeChildNameConflict = 18,

    #[error("Node has a sibling with this name")]
    NodeSiblingNameConflict = 19,

    #[error("File not found")]
    FileNotFound = 20,
}
