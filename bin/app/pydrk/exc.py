class InvalidScenePath(Exception):
    pass
class NodeNotFound(Exception):
    pass
class ChildNodeNotFound(Exception):
    pass
class ParentNodeNotFound(Exception):
    pass
class PropertyAlreadyExists(Exception):
    pass
class PropertyNotFound(Exception):
    pass
class PropertyWrongType(Exception):
    pass
class PropertyWrongLen(Exception):
    pass
class PropertyWrongIndex(Exception):
    pass
class PropertyOutOfRange(Exception):
    pass
class PropertyNullNotAllowed(Exception):
    pass
class PropertySExprNotAllowed(Exception):
    pass
class PropertyIsBounded(Exception):
    pass
class PropertyWrongEnumItem(Exception):
    pass
class SignalAlreadyExists(Exception):
    pass
class SignalNotFound(Exception):
    pass
class SlotNotFound(Exception):
    pass
class MethodAlreadyExists(Exception):
    pass
class MethodNotFound(Exception):
    pass
class NodesAreLinked(Exception):
    pass
class NodesNotLinked(Exception):
    pass
class NodeHasParents(Exception):
    pass
class NodeHasChildren(Exception):
    pass
class NodeParentNameConflict(Exception):
    pass
class NodeChildNameConflict(Exception):
    pass
class NodeSiblingNameConflict(Exception):
    pass
class SExprGlobalNotFound(Exception):
    pass
class PublisherDestroyed(Exception):
    pass
class ChannelClosed(Exception):
    pass
class NodesAreSame(Exception):
    pass
class UnexpectedToken(Exception):
    pass
class KvdbErr(Exception):
    pass
class ServiceFailed(Exception):
    pass
class GfxDuplicateTextureID(Exception):
    pass
class GfxUnknownTextureID(Exception):
    pass
class GfxDuplicateBufferID(Exception):
    pass
class GfxUnknownBufferID(Exception):
    pass
class GfxDuplicateAnimID(Exception):
    pass
class GfxUnknownAnimID(Exception):
    pass
class ContactNotFound(Exception):
    pass
class SerialErr(Exception):
    pass
class TursoErr(Exception):
    pass
class UnsupportedNodeType(Exception):
    pass
class NodeNotRemovable(Exception):
    pass
class UnknownError(Exception):
    pass
