use anyhow::Result;
use himsha_runtime::{
    error::NodeError,
    pubkey::Pubkey,
    receipt::{ExecutionReceipt, StateTransition},
    utxo::UtxoMeta,
};
use risc0_zkvm::{default_prover, ExecutorEnv};
use sha2::{Digest, Sha256};
use tracing::{debug, info};

use crate::{dispatch, registry::ProgramRegistry};

// Shared zkVM I/O types now live in himsha-runtime so the guest can use them too.
pub use himsha_runtime::exec::{ExecutionInput, ExecutionOutput};

/// Executes HIMSHA programs inside the RISC Zero zkVM and returns verified state transitions.
pub struct ProgramExecutor<'a> {
    registry: &'a ProgramRegistry,
}

impl<'a> ProgramExecutor<'a> {
    pub fn new(registry: &'a ProgramRegistry) -> Self {
        Self { registry }
    }

    /// Execute a program and return its `StateTransition`.
    ///
    /// Default build: built-in programs run natively (see [`crate::dispatch`]) and
    /// everything else goes through the zkVM prover. With the `zkvm` feature enabled
    /// *all* programs — including built-ins, via the universal guest — are ZK-proven.
    /// This is the entry point the node should call.
    pub fn execute_program(
        &self,
        program_id: &Pubkey,
        input: ExecutionInput,
        anchor_utxos: Vec<UtxoMeta>,
    ) -> Result<StateTransition, NodeError> {
        #[cfg(feature = "zkvm")]
        {
            return self.execute(program_id, input, anchor_utxos);
        }
        #[cfg(not(feature = "zkvm"))]
        {
            if dispatch::is_builtin(program_id) {
                self.execute_native(program_id, input, anchor_utxos)
            } else {
                self.execute(program_id, input, anchor_utxos)
            }
        }
    }

    /// Run a built-in program natively (no proof) and return its `StateTransition`.
    ///
    /// The receipt is marked `verified: false` and its `journal_hash` is the SHA-256
    /// of the borsh-encoded updated accounts, so callers can still detect tampering
    /// of the recorded state even though no ZK proof was produced.
    pub fn execute_native(
        &self,
        program_id: &Pubkey,
        input: ExecutionInput,
        anchor_utxos: Vec<UtxoMeta>,
    ) -> Result<StateTransition, NodeError> {
        let mut accounts = input.accounts;
        dispatch::dispatch(program_id, &mut accounts, &input.instruction_data, input.timestamp)
            .map_err(|e| NodeError::VmError(format!("program error: {e}")))?;

        let state_bytes = borsh::to_vec(&accounts)
            .map_err(|e| NodeError::SerError(e.to_string()))?;
        let journal_hash: [u8; 32] = Sha256::digest(&state_bytes).into();

        debug!("native execution of program {} ({} accounts)", program_id, accounts.len());

        Ok(StateTransition {
            receipt: ExecutionReceipt {
                program_id: *program_id,
                image_id: [0u8; 32], // no ELF/image for native execution
                journal_hash,
                proof_bytes: Vec::new(),
                verified: false, // native run — not ZK-proven
            },
            updated_accounts: accounts,
            new_utxos: anchor_utxos,
            bitcoin_txid: None,
        })
    }

    /// Run a program inside the zkVM and return a *verified* `StateTransition`.
    pub fn execute(
        &self,
        program_id: &Pubkey,
        input: ExecutionInput,
        anchor_utxos: Vec<UtxoMeta>,
    ) -> Result<StateTransition, NodeError> {
        let prog = self
            .registry
            .get(program_id)
            .ok_or_else(|| NodeError::ProgramNotFound(program_id.to_string()))?;

        // The universal guest reads the program id first, then the execution
        // input, both via risc0 serde (`env::read()`), so it can dispatch.
        let mut builder = ExecutorEnv::builder();
        builder
            .write(program_id)
            .map_err(|e| NodeError::VmError(e.to_string()))?;
        builder
            .write(&input)
            .map_err(|e| NodeError::VmError(e.to_string()))?;
        let env = builder
            .build()
            .map_err(|e| NodeError::VmError(e.to_string()))?;

        info!("executing program {} in zkVM", program_id);

        // risc0-zkvm 0.19: prove_elf returns Receipt directly
        let receipt = default_prover()
            .prove_elf(env, &prog.elf)
            .map_err(|e: anyhow::Error| NodeError::VmError(e.to_string()))?;

        receipt
            .verify(prog.image_id)
            .map_err(|e| NodeError::VmError(format!("{e:?}")))?;

        debug!("receipt verified for program {}", program_id);

        let output: ExecutionOutput = receipt
            .journal
            .decode()
            .map_err(|e| NodeError::VmError(e.to_string()))?;

        let journal_hash: [u8; 32] = Sha256::digest(&receipt.journal.bytes).into();

        let exec_receipt = ExecutionReceipt {
            program_id: *program_id,
            image_id:   prog.image_id,
            journal_hash,
            proof_bytes: receipt.journal.bytes.clone(), // store journal bytes as proof ref
            verified:   true,
        };

        Ok(StateTransition {
            receipt:          exec_receipt,
            updated_accounts: output.updated_accounts,
            new_utxos:        anchor_utxos,
            bitcoin_txid:     None,
        })
    }
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ProgramRegistry;
    use himsha_runtime::{account::AccountInfo, program_ids, pubkey::Pubkey};

    // A system Transfer of 250 lamports, ready to feed the executor.
    fn transfer_input() -> (Pubkey, ExecutionInput) {
        let sys = program_ids::system_program();
        let accounts = vec![
            AccountInfo::new(Pubkey::from_seed(b"from"), sys, 1_000, 0).as_signer(),
            AccountInfo::new(Pubkey::from_seed(b"to"), sys, 0, 0),
        ];
        let data = borsh::to_vec(
            &himsha_system_program::SystemInstruction::Transfer { lamports: 250 },
        ).unwrap();
        (sys, ExecutionInput { accounts, instruction_data: data, timestamp: 0 })
    }

    #[test]
    fn test_execute_native_runs_builtin_without_proof() {
        let reg = ProgramRegistry::new();
        let ex = ProgramExecutor::new(&reg);
        let (sys, input) = transfer_input();
        let st = ex.execute_native(&sys, input, vec![]).unwrap();

        assert!(!st.receipt.verified); // native execution is not ZK-proven
        assert_eq!(st.updated_accounts[0].lamports, 750);
        assert_eq!(st.updated_accounts[1].lamports, 250);
    }

    #[test]
    fn test_execute_program_routes_builtin_to_native() {
        // Registry has no ELF, yet a built-in still executes (native path).
        let reg = ProgramRegistry::new();
        let ex = ProgramExecutor::new(&reg);
        let (sys, input) = transfer_input();
        let st = ex.execute_program(&sys, input, vec![]).unwrap();
        assert_eq!(st.updated_accounts[1].lamports, 250);
    }

    #[test]
    fn test_execute_native_surfaces_program_error() {
        // No signer on the source account → the program's error propagates as VmError.
        let sys = program_ids::system_program();
        let accounts = vec![
            AccountInfo::new(Pubkey::from_seed(b"from"), sys, 1_000, 0), // not a signer
            AccountInfo::new(Pubkey::from_seed(b"to"), sys, 0, 0),
        ];
        let data = borsh::to_vec(
            &himsha_system_program::SystemInstruction::Transfer { lamports: 100 },
        ).unwrap();
        let input = ExecutionInput { accounts, instruction_data: data, timestamp: 0 };
        let reg = ProgramRegistry::new();
        let ex = ProgramExecutor::new(&reg);
        assert!(ex.execute_native(&sys, input, vec![]).is_err());
    }
}
