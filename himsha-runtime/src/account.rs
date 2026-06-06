use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{error::ProgramError, pubkey::Pubkey, utxo::UtxoMeta};

/// Lifecycle state for token / data accounts.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Default,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
)]
pub enum AccountState {
    #[default]
    Uninitialized,
    Initialized,
    Frozen,
}

/// Describes how a program account is accessed within one instruction.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct AccountMeta {
    pub pubkey: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
}

impl AccountMeta {
    pub fn writable(pubkey: Pubkey, is_signer: bool) -> Self {
        Self {
            pubkey,
            is_signer,
            is_writable: true,
        }
    }
    pub fn readonly(pubkey: Pubkey, is_signer: bool) -> Self {
        Self {
            pubkey,
            is_signer,
            is_writable: false,
        }
    }
}

/// A live account as seen by a running program.
///
/// HIMSHA's account model is hybrid: the key-value state lives in every
/// validator's local database, but each account can be anchored to a real
/// Bitcoin UTXO that ensures the state can be independently reconstructed
/// from the Bitcoin chain even if all validators disappear.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct AccountInfo {
    /// Globally unique identifier (32-byte hash).
    pub key: Pubkey,
    /// Native token balance used for fees and rent.
    pub lamports: u64,
    /// Raw program-specific data (program decides the layout via borsh).
    pub data: Vec<u8>,
    /// Program that can write to `data`.  System program can transfer ownership.
    pub owner: Pubkey,
    /// True only for deployed program accounts — `data` contains ELF bytes.
    pub executable: bool,
    /// Transient (per-instruction) flag: did this account authorize the
    /// instruction? Set by the node from the instruction's `AccountMeta`, or by
    /// a calling program via [`crate::cpi::invoke_signed_indexed`]. Not persisted.
    pub is_signer: bool,
    /// Transient (per-instruction) flag: may this account's data be mutated?
    /// Set by the node from the instruction's `AccountMeta`. [`write_data`](Self::write_data)
    /// refuses to mutate a non-writable account. Not persisted. Defaults to `true`
    /// when constructed directly (so unit tests can write); the node sets it to the
    /// declared writability for real execution.
    pub is_writable: bool,
    /// Optional Bitcoin UTXO that checkpoints this account's state on L1.
    pub utxo: Option<UtxoMeta>,
}

impl AccountInfo {
    pub fn new(key: Pubkey, owner: Pubkey, lamports: u64, space: usize) -> Self {
        Self {
            key,
            lamports,
            data: vec![0u8; space],
            owner,
            executable: false,
            is_signer: false,
            is_writable: true,
            utxo: None,
        }
    }

    pub fn new_program(key: Pubkey, bytecode: Vec<u8>) -> Self {
        Self {
            key,
            lamports: 0,
            data: bytecode,
            owner: Pubkey::default(),
            executable: true,
            is_signer: false,
            is_writable: true,
            utxo: None,
        }
    }

    pub fn with_utxo(mut self, utxo: UtxoMeta) -> Self {
        self.utxo = Some(utxo);
        self
    }

    /// Builder: mark this account as having signed the instruction (for tests
    /// and CPI authority simulation).
    pub fn as_signer(mut self) -> Self {
        self.is_signer = true;
        self
    }

    /// Builder: mark this account read-only (for tests of writable enforcement).
    pub fn as_readonly(mut self) -> Self {
        self.is_writable = false;
        self
    }

    /// Require that this account signed the instruction.
    pub fn require_signer(&self) -> Result<(), ProgramError> {
        if self.is_signer {
            Ok(())
        } else {
            Err(ProgramError::MissingSigner)
        }
    }

    pub fn read_data<T: BorshDeserialize>(&self) -> Result<T, ProgramError> {
        T::try_from_slice(&self.data).map_err(|_| ProgramError::BorshError)
    }

    /// Serialize `value` into this account's `data`. Refuses to mutate a
    /// non-writable account (the node marks accounts writable per the instruction's
    /// `AccountMeta`), closing the "any program can write any account" hole.
    pub fn write_data<T: BorshSerialize>(&mut self, value: &T) -> Result<(), ProgramError> {
        if !self.is_writable {
            return Err(ProgramError::NotWritable);
        }
        self.data = borsh::to_vec(value).map_err(|_| ProgramError::BorshError)?;
        Ok(())
    }
}

/// Compact form stored in redb (avoids storing the key twice).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct StoredAccount {
    pub lamports: u64,
    pub data: Vec<u8>,
    pub owner: [u8; 32],
    pub executable: bool,
    pub utxo_txid: Option<[u8; 32]>,
    pub utxo_vout: Option<u32>,
}

impl From<&AccountInfo> for StoredAccount {
    fn from(a: &AccountInfo) -> Self {
        Self {
            lamports: a.lamports,
            data: a.data.clone(),
            owner: a.owner.into(),
            executable: a.executable,
            utxo_txid: a.utxo.map(|u| u.txid),
            utxo_vout: a.utxo.map(|u| u.vout),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writable_account_can_be_written() {
        let mut a = AccountInfo::new(Pubkey::default(), Pubkey::default(), 0, 0);
        assert!(a.write_data(&42u64).is_ok());
    }

    #[test]
    fn readonly_account_write_is_rejected() {
        let mut a = AccountInfo::new(Pubkey::default(), Pubkey::default(), 0, 0).as_readonly();
        assert_eq!(a.write_data(&42u64), Err(ProgramError::NotWritable));
    }
}

impl StoredAccount {
    pub fn into_account(self, key: Pubkey) -> AccountInfo {
        AccountInfo {
            key,
            lamports: self.lamports,
            data: self.data,
            owner: Pubkey::from(self.owner),
            executable: self.executable,
            is_signer: false,  // transient; set per-instruction by the node
            is_writable: true, // transient; node overrides per-instruction from AccountMeta
            utxo: self
                .utxo_txid
                .zip(self.utxo_vout)
                .map(|(txid, vout)| UtxoMeta { txid, vout }),
        }
    }
}
