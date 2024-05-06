pub type Result<T> = std::result::Result<T, Error>;

#[repr(u8)]
#[derive(Debug, Copy, Clone, thiserror::Error)]
pub enum Error {
    #[error("Invalid scene path")]
    InvalidScenePath = 1,

    #[error("Node not found")]
    NodeNotFound = 2,

    #[error("Child node not found")]
    ChildNodeNotFound = 3,

    #[error("Parent node not found")]
    ParentNodeNotFound = 4,

    #[error("Property already exists")]
    PropertyAlreadyExists = 5,

    #[error("Property not found")]
    PropertyNotFound = 6,

    #[error("Property has wrong type")]
    PropertyWrongType = 7,

    #[error("Property value has the wrong length")]
    PropertyWrongLen = 8,

    #[error("Property index is wrong")]
    PropertyWrongIndex = 9,

    #[error("Property out of range")]
    PropertyOutOfRange = 10,

    #[error("Property null not allowed")]
    PropertyNullNotAllowed = 11,

    #[error("Property array is bounded length")]
    PropertyIsBounded = 12,

    #[error("Property enum item is invalid")]
    PropertyWrongEnumItem = 13,

    #[error("Signal already exists")]
    SignalAlreadyExists = 14,

    #[error("Signal not found")]
    SignalNotFound = 15,

    #[error("Slot not found")]
    SlotNotFound = 16,

    #[error("Signal already exists")]
    MethodAlreadyExists = 17,

    #[error("Method not found")]
    MethodNotFound = 18,

    #[error("Nodes are not linked")]
    NodesAreLinked = 19,

    #[error("Nodes are not linked")]
    NodesNotLinked = 20,

    #[error("Node has parents")]
    NodeHasParents = 21,

    #[error("Node has children")]
    NodeHasChildren = 22,

    #[error("Node has a parent with this name")]
    NodeParentNameConflict = 23,

    #[error("Node has a child with this name")]
    NodeChildNameConflict = 24,

    #[error("Node has a sibling with this name")]
    NodeSiblingNameConflict = 25,

    #[error("File not found")]
    FileNotFound = 26,
}
