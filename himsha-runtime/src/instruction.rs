use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{account::AccountMeta, pubkey::Pubkey};

/// A single operation sent to one program within a transaction.
///
/// Programs decode `data` themselves using borsh.  The convention is to
/// put a 1-byte discriminant at the start identifying the variant.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct Instruction {
    pub program_id: Pubkey,
    pub accounts: Vec<AccountMeta>,
    pub data: Vec<u8>,
}

impl Instruction {
    pub fn new(program_id: Pubkey, accounts: Vec<AccountMeta>, data: Vec<u8>) -> Self {
        Self {
            program_id,
            accounts,
            data,
        }
    }

    /// Convenience constructor: borsh-encodes `args` as the instruction data.
    pub fn with_args<T: BorshSerialize>(
        program_id: Pubkey,
        accounts: Vec<AccountMeta>,
        args: &T,
    ) -> Self {
        Self {
            program_id,
            accounts,
            data: borsh::to_vec(args).expect("borsh encode instruction"),
        }
    }
}
