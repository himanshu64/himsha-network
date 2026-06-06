//! ZK-proving support (feature `zkvm`).
//!
//! When built with `--features zkvm`, the `himsha-methods` crate provides the
//! compiled universal guest ELF and its image id. Every built-in program shares
//! this single guest (it dispatches by `program_id` internally), so they all
//! register against the same ELF/image id.

/// The universal guest ELF bytes.
pub fn guest_elf() -> &'static [u8] {
    himsha_methods::HIMSHA_GUEST_ELF
}

/// The universal guest image id as a 32-byte array.
///
/// `risc0-build` emits the id as `[u32; 8]`; the registry stores `[u8; 32]`, so
/// we serialize the words little-endian to match the form `Receipt::verify`
/// accepts elsewhere in the executor.
pub fn guest_image_id() -> [u8; 32] {
    let words: [u32; 8] = himsha_methods::HIMSHA_GUEST_ID;
    let mut id = [0u8; 32];
    for (i, w) in words.iter().enumerate() {
        id[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    id
}

/// Register every built-in program against the universal guest, so the executor
/// can prove them through the zkVM path.
pub fn register_builtins(registry: &mut crate::registry::ProgramRegistry) {
    let elf = guest_elf().to_vec();
    let image_id = guest_image_id();
    for id in himsha_runtime::program_ids::builtins() {
        registry.register(id, elf.clone(), image_id);
    }
}
