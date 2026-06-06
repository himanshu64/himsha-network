//! Universal HIMSHA guest.
//!
//! Reads `program_id` then [`ExecutionInput`] from the host, dispatches to the
//! matching built-in program's `process()`, and commits [`ExecutionOutput`] to
//! the journal. Because every built-in shares this one guest, they all share a
//! single image id (see `himsha-vm::zk`).
//!
//! NOTE: this dispatch table mirrors `himsha-vm::dispatch`; the guest can't depend
//! on `himsha-vm` (which links the RISC Zero host), so the match is duplicated.

#![no_main]

use risc0_zkvm::guest::env;

use himsha_runtime::{
    account::AccountInfo,
    error::ProgramError,
    exec::{ExecutionInput, ExecutionOutput},
    program_ids,
    pubkey::Pubkey,
};

risc0_zkvm::guest::entry!(main);

fn dispatch(
    program_id: &Pubkey,
    accounts: &mut [AccountInfo],
    data: &[u8],
    timestamp: u64,
) -> Result<(), ProgramError> {
    let id = *program_id;
    if id == program_ids::system_program() {
        himsha_system_program::process(accounts, data)
    } else if id == program_ids::token_program() {
        himsha_token_program::process(accounts, data)
    } else if id == program_ids::ata_program() {
        himsha_ata_program::process(accounts, data)
    } else if id == program_ids::swap_program() {
        himsha_swap_program::process(accounts, data)
    } else if id == program_ids::nft_metadata_program() {
        himsha_nft_metadata_program::process(accounts, data)
    } else if id == program_ids::lending_program() {
        himsha_lending_program::process(accounts, data, timestamp)
    } else if id == program_ids::runes_program() {
        himsha_runes_program::process(accounts, data, timestamp)
    } else if id == program_ids::money_market_program() {
        himsha_money_market_program::process(accounts, data, timestamp)
    } else if id == program_ids::vault_program() {
        himsha_vault_program::process(accounts, data, timestamp)
    } else if id == program_ids::oracle_program() {
        himsha_oracle_program::process(accounts, data, timestamp)
    } else {
        Err(ProgramError::Custom(0x4040))
    }
}

pub fn main() {
    let program_id: Pubkey = env::read();
    let input: ExecutionInput = env::read();

    let mut accounts = input.accounts;
    // A program error makes the proof fail — exactly the desired behavior: an
    // invalid state transition simply has no valid receipt.
    dispatch(&program_id, &mut accounts, &input.instruction_data, input.timestamp)
        .expect("program execution failed");

    let output = ExecutionOutput {
        updated_accounts: accounts,
        unsigned_bitcoin_tx: None,
        logs: Vec::new(),
    };
    env::commit(&output);
}
