use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---- CLI definition ----

/// HIMSHA Network — ZK-proven Bitcoin programmability layer
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Node RPC endpoint
    #[arg(long, default_value = "http://127.0.0.1:9100", global = true)]
    rpc_url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deploy a compiled ELF binary as a new HIMSHA program
    Deploy {
        /// Path to the ELF file
        #[arg(long)]
        elf: PathBuf,
        /// RISC Zero image ID (hex, 32 bytes)
        #[arg(long)]
        image_id: String,
    },

    /// Query account state
    Account {
        #[command(subcommand)]
        sub: AccountCmd,
    },

    /// Node management
    Node {
        #[command(subcommand)]
        sub: NodeCmd,
    },

    /// Program utilities
    Program {
        #[command(subcommand)]
        sub: ProgramCmd,
    },
}

#[derive(Subcommand)]
enum AccountCmd {
    /// Show an account's details
    Get { pubkey: String },
    /// List all accounts for a program
    List { program_id: String },
}

#[derive(Subcommand)]
enum NodeCmd {
    /// Check if the node is ready
    Status,
    /// Print the current slot
    Slot,
    /// Get a block by slot number
    Block { slot: u64 },
}

#[derive(Subcommand)]
enum ProgramCmd {
    /// List all deployed programs
    List,
    /// Query UTXO info
    Utxo { txid: String, vout: u32 },
    /// Scaffold a new HIMSHA program crate (a ready-to-build counter template)
    New {
        /// Program name (kebab-case), e.g. `escrow` → crate `himsha-escrow-program`
        name: String,
        /// Directory to create the crate in (default: ./himsha-programs)
        #[arg(long, default_value = "himsha-programs")]
        dir: PathBuf,
    },
}

// ---- RPC client helpers ----

#[derive(Serialize)]
struct RpcRequest<'a, P: Serialize> {
    jsonrpc: &'a str,
    id:      u32,
    method:  &'a str,
    params:  P,
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error:  Option<serde_json::Value>,
}

async fn rpc_call<P: Serialize, R: for<'de> Deserialize<'de>>(
    url: &str,
    method: &str,
    params: P,
) -> Result<R> {
    let client = reqwest::Client::new();
    let req = RpcRequest { jsonrpc: "2.0", id: 1, method, params };
    let resp: RpcResponse<R> = client
        .post(url)
        .json(&req)
        .send()
        .await?
        .json()
        .await?;
    if let Some(err) = resp.error {
        anyhow::bail!("RPC error: {err}");
    }
    resp.result.ok_or_else(|| anyhow::anyhow!("null result"))
}

// ---- main ----

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let url = &cli.rpc_url;

    match cli.command {
        Commands::Deploy { elf, image_id } => {
            let bytes = std::fs::read(&elf)?;
            let elf_hex = hex::encode(&bytes);
            let program_id: String = rpc_call(url, "himsha_deployProgram", (elf_hex, image_id)).await?;
            println!("deployed program: {program_id}");
        }

        Commands::Account { sub } => match sub {
            AccountCmd::Get { pubkey } => {
                let info: Option<serde_json::Value> =
                    rpc_call(url, "himsha_getAccountInfo", (pubkey,)).await?;
                match info {
                    Some(v) => println!("{}", serde_json::to_string_pretty(&v)?),
                    None    => println!("account not found"),
                }
            }
            AccountCmd::List { program_id } => {
                let accounts: Vec<serde_json::Value> =
                    rpc_call(url, "himsha_getProgramAccounts", (program_id,)).await?;
                for a in accounts {
                    println!("{}", serde_json::to_string_pretty(&a)?);
                }
            }
        },

        Commands::Node { sub } => match sub {
            NodeCmd::Status => {
                let ready: bool = rpc_call(url, "himsha_isNodeReady", ()).await?;
                println!("node ready: {ready}");
            }
            NodeCmd::Slot => {
                let slot: u64 = rpc_call(url, "himsha_getSlot", ()).await?;
                println!("current slot: {slot}");
            }
            NodeCmd::Block { slot } => {
                let block: Option<serde_json::Value> =
                    rpc_call(url, "himsha_getBlock", (slot,)).await?;
                match block {
                    Some(b) => println!("{}", serde_json::to_string_pretty(&b)?),
                    None    => println!("block not found"),
                }
            }
        },

        Commands::Program { sub } => match sub {
            ProgramCmd::List => {
                let programs: Vec<String> = rpc_call(url, "himsha_listPrograms", ()).await?;
                if programs.is_empty() {
                    println!("no programs deployed");
                } else {
                    for p in programs { println!("  {p}"); }
                }
            }
            ProgramCmd::Utxo { txid, vout } => {
                let info: Option<serde_json::Value> =
                    rpc_call(url, "himsha_getUtxo", (txid, vout)).await?;
                match info {
                    Some(v) => println!("{}", serde_json::to_string_pretty(&v)?),
                    None    => println!("UTXO not found or spent"),
                }
            }
            ProgramCmd::New { name, dir } => scaffold_program(&name, &dir)?,
        },
    }

    Ok(())
}

/// Scaffold a new program crate (`himsha-<name>-program`) under `dir` from a
/// minimal, compiling counter template — the "new program" on-ramp that improves
/// the developer story around the native/RISC-Zero runtime.
fn scaffold_program(name: &str, dir: &PathBuf) -> Result<()> {
    let slug = name.trim().to_lowercase().replace([' ', '_'], "-");
    if slug.is_empty() || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        anyhow::bail!("invalid program name '{name}' (use kebab-case: letters, digits, '-')");
    }
    let crate_name = format!("himsha-{slug}-program");
    let mod_name = slug.replace('-', "_");
    let crate_dir = dir.join(&slug);
    if crate_dir.exists() {
        anyhow::bail!("{} already exists", crate_dir.display());
    }
    std::fs::create_dir_all(crate_dir.join("src"))?;

    let cargo_toml = format!(
        r#"[package]
name    = "{crate_name}"
version = "0.1.0"
edition = "2021"
description = "HIMSHA {slug} program"

[dependencies]
himsha-runtime = {{ workspace = true }}
borsh       = {{ workspace = true }}
serde       = {{ workspace = true }}
"#
    );

    let lib_rs = format!(
        r#"//! HIMSHA `{slug}` program (scaffolded by `himsha program new`).
//!
//! A minimal counter to start from: an account holds a `count` and an `authority`;
//! `Initialize` sets the authority, `Increment` bumps the count (authority must sign).
//! Replace the state/instructions with your own — the `process` entrypoint shape is
//! what the runtime (native dispatch / RISC Zero guest) calls.

use borsh::{{BorshDeserialize, BorshSerialize}};
use himsha_runtime::{{
    account::AccountInfo,
    error::ProgramError,
    pubkey::Pubkey,
}};

// ---- state ----

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct Counter {{
    pub authority: Pubkey,
    pub count: u64,
    pub is_initialized: bool,
}}

// ---- instructions ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum {Mod}Instruction {{
    /// Initialize the counter, setting `accounts[1]` as the authority.
    /// accounts: [0] = counter (writable), [1] = authority (signer).
    Initialize,
    /// Increment the counter by `amount`.
    /// accounts: [0] = counter (writable), [1] = authority (signer).
    Increment {{ amount: u64 }},
}}

// ---- entrypoint ----

pub fn process(accounts: &mut [AccountInfo], data: &[u8]) -> Result<(), ProgramError> {{
    let ix = {Mod}Instruction::try_from_slice(data).map_err(|_| ProgramError::InvalidInstruction)?;
    match ix {{
        {Mod}Instruction::Initialize => {{
            if accounts.len() < 2 {{ return Err(ProgramError::NotEnoughAccounts); }}
            let mut counter: Counter = accounts[0].read_data().unwrap_or_default();
            if counter.is_initialized {{ return Err(ProgramError::AlreadyInitialized); }}
            if !accounts[1].is_signer {{ return Err(ProgramError::MissingSigner); }}
            counter.authority = accounts[1].key;
            counter.count = 0;
            counter.is_initialized = true;
            accounts[0].write_data(&counter)
        }}
        {Mod}Instruction::Increment {{ amount }} => {{
            if accounts.len() < 2 {{ return Err(ProgramError::NotEnoughAccounts); }}
            let mut counter: Counter = accounts[0].read_data()?;
            if !counter.is_initialized {{ return Err(ProgramError::NotInitialized); }}
            if !accounts[1].is_signer || accounts[1].key != counter.authority {{
                return Err(ProgramError::Unauthorized);
            }}
            counter.count = counter.count.checked_add(amount).ok_or(ProgramError::Overflow)?;
            accounts[0].write_data(&counter)
        }}
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;
    use himsha_runtime::program_ids::system_program;

    fn account(key: Pubkey, signer: bool) -> AccountInfo {{
        let mut a = AccountInfo::new(key, system_program(), 0, 256);
        a.is_signer = signer;
        a
    }}

    #[test]
    fn test_initialize_then_increment() {{
        let counter_key = Pubkey::from_seed(b"counter");
        let auth = Pubkey::from_seed(b"authority");
        let mut accts = vec![account(counter_key, false), account(auth, true)];

        let init = borsh::to_vec(&{Mod}Instruction::Initialize).unwrap();
        process(&mut accts, &init).unwrap();

        let inc = borsh::to_vec(&{Mod}Instruction::Increment {{ amount: 5 }}).unwrap();
        process(&mut accts, &inc).unwrap();

        let counter: Counter = accts[0].read_data().unwrap();
        assert_eq!(counter.count, 5);
        assert_eq!(counter.authority, auth);
    }}

    #[test]
    fn test_increment_requires_authority() {{
        let counter_key = Pubkey::from_seed(b"counter");
        let auth = Pubkey::from_seed(b"authority");
        let imposter = Pubkey::from_seed(b"imposter");
        let mut accts = vec![account(counter_key, false), account(auth, true)];
        process(&mut accts, &borsh::to_vec(&{Mod}Instruction::Initialize).unwrap()).unwrap();

        // Wrong signer → rejected.
        accts[1] = account(imposter, true);
        let inc = borsh::to_vec(&{Mod}Instruction::Increment {{ amount: 1 }}).unwrap();
        assert!(matches!(process(&mut accts, &inc), Err(ProgramError::Unauthorized)));
    }}
}}
"#,
        Mod = to_pascal(&mod_name),
    );

    std::fs::write(crate_dir.join("Cargo.toml"), cargo_toml)?;
    std::fs::write(crate_dir.join("src").join("lib.rs"), lib_rs)?;

    println!("✅ scaffolded {crate_name} at {}", crate_dir.display());
    println!();
    println!("next steps:");
    println!("  1. add it to the workspace members in the root Cargo.toml:");
    println!("       \"{}\",", crate_dir.display());
    println!("  2. build + test it:");
    println!("       cargo test -p {crate_name}");
    println!("  3. wire it into the node: add a program id in himsha-runtime");
    println!("     (program_ids) and dispatch to `{mod_name}::process` in himsha-vm.");
    Ok(())
}

/// kebab/snake → PascalCase for the instruction enum name.
fn to_pascal(s: &str) -> String {
    s.split(['-', '_'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}
