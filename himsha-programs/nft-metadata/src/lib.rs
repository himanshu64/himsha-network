//! HIMSHA NFT Metadata Program
//!
//! Stores human-readable metadata for tokens and inscriptions:
//! name, symbol, URI (pointing to off-chain JSON), royalties, creators.

use borsh::{BorshDeserialize, BorshSerialize};
use himsha_runtime::{
    account::{AccountInfo, AccountMeta},
    error::ProgramError,
    instruction::Instruction,
    pubkey::Pubkey,
};

// ---- state ----

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct Creator {
    pub address: Pubkey,
    /// 0-100, all creators must sum to 100.
    pub share: u8,
    pub verified: bool,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct Metadata {
    /// The mint this metadata describes.
    pub mint: Pubkey,
    /// Who can update this record.
    pub update_authority: Pubkey,
    /// Display name (max 32 chars).
    pub name: String,
    /// Ticker symbol (max 10 chars).
    pub symbol: String,
    /// HTTPS URI to off-chain JSON (max 200 chars).
    pub uri: String,
    /// Royalty basis points (e.g. 500 = 5 %).
    pub seller_fee_basis_points: u16,
    pub creators: Vec<Creator>,
    pub is_mutable: bool,
}

// ---- instructions ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum MetadataInstruction {
    /// Create a new metadata account.
    Create {
        name: String,
        symbol: String,
        uri: String,
        seller_fee_basis_points: u16,
        creators: Vec<Creator>,
    },
    /// Update mutable metadata.
    Update {
        name: Option<String>,
        uri: Option<String>,
        seller_fee_basis_points: Option<u16>,
    },
    /// Verify a creator (signs their own creator entry).
    VerifyCreator,
    /// Freeze: make metadata immutable.
    MakeImmutable,
}

// ---- builders ----

pub fn create_metadata(
    metadata_account: Pubkey,
    mint: Pubkey,
    update_authority: Pubkey,
    name: String, symbol: String, uri: String,
    seller_fee_basis_points: u16,
    creators: Vec<Creator>,
) -> Instruction {
    assert!(name.len() <= 32, "name too long");
    assert!(symbol.len() <= 10, "symbol too long");
    assert!(uri.len() <= 200, "URI too long");
    Instruction::with_args(
        himsha_runtime::program_ids::nft_metadata_program(),
        vec![
            AccountMeta::writable(metadata_account, false),
            AccountMeta::readonly(mint, false),
            AccountMeta::readonly(update_authority, true),
        ],
        &MetadataInstruction::Create { name, symbol, uri, seller_fee_basis_points, creators },
    )
}

pub fn update_metadata(
    metadata_account: Pubkey,
    update_authority: Pubkey,
    name: Option<String>, uri: Option<String>,
    seller_fee_basis_points: Option<u16>,
) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::nft_metadata_program(),
        vec![
            AccountMeta::writable(metadata_account, false),
            AccountMeta::readonly(update_authority, true),
        ],
        &MetadataInstruction::Update { name, uri, seller_fee_basis_points },
    )
}

// ---- processing ----

pub fn process(accounts: &mut [AccountInfo], data: &[u8]) -> Result<(), ProgramError> {
    let ix = MetadataInstruction::try_from_slice(data)
        .map_err(|_| ProgramError::InvalidInstruction)?;

    match ix {
        MetadataInstruction::Create { name, symbol, uri, seller_fee_basis_points, creators } => {
            let mint_key      = accounts[1].key;
            let authority_key = accounts[2].key;

            let meta_acc = &mut accounts[0];
            let existing: Option<Metadata> = meta_acc.read_data().ok();
            if existing.map(|m| !m.name.is_empty()).unwrap_or(false) {
                return Err(ProgramError::AlreadyInitialized);
            }
            let creator_share_sum: u16 = creators.iter().map(|c| c.share as u16).sum();
            if !creators.is_empty() && creator_share_sum != 100 {
                return Err(ProgramError::InvalidInstruction);
            }
            let meta = Metadata {
                mint: mint_key,
                update_authority: authority_key,
                name, symbol, uri,
                seller_fee_basis_points,
                creators,
                is_mutable: true,
            };
            meta_acc.write_data(&meta)?;
        }

        MetadataInstruction::Update { name, uri, seller_fee_basis_points } => {
            let mut meta: Metadata = accounts[0].read_data()?;
            if !meta.is_mutable { return Err(ProgramError::Unauthorized); }
            if accounts[1].key != meta.update_authority { return Err(ProgramError::Unauthorized); }
            if let Some(n) = name { meta.name = n; }
            if let Some(u) = uri  { meta.uri  = u; }
            if let Some(f) = seller_fee_basis_points { meta.seller_fee_basis_points = f; }
            accounts[0].write_data(&meta)?;
        }

        MetadataInstruction::VerifyCreator => {
            let mut meta: Metadata = accounts[0].read_data()?;
            let signer_key = accounts[1].key;
            for c in meta.creators.iter_mut() {
                if c.address == signer_key { c.verified = true; }
            }
            accounts[0].write_data(&meta)?;
        }

        MetadataInstruction::MakeImmutable => {
            let mut meta: Metadata = accounts[0].read_data()?;
            if accounts[1].key != meta.update_authority { return Err(ProgramError::Unauthorized); }
            meta.is_mutable = false;
            accounts[0].write_data(&meta)?;
        }
    }

    Ok(())
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn prog() -> Pubkey { himsha_runtime::program_ids::nft_metadata_program() }

    fn acct(seed: &str, space: usize) -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(seed.as_bytes()), prog(), 0, space)
    }

    /// [0] = metadata, [1] = mint, [2] = update authority.
    fn create_accounts() -> Vec<AccountInfo> {
        vec![acct("meta", 0), acct("mint", 0), acct("authority", 0)]
    }

    fn do_create(accounts: &mut [AccountInfo], creators: Vec<Creator>) -> Result<(), ProgramError> {
        let data = borsh::to_vec(&MetadataInstruction::Create {
            name: "Frog #1".into(),
            symbol: "FROG".into(),
            uri: "https://example.com/1.json".into(),
            seller_fee_basis_points: 500,
            creators,
        }).unwrap();
        process(accounts, &data)
    }

    #[test]
    fn test_create_metadata() {
        let mut accounts = create_accounts();
        do_create(&mut accounts, vec![]).unwrap();
        let m: Metadata = accounts[0].read_data().unwrap();
        assert_eq!(m.name, "Frog #1");
        assert_eq!(m.symbol, "FROG");
        assert_eq!(m.seller_fee_basis_points, 500);
        assert_eq!(m.mint, Pubkey::from_seed(b"mint"));
        assert_eq!(m.update_authority, Pubkey::from_seed(b"authority"));
        assert!(m.is_mutable);
    }

    #[test]
    fn test_create_already_initialized_fails() {
        let mut accounts = create_accounts();
        do_create(&mut accounts, vec![]).unwrap();
        assert_eq!(do_create(&mut accounts, vec![]), Err(ProgramError::AlreadyInitialized));
    }

    #[test]
    fn test_create_bad_creator_shares_fails() {
        let mut accounts = create_accounts();
        let creators = vec![
            Creator { address: Pubkey::from_seed(b"c1"), share: 40, verified: false },
            Creator { address: Pubkey::from_seed(b"c2"), share: 40, verified: false }, // sum 80 ≠ 100
        ];
        assert_eq!(do_create(&mut accounts, creators), Err(ProgramError::InvalidInstruction));
    }

    #[test]
    fn test_create_valid_creator_shares() {
        let mut accounts = create_accounts();
        let creators = vec![
            Creator { address: Pubkey::from_seed(b"c1"), share: 60, verified: false },
            Creator { address: Pubkey::from_seed(b"c2"), share: 40, verified: false },
        ];
        do_create(&mut accounts, creators).unwrap();
        assert_eq!(accounts[0].read_data::<Metadata>().unwrap().creators.len(), 2);
    }

    #[test]
    fn test_update_changes_mutable_fields() {
        let mut accounts = create_accounts();
        do_create(&mut accounts, vec![]).unwrap();
        // Update with the correct authority ([1].key == update_authority).
        let mut upd = vec![accounts[0].clone(), acct("authority", 0)];
        let data = borsh::to_vec(&MetadataInstruction::Update {
            name: Some("Frog #1 (v2)".into()),
            uri: None,
            seller_fee_basis_points: Some(250),
        }).unwrap();
        process(&mut upd, &data).unwrap();
        let m: Metadata = upd[0].read_data().unwrap();
        assert_eq!(m.name, "Frog #1 (v2)");
        assert_eq!(m.seller_fee_basis_points, 250);
        assert_eq!(m.uri, "https://example.com/1.json"); // untouched
    }

    #[test]
    fn test_update_wrong_authority_fails() {
        let mut accounts = create_accounts();
        do_create(&mut accounts, vec![]).unwrap();
        let mut upd = vec![accounts[0].clone(), acct("intruder", 0)];
        let data = borsh::to_vec(&MetadataInstruction::Update {
            name: Some("hijack".into()), uri: None, seller_fee_basis_points: None,
        }).unwrap();
        assert_eq!(process(&mut upd, &data), Err(ProgramError::Unauthorized));
    }

    #[test]
    fn test_make_immutable_then_update_fails() {
        let mut accounts = create_accounts();
        do_create(&mut accounts, vec![]).unwrap();
        let mut mk = vec![accounts[0].clone(), acct("authority", 0)];
        process(&mut mk, &borsh::to_vec(&MetadataInstruction::MakeImmutable).unwrap()).unwrap();
        assert!(!mk[0].read_data::<Metadata>().unwrap().is_mutable);

        let data = borsh::to_vec(&MetadataInstruction::Update {
            name: Some("x".into()), uri: None, seller_fee_basis_points: None,
        }).unwrap();
        assert_eq!(process(&mut mk, &data), Err(ProgramError::Unauthorized)); // immutable
    }

    #[test]
    fn test_verify_creator() {
        let mut accounts = create_accounts();
        let c1 = Pubkey::from_seed(b"c1");
        do_create(&mut accounts, vec![Creator { address: c1, share: 100, verified: false }]).unwrap();
        // The signer ([1].key) is the creator being verified.
        let mut vc = vec![accounts[0].clone(), AccountInfo::new(c1, prog(), 0, 0)];
        process(&mut vc, &borsh::to_vec(&MetadataInstruction::VerifyCreator).unwrap()).unwrap();
        assert!(vc[0].read_data::<Metadata>().unwrap().creators[0].verified);
    }
}
