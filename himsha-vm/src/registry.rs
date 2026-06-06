use himsha_runtime::Pubkey;
use std::collections::HashMap;

/// Maps program IDs to their compiled ELF bytecode and RISC Zero image IDs.
///
/// At node startup, all built-in programs are pre-registered.
/// Users can deploy custom programs via the `deployProgram` RPC call.
#[derive(Default)]
pub struct ProgramRegistry {
    programs: HashMap<[u8; 32], RegisteredProgram>,
}

pub struct RegisteredProgram {
    pub program_id: Pubkey,
    /// The RISC-V ELF binary that runs inside the zkVM.
    pub elf: Vec<u8>,
    /// RISC Zero image ID (SHA-256 of ELF in a specific format).
    pub image_id: [u8; 32],
}

impl ProgramRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, program_id: Pubkey, elf: Vec<u8>, image_id: [u8; 32]) {
        self.programs.insert(
            program_id.into(),
            RegisteredProgram {
                program_id,
                elf,
                image_id,
            },
        );
    }

    pub fn get(&self, program_id: &Pubkey) -> Option<&RegisteredProgram> {
        let key: [u8; 32] = (*program_id).into();
        self.programs.get(&key)
    }

    pub fn contains(&self, program_id: &Pubkey) -> bool {
        let key: [u8; 32] = (*program_id).into();
        self.programs.contains_key(&key)
    }

    pub fn list(&self) -> Vec<Pubkey> {
        self.programs.values().map(|p| p.program_id).collect()
    }
}
