use binrw::binrw;

use crate::common::ContainerType;

/// Client → Server opcode 812 (0x032C), 24 bytes.
/// Sent when the player requests to meld a materia onto an equipped item (self-meld path).
/// Fields 0x0A and 0x14 are uninitialized client stack garbage — never validate them.
#[binrw]
#[brw(little)]
#[derive(Debug, Clone, Default)]
pub struct MeldMateriaRequest {
    /// The player's ObjectId (sender).
    pub object_id: u32,
    /// Container holding the materia to meld (source).
    #[brw(pad_size_to = 4)]
    pub src_container: ContainerType,
    /// Slot index of the materia in the source container.
    pub src_slot: u16,
    /// Uninitialized stack garbage — do NOT interpret.
    pub _pad_garbage_0a: u16,
    /// Container holding the target item (destination), always Equipped.
    #[brw(pad_size_to = 4)]
    pub dst_container: ContainerType,
    /// Slot index of the target item in the destination container.
    pub dst_slot: u16,
    /// 1 = repeat mode (一气呵成): loop until success or materia exhausted.
    pub repeat: u16,
    /// Uninitialized stack garbage — do NOT interpret.
    pub _pad_garbage_14: u32,
}
