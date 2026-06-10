use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

/// Errors returned by on-chain programs during execution inside the zkVM.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Error)]
pub enum ProgramError {
    #[error("custom({0:#x})")]
    Custom(u32),
    #[error("bad instruction data")]
    InvalidInstruction,
    #[error("bad account data")]
    InvalidAccountData,
    #[error("account too small")]
    AccountDataTooSmall,
    #[error("insufficient lamports")]
    InsufficientFunds,
    #[error("wrong program id")]
    WrongProgram,
    #[error("already initialized")]
    AlreadyInitialized,
    #[error("not initialized")]
    NotInitialized,
    #[error("missing accounts")]
    NotEnoughAccounts,
    #[error("account not writable")]
    NotWritable,
    #[error("missing signature")]
    MissingSigner,
    #[error("arithmetic overflow")]
    Overflow,
    #[error("invalid UTXO")]
    InvalidUtxo,
    #[error("unauthorized")]
    Unauthorized,
    #[error("CPI depth exceeded")]
    CpiDepthExceeded,
    #[error("borsh error")]
    BorshError,
    #[error("loan not expired")]
    LoanNotExpired,
    #[error("loan expired")]
    LoanExpired,
    #[error("slippage exceeded")]
    SlippageExceeded,
    #[error("pool empty")]
    PoolEmpty,
    #[error("position undercollateralized")]
    Undercollateralized,
    #[error("insufficient liquidity")]
    InsufficientLiquidity,
    #[error("stale or missing price")]
    StalePrice,
    #[error("write to an account the program does not own")]
    IllegalOwnerWrite,
    #[error("price moves more than the feed's deviation bound")]
    PriceOutOfBounds,
    #[error("compute budget exhausted")]
    ComputeExhausted,
    #[error("the same account appears writable more than once")]
    DuplicateWritableAccount,
}

impl From<std::io::Error> for ProgramError {
    fn from(_: std::io::Error) -> Self {
        ProgramError::BorshError
    }
}

impl ProgramError {
    pub fn custom(code: u32) -> Self {
        Self::Custom(code)
    }
}

/// Node-level errors (outside the zkVM).
#[derive(Debug, Error)]
pub enum NodeError {
    #[error("program not found: {0}")]
    ProgramNotFound(String),
    #[error("account not found: {0}")]
    AccountNotFound(String),
    #[error("invalid transaction: {0}")]
    InvalidTransaction(String),
    #[error("proof verification failed")]
    ProofInvalid,
    #[error("VM error: {0}")]
    VmError(String),
    #[error("bitcoin error: {0}")]
    BitcoinError(String),
    #[error("storage error: {0}")]
    StorageError(String),
    #[error("serialization error: {0}")]
    SerError(String),
    #[error("rpc error: {0}")]
    RpcError(String),
}
