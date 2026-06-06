//! Shared zkVM execution I/O types.
//!
//! These live in `himsha-runtime` (not `himsha-vm`) so the RISC Zero **guest** can use
//! them without depending on the prover/host crate. The host writes
//! `program_id` then [`ExecutionInput`] into the guest; the guest commits
//! [`ExecutionOutput`] to its journal.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::account::AccountInfo;

/// Data fed into the zkVM guest before execution.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct ExecutionInput {
    pub accounts:         Vec<AccountInfo>,
    pub instruction_data: Vec<u8>,
    pub timestamp:        u64,
}

/// Data emitted from the zkVM guest's journal after execution.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct ExecutionOutput {
    pub updated_accounts:    Vec<AccountInfo>,
    pub unsigned_bitcoin_tx: Option<Vec<u8>>,
    pub logs:                Vec<String>,
}
