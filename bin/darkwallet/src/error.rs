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

    #[error("Property has wrong subtype")]
    PropertyWrongSubType = 8,

    #[error("Property value has the wrong length")]
    PropertyWrongLen = 9,

    #[error("Property index is wrong")]
    PropertyWrongIndex = 10,

    #[error("Property out of range")]
    PropertyOutOfRange = 11,

    #[error("Property null not allowed")]
    PropertyNullNotAllowed = 12,

    #[error("Property array is bounded length")]
    PropertyIsBounded = 13,

    #[error("Property enum item is invalid")]
    PropertyWrongEnumItem = 14,

    #[error("Signal already exists")]
    SignalAlreadyExists = 15,

    #[error("Signal not found")]
    SignalNotFound = 16,

    #[error("Slot not found")]
    SlotNotFound = 17,

    #[error("Signal already exists")]
    MethodAlreadyExists = 18,

    #[error("Method not found")]
    MethodNotFound = 19,

    #[error("Nodes are not linked")]
    NodesAreLinked = 20,

    #[error("Nodes are not linked")]
    NodesNotLinked = 21,

    #[error("Node has parents")]
    NodeHasParents = 22,

    #[error("Node has children")]
    NodeHasChildren = 23,

    #[error("Node has a parent with this name")]
    NodeParentNameConflict = 24,

    #[error("Node has a child with this name")]
    NodeChildNameConflict = 25,

    #[error("Node has a sibling with this name")]
    NodeSiblingNameConflict = 26,

    #[error("File not found")]
    FileNotFound = 27,

    #[error("Resource is not found")]
    ResourceNotFound = 28,

    #[error("Python expr eval error")]
    PyEvalErr = 29,
}
