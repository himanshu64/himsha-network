//! HIMSHA Runtime — core types shared across the entire HIMSHA Network stack.
//!
//! HIMSHA (Hashable Instruction Machine) is a ZK-proven Bitcoin programmability
//! layer. Every state transition is backed by a RISC Zero receipt, so
//! correctness is guaranteed by cryptographic proof — not validator majority.

pub mod account;
pub mod compute;
pub mod cpi;
pub mod error;
pub mod exec;
pub mod instruction;
pub mod merkle;
pub mod owner;
pub mod pubkey;
pub mod receipt;
pub mod signature;
pub mod transaction;
pub mod utxo;

pub use account::{AccountInfo, AccountMeta, AccountState, StoredAccount};
pub use error::{NodeError, ProgramError};
pub use instruction::Instruction;
pub use pubkey::Pubkey;
pub use receipt::{ExecutionReceipt, StateTransition};
pub use signature::Signature;
pub use transaction::{Block, Message, RuntimeTransaction};
pub use utxo::{UtxoInfo, UtxoMeta};

/// Well-known program IDs (set at genesis).
pub mod program_ids {
    use super::Pubkey;

    /// Creates and manages accounts.
    pub fn system_program() -> Pubkey {
        Pubkey::from_seed(b"himsha::system_program")
    }

    /// SPL-compatible fungible token program.
    pub fn token_program() -> Pubkey {
        Pubkey::from_seed(b"himsha::token_program")
    }

    /// Associated token account program.
    pub fn ata_program() -> Pubkey {
        Pubkey::from_seed(b"himsha::ata_program")
    }

    /// Constant-product AMM (x * y = k).
    pub fn swap_program() -> Pubkey {
        Pubkey::from_seed(b"himsha::swap_program")
    }

    /// Bitcoin ordinals / inscription lending.
    pub fn lending_program() -> Pubkey {
        Pubkey::from_seed(b"himsha::lending_program")
    }

    /// On-chain NFT metadata.
    pub fn nft_metadata_program() -> Pubkey {
        Pubkey::from_seed(b"himsha::nft_metadata_program")
    }

    /// Bitcoin Runes fungible-token protocol (etch / mint / transfer / burn).
    pub fn runes_program() -> Pubkey {
        Pubkey::from_seed(b"himsha::runes_program")
    }

    /// Over-collateralized money market (supply / borrow / repay / liquidate).
    pub fn money_market_program() -> Pubkey {
        Pubkey::from_seed(b"himsha::money_market_program")
    }

    /// Automated yield vault (ERC-4626-style shares over a strategy).
    pub fn vault_program() -> Pubkey {
        Pubkey::from_seed(b"himsha::vault_program")
    }

    /// Signed price-feed oracle.
    pub fn oracle_program() -> Pubkey {
        Pubkey::from_seed(b"himsha::oracle_program")
    }

    /// Every program ID that the node pre-registers at genesis.
    pub fn builtins() -> [Pubkey; 10] {
        [
            system_program(),
            token_program(),
            ata_program(),
            swap_program(),
            lending_program(),
            nft_metadata_program(),
            runes_program(),
            money_market_program(),
            vault_program(),
            oracle_program(),
        ]
    }
}
