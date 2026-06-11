//! HIMSHA Ordinals Lending Program
//!
//! Borrowers pledge a Bitcoin inscription (Ordinal) as collateral and
//! receive BTC from the highest-bidding lender.  If the borrower repays
//! within the agreed period the inscription returns; otherwise the lender
//! can claim it after the deadline.
//!
//! State design (all stored in HIMSHA accounts via borsh):
//!   CollectionAccount  — one per named NFT collection
//!   LoanAccount        — one per active loan, keyed by inscription ID
//!
//! Differences from the original RISC Zero implementation:
//!   - No manual Bitcoin transaction building — the HIMSHA node handles UTXO management.
//!   - No zkVM-specific env::read / env::commit — uses HIMSHA account model.
//!   - Timestamp comes from `Message.timestamp` instead of host injection.
//!   - State lives in HIMSHA accounts, not Taproot scripts.

use borsh::{BorshDeserialize, BorshSerialize};
use himsha_runtime::{
    account::{AccountInfo, AccountMeta},
    error::ProgramError,
    instruction::Instruction,
    pubkey::Pubkey,
    utxo::UtxoMeta,
};
use std::collections::HashMap;

// ---- on-chain state ----

/// A lending market for one NFT collection.
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct CollectionAccount {
    pub name: String,
    /// Open bids (loan offers) keyed by lender's UTXO "txid:vout".
    pub bids: HashMap<String, Bid>,
    /// Active loans keyed by inscription ID.
    pub active_loans: HashMap<String, Loan>,
    /// Settlement directives produced by repay/default, awaiting the node to
    /// move the actual Bitcoin UTXOs. The node drains these after execution
    /// (see [`take_settlements`]).
    pub pending_settlements: Vec<Settlement>,
}

/// What the node must do with a UTXO to settle a loan outcome.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SettlementKind {
    /// Loan repaid in time → return the inscription to the borrower.
    ReturnInscription,
    /// Borrower defaulted → transfer the inscription to the lender.
    SeizeInscription,
    /// Pay the repayment sats to the lender.
    Repayment,
}

/// A single Bitcoin-layer action the node must perform to settle a loan.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Settlement {
    pub kind: SettlementKind,
    pub inscription_id: String,
    /// UTXO to spend — the inscription's UTXO, or the repayment UTXO.
    pub utxo: UtxoMeta,
    /// Destination Bitcoin address.
    pub recipient: String,
    /// Sats to send (0 for inscription moves — the inscription rides its own sat).
    pub amount: u64,
}

/// Remove and return all queued settlements. The node calls this after the
/// instruction executes, builds the Bitcoin transactions, and broadcasts them.
pub fn take_settlements(collection: &mut CollectionAccount) -> Vec<Settlement> {
    core::mem::take(&mut collection.pending_settlements)
}

/// Basis-points denominator for interest.
pub const BPS: u64 = 10_000;

/// A lender's offer to provide liquidity.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Bid {
    /// The UTXO the lender is committing as loan funds.
    pub utxo: UtxoMeta,
    /// Satoshis the lender offers.
    pub loan_value: u64,
    /// Loan duration in seconds.
    pub loan_period: u64,
    /// Flat interest charged over the loan term, in bps of `loan_value`.
    pub interest_rate_bps: u64,
    /// Where the inscription should go if borrower defaults.
    pub lender_ordinals_address: String,
    /// Where repayment should be sent.
    pub lender_payments_address: String,
}

/// An active loan after a borrower accepted a bid.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Loan {
    pub inscription_id: String,
    pub inscription_utxo: UtxoMeta,
    pub loan_value: u64,
    pub loan_period: u64,
    pub interest_rate_bps: u64,
    pub lender_ordinals_address: String,
    pub lender_payments_address: String,
    pub borrower_ordinals_address: String,
    pub borrower_payments_address: String,
    /// Unix timestamp when the loan started.
    pub started_at: u64,
    /// Sats repaid so far (supports partial repayment).
    pub repaid: u64,
}

impl Loan {
    /// Total sats the borrower must repay to redeem the inscription:
    /// principal plus flat term interest.
    pub fn repayment_due(&self) -> u64 {
        self.loan_value + self.loan_value.saturating_mul(self.interest_rate_bps) / BPS
    }
}

// ---- instructions ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum LendingInstruction {
    /// Create a new empty collection market.
    /// accounts[0] = collection account (writable), accounts[1] = payer (signer).
    InitCollection { name: String },

    /// Place a lending bid into a collection.
    /// accounts[0] = collection account (writable), accounts[1] = lender (signer).
    PlaceBid {
        bid_utxo: UtxoMeta,
        loan_value: u64,
        loan_period: u64,
        interest_rate_bps: u64,
        lender_ordinals_address: String,
        lender_payments_address: String,
    },

    /// Lender withdraws an open (unaccepted) bid.
    /// accounts[0] = collection account (writable), accounts[1] = lender (signer).
    CancelBid { bid_utxo: UtxoMeta },

    /// Borrower accepts the highest bid and receives funds.
    /// accounts[0] = collection account (writable), accounts[1] = borrower (signer).
    AcceptBid {
        inscription_id: String,
        inscription_utxo: UtxoMeta,
        borrower_ordinals_address: String,
        borrower_payments_address: String,
    },

    /// Borrower repays (possibly partially) within the agreed period. The
    /// inscription is returned only once cumulative repayment covers the amount
    /// due (principal + interest). `amount` is the sats this UTXO repays; the
    /// node verifies it against the actual UTXO value.
    /// accounts[0] = collection account (writable), accounts[1] = borrower (signer).
    RepayLoan {
        inscription_id: String,
        repayment_utxo: UtxoMeta,
        amount: u64,
    },

    /// Lender claims inscription after borrower defaults (deadline passed).
    /// accounts[0] = collection account (writable), accounts[1] = lender (signer).
    ClaimDefault { inscription_id: String },
}

// ---- instruction builders ----

pub fn init_collection(collection: Pubkey, payer: Pubkey, name: String) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::lending_program(),
        vec![
            AccountMeta::writable(collection, false),
            AccountMeta::writable(payer, true),
        ],
        &LendingInstruction::InitCollection { name },
    )
}

#[allow(clippy::too_many_arguments)]
pub fn place_bid(
    collection: Pubkey,
    lender: Pubkey,
    bid_utxo: UtxoMeta,
    loan_value: u64,
    loan_period: u64,
    interest_rate_bps: u64,
    lender_ordinals_address: String,
    lender_payments_address: String,
) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::lending_program(),
        vec![
            AccountMeta::writable(collection, false),
            AccountMeta::readonly(lender, true),
        ],
        &LendingInstruction::PlaceBid {
            bid_utxo,
            loan_value,
            loan_period,
            interest_rate_bps,
            lender_ordinals_address,
            lender_payments_address,
        },
    )
}

pub fn cancel_bid(collection: Pubkey, lender: Pubkey, bid_utxo: UtxoMeta) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::lending_program(),
        vec![
            AccountMeta::writable(collection, false),
            AccountMeta::readonly(lender, true),
        ],
        &LendingInstruction::CancelBid { bid_utxo },
    )
}

pub fn accept_bid(
    collection: Pubkey,
    borrower: Pubkey,
    inscription_id: String,
    inscription_utxo: UtxoMeta,
    borrower_ordinals_address: String,
    borrower_payments_address: String,
) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::lending_program(),
        vec![
            AccountMeta::writable(collection, false),
            AccountMeta::readonly(borrower, true),
        ],
        &LendingInstruction::AcceptBid {
            inscription_id,
            inscription_utxo,
            borrower_ordinals_address,
            borrower_payments_address,
        },
    )
}

pub fn repay_loan(
    collection: Pubkey,
    borrower: Pubkey,
    inscription_id: String,
    repayment_utxo: UtxoMeta,
    amount: u64,
) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::lending_program(),
        vec![
            AccountMeta::writable(collection, false),
            AccountMeta::readonly(borrower, true),
        ],
        &LendingInstruction::RepayLoan {
            inscription_id,
            repayment_utxo,
            amount,
        },
    )
}

pub fn claim_default(collection: Pubkey, lender: Pubkey, inscription_id: String) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::lending_program(),
        vec![
            AccountMeta::writable(collection, false),
            AccountMeta::readonly(lender, true),
        ],
        &LendingInstruction::ClaimDefault { inscription_id },
    )
}

// ---- processing (runs inside zkVM) ----

pub fn process(
    accounts: &mut [AccountInfo],
    data: &[u8],
    timestamp: u64,
) -> Result<(), ProgramError> {
    let ix =
        LendingInstruction::try_from_slice(data).map_err(|_| ProgramError::InvalidInstruction)?;

    // Every lending instruction is authorized by accounts[1] (the lender/borrower
    // for that action). The node marks it as a signer after verifying signatures.
    if accounts.len() < 2 || !accounts[1].is_signer {
        return Err(ProgramError::MissingSigner);
    }

    match ix {
        LendingInstruction::InitCollection { name } => {
            let acc = &mut accounts[0];
            let existing: Option<CollectionAccount> = acc.read_data().ok();
            if existing.map(|c| !c.name.is_empty()).unwrap_or(false) {
                return Err(ProgramError::AlreadyInitialized);
            }
            let collection = CollectionAccount {
                name,
                bids: HashMap::new(),
                active_loans: HashMap::new(),
                pending_settlements: Vec::new(),
            };
            acc.write_data(&collection)?;
        }

        LendingInstruction::PlaceBid {
            bid_utxo,
            loan_value,
            loan_period,
            interest_rate_bps,
            lender_ordinals_address,
            lender_payments_address,
        } => {
            let acc = &mut accounts[0];
            let mut collection: CollectionAccount = acc.read_data()?;

            let key = format!("{}:{}", hex::encode(bid_utxo.txid), bid_utxo.vout);
            if collection.bids.contains_key(&key) {
                return Err(ProgramError::AlreadyInitialized);
            }
            collection.bids.insert(
                key,
                Bid {
                    utxo: bid_utxo,
                    loan_value,
                    loan_period,
                    interest_rate_bps,
                    lender_ordinals_address,
                    lender_payments_address,
                },
            );
            acc.write_data(&collection)?;
        }

        LendingInstruction::CancelBid { bid_utxo } => {
            let acc = &mut accounts[0];
            let mut collection: CollectionAccount = acc.read_data()?;

            let key = format!("{}:{}", hex::encode(bid_utxo.txid), bid_utxo.vout);
            // Removing releases the lender's committed funds (never spent on-chain).
            collection
                .bids
                .remove(&key)
                .ok_or(ProgramError::NotInitialized)?;
            acc.write_data(&collection)?;
        }

        LendingInstruction::AcceptBid {
            inscription_id,
            inscription_utxo,
            borrower_ordinals_address,
            borrower_payments_address,
        } => {
            let acc = &mut accounts[0];
            let mut collection: CollectionAccount = acc.read_data()?;

            if collection.bids.is_empty() {
                return Err(ProgramError::NotInitialized);
            }
            // An inscription already pledged to an active loan can't be re-collateralized.
            if collection.active_loans.contains_key(&inscription_id) {
                return Err(ProgramError::AlreadyInitialized);
            }

            // Pick the best bid (highest loan value)
            let best_key = collection
                .bids
                .iter()
                .max_by_key(|(_, b)| b.loan_value)
                .map(|(k, _)| k.clone())
                .ok_or(ProgramError::NotInitialized)?;

            let bid = collection.bids.remove(&best_key).unwrap();

            collection.active_loans.insert(
                inscription_id.clone(),
                Loan {
                    inscription_id: inscription_id.clone(),
                    inscription_utxo,
                    loan_value: bid.loan_value,
                    loan_period: bid.loan_period,
                    interest_rate_bps: bid.interest_rate_bps,
                    lender_ordinals_address: bid.lender_ordinals_address,
                    lender_payments_address: bid.lender_payments_address,
                    borrower_ordinals_address,
                    borrower_payments_address,
                    started_at: timestamp,
                    repaid: 0,
                },
            );
            acc.write_data(&collection)?;
        }

        LendingInstruction::RepayLoan {
            inscription_id,
            repayment_utxo,
            amount,
        } => {
            let acc = &mut accounts[0];
            let mut collection: CollectionAccount = acc.read_data()?;

            let mut loan = collection
                .active_loans
                .remove(&inscription_id)
                .ok_or(ProgramError::NotInitialized)?;

            // Must repay within the agreed period.
            let elapsed = timestamp.saturating_sub(loan.started_at);
            if elapsed > loan.loan_period {
                // Put the loan back before erroring so it isn't lost.
                collection.active_loans.insert(inscription_id.clone(), loan);
                acc.write_data(&collection)?;
                return Err(ProgramError::LoanExpired);
            }
            if amount == 0 {
                return Err(ProgramError::InvalidInstruction);
            }

            loan.repaid = loan.repaid.saturating_add(amount);

            // Forward this payment to the lender regardless of partial/full.
            collection.pending_settlements.push(Settlement {
                kind: SettlementKind::Repayment,
                inscription_id: inscription_id.clone(),
                utxo: repayment_utxo,
                recipient: loan.lender_payments_address.clone(),
                amount,
            });

            if loan.repaid >= loan.repayment_due() {
                // Fully repaid (principal + interest) → return the inscription.
                collection.pending_settlements.push(Settlement {
                    kind: SettlementKind::ReturnInscription,
                    inscription_id: inscription_id.clone(),
                    utxo: loan.inscription_utxo,
                    recipient: loan.borrower_ordinals_address.clone(),
                    amount: 0,
                });
                // Loan closed (removed above; do not reinsert).
            } else {
                // Partial repayment — keep the loan open with updated progress.
                collection.active_loans.insert(inscription_id.clone(), loan);
            }

            acc.write_data(&collection)?;
        }

        LendingInstruction::ClaimDefault { inscription_id } => {
            let acc = &mut accounts[0];
            let mut collection: CollectionAccount = acc.read_data()?;

            let loan = collection
                .active_loans
                .remove(&inscription_id)
                .ok_or(ProgramError::NotInitialized)?;

            let elapsed = timestamp.saturating_sub(loan.started_at);
            if elapsed <= loan.loan_period {
                return Err(ProgramError::LoanNotExpired);
            }

            // Queue settlement: the node transfers the inscription to the lender.
            collection.pending_settlements.push(Settlement {
                kind: SettlementKind::SeizeInscription,
                inscription_id: inscription_id.clone(),
                utxo: loan.inscription_utxo,
                recipient: loan.lender_ordinals_address.clone(),
                amount: 0,
            });

            acc.write_data(&collection)?;
        }
    }

    Ok(())
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use himsha_runtime::pubkey::Pubkey;

    fn prog() -> Pubkey {
        himsha_runtime::program_ids::lending_program()
    }

    fn accounts() -> Vec<AccountInfo> {
        vec![
            AccountInfo::new(Pubkey::from_seed(b"coll"), prog(), 0, 4096),
            AccountInfo::new(Pubkey::from_seed(b"signer"), prog(), 0, 0).as_signer(),
        ]
    }

    fn utxo(tag: u8, vout: u32) -> UtxoMeta {
        UtxoMeta {
            txid: [tag; 32],
            vout,
        }
    }

    fn run(
        accounts: &mut [AccountInfo],
        ix: &LendingInstruction,
        ts: u64,
    ) -> Result<(), ProgramError> {
        process(accounts, &borsh::to_vec(ix).unwrap(), ts)
    }

    /// Init a collection, place a bid, and accept it (loan starts at `start`).
    fn open_loan(accounts: &mut [AccountInfo], start: u64) {
        open_loan_at(accounts, start, 0);
    }

    /// Like `open_loan`, but with an explicit flat interest rate (bps).
    fn open_loan_at(accounts: &mut [AccountInfo], start: u64, interest_rate_bps: u64) {
        run(
            accounts,
            &LendingInstruction::InitCollection {
                name: "punks".into(),
            },
            start,
        )
        .unwrap();
        run(
            accounts,
            &LendingInstruction::PlaceBid {
                bid_utxo: utxo(1, 0),
                loan_value: 100_000,
                loan_period: 1_000,
                interest_rate_bps,
                lender_ordinals_address: "lender_ord".into(),
                lender_payments_address: "lender_pay".into(),
            },
            start,
        )
        .unwrap();
        run(
            accounts,
            &LendingInstruction::AcceptBid {
                inscription_id: "insc1".into(),
                inscription_utxo: utxo(2, 0),
                borrower_ordinals_address: "borrower_ord".into(),
                borrower_payments_address: "borrower_pay".into(),
            },
            start,
        )
        .unwrap();
    }

    fn collection(accounts: &[AccountInfo]) -> CollectionAccount {
        accounts[0].read_data().unwrap()
    }

    #[test]
    fn test_accept_bid_starts_loan() {
        let mut accounts = accounts();
        open_loan(&mut accounts, 0);
        let coll = collection(&accounts);
        assert!(coll.bids.is_empty()); // best bid consumed
        assert_eq!(coll.active_loans.len(), 1);
        assert_eq!(coll.active_loans["insc1"].loan_value, 100_000);
    }

    #[test]
    fn test_repay_in_time_queues_payment_and_return() {
        let mut accounts = accounts();
        open_loan(&mut accounts, 0); // 0% interest → due = 100_000
        run(
            &mut accounts,
            &LendingInstruction::RepayLoan {
                inscription_id: "insc1".into(),
                repayment_utxo: utxo(3, 0),
                amount: 100_000,
            },
            500,
        )
        .unwrap(); // within the 1000s period

        let coll = collection(&accounts);
        assert!(coll.active_loans.is_empty()); // fully repaid → loan closed
        assert_eq!(coll.pending_settlements.len(), 2);

        let pay = &coll.pending_settlements[0];
        assert_eq!(pay.kind, SettlementKind::Repayment);
        assert_eq!(pay.recipient, "lender_pay");
        assert_eq!(pay.amount, 100_000);
        assert_eq!(pay.utxo, utxo(3, 0)); // the repayment UTXO

        let ret = &coll.pending_settlements[1];
        assert_eq!(ret.kind, SettlementKind::ReturnInscription);
        assert_eq!(ret.recipient, "borrower_ord");
        assert_eq!(ret.utxo, utxo(2, 0)); // the inscription UTXO
    }

    #[test]
    fn test_full_repay_requires_interest() {
        let mut accounts = accounts();
        open_loan_at(&mut accounts, 0, 1000); // 10% interest → due = 110_000
                                              // Paying only the principal is a partial repayment: loan stays open.
        run(
            &mut accounts,
            &LendingInstruction::RepayLoan {
                inscription_id: "insc1".into(),
                repayment_utxo: utxo(3, 0),
                amount: 100_000,
            },
            100,
        )
        .unwrap();
        assert_eq!(collection(&accounts).active_loans.len(), 1);

        // Paying the remaining 10_000 interest closes it and returns the inscription.
        run(
            &mut accounts,
            &LendingInstruction::RepayLoan {
                inscription_id: "insc1".into(),
                repayment_utxo: utxo(4, 0),
                amount: 10_000,
            },
            200,
        )
        .unwrap();
        let coll = collection(&accounts);
        assert!(coll.active_loans.is_empty());
        assert!(coll
            .pending_settlements
            .iter()
            .any(|s| s.kind == SettlementKind::ReturnInscription));
        // Two repayment settlements (100k + 10k) plus one return.
        assert_eq!(
            coll.pending_settlements
                .iter()
                .filter(|s| s.kind == SettlementKind::Repayment)
                .count(),
            2
        );
    }

    #[test]
    fn test_partial_repay_keeps_loan_open() {
        let mut accounts = accounts();
        open_loan(&mut accounts, 0); // due 100_000
        run(
            &mut accounts,
            &LendingInstruction::RepayLoan {
                inscription_id: "insc1".into(),
                repayment_utxo: utxo(3, 0),
                amount: 40_000,
            },
            100,
        )
        .unwrap();
        let coll = collection(&accounts);
        assert_eq!(coll.active_loans.len(), 1);
        assert_eq!(coll.active_loans["insc1"].repaid, 40_000);
        // Only the forwarded payment is queued; no inscription return yet.
        assert_eq!(coll.pending_settlements.len(), 1);
        assert_eq!(coll.pending_settlements[0].kind, SettlementKind::Repayment);
    }

    #[test]
    fn test_repay_after_expiry_fails() {
        let mut accounts = accounts();
        open_loan(&mut accounts, 0);
        assert_eq!(
            run(
                &mut accounts,
                &LendingInstruction::RepayLoan {
                    inscription_id: "insc1".into(),
                    repayment_utxo: utxo(3, 0),
                    amount: 100_000,
                },
                2_000
            ), // past the deadline
            Err(ProgramError::LoanExpired),
        );
        // Loan preserved (not lost) after the failed repay.
        assert_eq!(collection(&accounts).active_loans.len(), 1);
    }

    #[test]
    fn test_cancel_bid_removes_offer() {
        let mut accounts = accounts();
        run(
            &mut accounts,
            &LendingInstruction::InitCollection {
                name: "punks".into(),
            },
            0,
        )
        .unwrap();
        run(
            &mut accounts,
            &LendingInstruction::PlaceBid {
                bid_utxo: utxo(1, 0),
                loan_value: 100_000,
                loan_period: 1_000,
                interest_rate_bps: 0,
                lender_ordinals_address: "lo".into(),
                lender_payments_address: "lp".into(),
            },
            0,
        )
        .unwrap();
        assert_eq!(collection(&accounts).bids.len(), 1);

        run(
            &mut accounts,
            &LendingInstruction::CancelBid {
                bid_utxo: utxo(1, 0),
            },
            0,
        )
        .unwrap();
        assert!(collection(&accounts).bids.is_empty());

        // Cancelling a non-existent bid fails.
        assert_eq!(
            run(
                &mut accounts,
                &LendingInstruction::CancelBid {
                    bid_utxo: utxo(9, 0)
                },
                0
            ),
            Err(ProgramError::NotInitialized),
        );
    }

    #[test]
    fn test_accept_bid_double_pledge_fails() {
        let mut accounts = accounts();
        open_loan(&mut accounts, 0); // insc1 now has an active loan
                                     // Place another bid, then try to pledge insc1 again.
        run(
            &mut accounts,
            &LendingInstruction::PlaceBid {
                bid_utxo: utxo(5, 0),
                loan_value: 50_000,
                loan_period: 1_000,
                interest_rate_bps: 0,
                lender_ordinals_address: "lo".into(),
                lender_payments_address: "lp".into(),
            },
            0,
        )
        .unwrap();
        assert_eq!(
            run(
                &mut accounts,
                &LendingInstruction::AcceptBid {
                    inscription_id: "insc1".into(),
                    inscription_utxo: utxo(2, 0),
                    borrower_ordinals_address: "bo".into(),
                    borrower_payments_address: "bp".into(),
                },
                0
            ),
            Err(ProgramError::AlreadyInitialized),
        );
    }

    #[test]
    fn test_claim_default_after_expiry_seizes_inscription() {
        let mut accounts = accounts();
        open_loan(&mut accounts, 0);
        run(
            &mut accounts,
            &LendingInstruction::ClaimDefault {
                inscription_id: "insc1".into(),
            },
            2_000,
        )
        .unwrap();

        let coll = collection(&accounts);
        assert!(coll.active_loans.is_empty());
        assert_eq!(coll.pending_settlements.len(), 1);
        let s = &coll.pending_settlements[0];
        assert_eq!(s.kind, SettlementKind::SeizeInscription);
        assert_eq!(s.recipient, "lender_ord");
        assert_eq!(s.utxo, utxo(2, 0));
    }

    #[test]
    fn test_claim_default_before_expiry_fails() {
        let mut accounts = accounts();
        open_loan(&mut accounts, 0);
        assert_eq!(
            run(
                &mut accounts,
                &LendingInstruction::ClaimDefault {
                    inscription_id: "insc1".into()
                },
                500
            ),
            Err(ProgramError::LoanNotExpired),
        );
    }

    #[test]
    fn test_take_settlements_drains_queue() {
        let mut accounts = accounts();
        open_loan(&mut accounts, 0);
        run(
            &mut accounts,
            &LendingInstruction::ClaimDefault {
                inscription_id: "insc1".into(),
            },
            2_000,
        )
        .unwrap();

        let mut coll = collection(&accounts);
        let drained = take_settlements(&mut coll);
        assert_eq!(drained.len(), 1);
        assert!(coll.pending_settlements.is_empty()); // queue cleared for the node
    }
}
