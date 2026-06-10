//! HIMSHA Oracle — signed, aggregated price feeds.
//!
//! A `PriceFeed` is fed by one or more authorized **publishers**. Each publisher
//! posts its own price; the feed's reported `price` is the **median of fresh
//! submissions**, so one compromised or glitching publisher cannot move the
//! aggregate on its own once ≥3 publishers exist. Consumers (e.g. the money
//! market) read the aggregate and reject prices older than their staleness
//! window. ZK proves "we used exactly the price the oracle published" — it
//! can't prove the price is *true*, so the oracle remains a separate trust
//! component (see docs/use-cases and the oracle/consensus discussion).
//!
//! Robustness controls (both opt-in at `InitFeed`, 0 = disabled):
//!   - **`max_deviation_bps`** — an update may not move the aggregate by more
//!     than this fraction in one step. A real crash still gets through, one
//!     bounded step per update (a "speed bump"), but a single fat-finger or
//!     manipulated print cannot instantly re-price the feed.
//!   - **`max_submission_age`** — submissions older than this many seconds are
//!     excluded from the median, so a publisher that went silent stops
//!     influencing the price.
//!
//! `price` is a fixed-point value; the consumer defines the scale (the money
//! market uses `PRICE_SCALE = 1e6`, i.e. price of 1 collateral unit in
//! borrow-asset units).

use borsh::{BorshDeserialize, BorshSerialize};
use himsha_runtime::{
    account::{AccountInfo, AccountMeta},
    error::ProgramError,
    instruction::Instruction,
    pubkey::Pubkey,
};

// ---- state ----

/// Upper bound on registered publishers — bounds feed account size.
pub const MAX_PUBLISHERS: usize = 16;
/// Basis-points denominator.
pub const BPS: u128 = 10_000;

/// One publisher's latest submission.
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct Submission {
    pub publisher: Pubkey,
    pub price: u64,
    pub publish_ts: u64,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct PriceFeed {
    /// Feed admin: manages the publisher set (and is itself a publisher).
    pub authority: Pubkey,
    /// Latest aggregate (median of fresh submissions).
    pub price: u64,
    /// Unix timestamp the aggregate was last recomputed.
    pub publish_ts: u64,
    pub is_initialized: bool,
    /// Max one-step move of the aggregate, in bps (0 = unbounded).
    pub max_deviation_bps: u64,
    /// Submissions older than this many seconds drop out of the median
    /// (0 = never expire).
    pub max_submission_age: u64,
    /// Per-publisher submissions; `authority` occupies the first slot.
    pub submissions: Vec<Submission>,
}

impl PriceFeed {
    /// True if the price was published within `max_staleness` seconds of `now`
    /// and is non-zero.
    pub fn is_fresh(&self, now: u64, max_staleness: u64) -> bool {
        self.price != 0 && now.saturating_sub(self.publish_ts) <= max_staleness
    }

    /// Median of the submissions still fresh at `now` (`None` if there are none).
    /// Even counts take the lower-middle element — a conservative, deterministic
    /// choice that never fabricates a price no publisher posted.
    pub fn fresh_median(&self, now: u64) -> Option<u64> {
        let mut prices: Vec<u64> = self
            .submissions
            .iter()
            .filter(|s| {
                s.price != 0
                    && (self.max_submission_age == 0
                        || now.saturating_sub(s.publish_ts) <= self.max_submission_age)
            })
            .map(|s| s.price)
            .collect();
        if prices.is_empty() {
            return None;
        }
        prices.sort_unstable();
        Some(prices[(prices.len() - 1) / 2])
    }
}

// ---- instructions ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum OracleInstruction {
    /// Create a feed administered (and initially published) by `authority`.
    /// [0] = feed (writable), [1] = authority (signer).
    InitFeed {
        max_deviation_bps: u64,
        max_submission_age: u64,
    },

    /// Publish a new price (registered publishers only).
    /// [0] = feed (writable), [1] = publisher (signer).
    UpdatePrice { price: u64 },

    /// Register an additional publisher (authority only).
    /// [0] = feed (writable), [1] = authority (signer).
    AddPublisher { publisher: Pubkey },

    /// Remove a publisher and its submission (authority only; the authority
    /// itself cannot be removed).
    /// [0] = feed (writable), [1] = authority (signer).
    RemovePublisher { publisher: Pubkey },
}

// ---- builders ----

fn program() -> Pubkey {
    himsha_runtime::program_ids::oracle_program()
}

fn admin_ix(feed: Pubkey, authority: Pubkey, ix: &OracleInstruction) -> Instruction {
    Instruction::with_args(
        program(),
        vec![
            AccountMeta::writable(feed, false),
            AccountMeta::readonly(authority, true),
        ],
        ix,
    )
}

pub fn init_feed(
    feed: Pubkey,
    authority: Pubkey,
    max_deviation_bps: u64,
    max_submission_age: u64,
) -> Instruction {
    admin_ix(
        feed,
        authority,
        &OracleInstruction::InitFeed {
            max_deviation_bps,
            max_submission_age,
        },
    )
}

pub fn update_price(feed: Pubkey, publisher: Pubkey, price: u64) -> Instruction {
    admin_ix(feed, publisher, &OracleInstruction::UpdatePrice { price })
}

pub fn add_publisher(feed: Pubkey, authority: Pubkey, publisher: Pubkey) -> Instruction {
    admin_ix(
        feed,
        authority,
        &OracleInstruction::AddPublisher { publisher },
    )
}

pub fn remove_publisher(feed: Pubkey, authority: Pubkey, publisher: Pubkey) -> Instruction {
    admin_ix(
        feed,
        authority,
        &OracleInstruction::RemovePublisher { publisher },
    )
}

// ---- processing ----

/// `|new - old| / old`, in bps.
fn deviation_bps(old: u64, new: u64) -> u128 {
    let old = old as u128;
    let new = new as u128;
    let diff = old.abs_diff(new);
    diff * BPS / old
}

fn require_authority(feed: &PriceFeed, accounts: &[AccountInfo]) -> Result<(), ProgramError> {
    accounts[1].require_signer()?;
    if accounts[1].key != feed.authority {
        return Err(ProgramError::Unauthorized);
    }
    Ok(())
}

pub fn process(
    accounts: &mut [AccountInfo],
    data: &[u8],
    timestamp: u64,
) -> Result<(), ProgramError> {
    let ix =
        OracleInstruction::try_from_slice(data).map_err(|_| ProgramError::InvalidInstruction)?;

    if accounts.len() < 2 {
        return Err(ProgramError::NotEnoughAccounts);
    }

    match ix {
        OracleInstruction::InitFeed {
            max_deviation_bps,
            max_submission_age,
        } => {
            accounts[1].require_signer()?; // authority
            let authority = accounts[1].key;
            let mut feed: PriceFeed = accounts[0].read_data().unwrap_or_default();
            if feed.is_initialized {
                return Err(ProgramError::AlreadyInitialized);
            }
            feed.authority = authority;
            feed.max_deviation_bps = max_deviation_bps;
            feed.max_submission_age = max_submission_age;
            feed.submissions = vec![Submission {
                publisher: authority,
                price: 0,
                publish_ts: 0,
            }];
            feed.is_initialized = true;
            accounts[0].write_data(&feed)?;
        }

        OracleInstruction::UpdatePrice { price } => {
            accounts[1].require_signer()?; // publisher
            let mut feed: PriceFeed = accounts[0].read_data()?;
            if !feed.is_initialized {
                return Err(ProgramError::NotInitialized);
            }
            if price == 0 {
                return Err(ProgramError::InvalidInstruction);
            }
            let publisher = accounts[1].key;
            let slot = feed
                .submissions
                .iter_mut()
                .find(|s| s.publisher == publisher)
                .ok_or(ProgramError::Unauthorized)?;
            slot.price = price;
            slot.publish_ts = timestamp;

            // Re-aggregate: median of fresh submissions, bounded per step.
            let median = feed
                .fresh_median(timestamp)
                .ok_or(ProgramError::StalePrice)?;
            if feed.price != 0
                && feed.max_deviation_bps != 0
                && deviation_bps(feed.price, median) > feed.max_deviation_bps as u128
            {
                // A bounded feed moves at most max_deviation_bps per update; a
                // genuine re-pricing walks there over several updates.
                return Err(ProgramError::PriceOutOfBounds);
            }
            feed.price = median;
            feed.publish_ts = timestamp;
            accounts[0].write_data(&feed)?;
        }

        OracleInstruction::AddPublisher { publisher } => {
            let mut feed: PriceFeed = accounts[0].read_data()?;
            if !feed.is_initialized {
                return Err(ProgramError::NotInitialized);
            }
            require_authority(&feed, accounts)?;
            if feed.submissions.iter().any(|s| s.publisher == publisher) {
                return Err(ProgramError::AlreadyInitialized);
            }
            if feed.submissions.len() >= MAX_PUBLISHERS {
                return Err(ProgramError::InvalidInstruction);
            }
            feed.submissions.push(Submission {
                publisher,
                price: 0,
                publish_ts: 0,
            });
            accounts[0].write_data(&feed)?;
        }

        OracleInstruction::RemovePublisher { publisher } => {
            let mut feed: PriceFeed = accounts[0].read_data()?;
            if !feed.is_initialized {
                return Err(ProgramError::NotInitialized);
            }
            require_authority(&feed, accounts)?;
            if publisher == feed.authority {
                return Err(ProgramError::InvalidInstruction);
            }
            let before = feed.submissions.len();
            feed.submissions.retain(|s| s.publisher != publisher);
            if feed.submissions.len() == before {
                return Err(ProgramError::InvalidAccountData);
            }
            accounts[0].write_data(&feed)?;
        }
    }
    Ok(())
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn prog() -> Pubkey {
        himsha_runtime::program_ids::oracle_program()
    }
    fn feed_acct() -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(b"feed"), prog(), 0, 128)
    }
    fn signer(seed: &[u8]) -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(seed), prog(), 0, 0).as_signer()
    }
    fn authority() -> AccountInfo {
        signer(b"oracle-auth")
    }

    fn run(
        accounts: &mut [AccountInfo],
        ix: &OracleInstruction,
        ts: u64,
    ) -> Result<(), ProgramError> {
        process(accounts, &borsh::to_vec(ix).unwrap(), ts)
    }

    fn init(accounts: &mut [AccountInfo], max_deviation_bps: u64, max_submission_age: u64) {
        run(
            accounts,
            &OracleInstruction::InitFeed {
                max_deviation_bps,
                max_submission_age,
            },
            0,
        )
        .unwrap();
    }

    fn feed(accounts: &[AccountInfo]) -> PriceFeed {
        accounts[0].read_data().unwrap()
    }

    #[test]
    fn test_init_and_update() {
        let mut accounts = vec![feed_acct(), authority()];
        init(&mut accounts, 0, 0);
        run(
            &mut accounts,
            &OracleInstruction::UpdatePrice { price: 95_000_000 },
            1_000,
        )
        .unwrap();
        let f = feed(&accounts);
        assert_eq!(f.price, 95_000_000);
        assert_eq!(f.publish_ts, 1_000);
        assert_eq!(f.authority, Pubkey::from_seed(b"oracle-auth"));
    }

    #[test]
    fn test_update_requires_registered_publisher() {
        let mut accounts = vec![feed_acct(), authority()];
        init(&mut accounts, 0, 0);
        // wrong key signs
        accounts[1] = signer(b"intruder");
        assert_eq!(
            run(
                &mut accounts,
                &OracleInstruction::UpdatePrice { price: 1 },
                1
            ),
            Err(ProgramError::Unauthorized),
        );
    }

    #[test]
    fn test_update_without_signature_fails() {
        let mut accounts = vec![feed_acct(), authority()];
        init(&mut accounts, 0, 0);
        accounts[1].is_signer = false;
        assert_eq!(
            run(
                &mut accounts,
                &OracleInstruction::UpdatePrice { price: 1 },
                1
            ),
            Err(ProgramError::MissingSigner),
        );
    }

    #[test]
    fn test_zero_price_rejected() {
        let mut accounts = vec![feed_acct(), authority()];
        init(&mut accounts, 0, 0);
        assert_eq!(
            run(
                &mut accounts,
                &OracleInstruction::UpdatePrice { price: 0 },
                1
            ),
            Err(ProgramError::InvalidInstruction),
        );
    }

    #[test]
    fn test_freshness() {
        let f = PriceFeed {
            authority: Pubkey::default(),
            price: 100,
            publish_ts: 1_000,
            is_initialized: true,
            ..Default::default()
        };
        assert!(f.is_fresh(1_050, 60)); // 50s old, window 60
        assert!(!f.is_fresh(1_200, 60)); // 200s old, stale
        let zero = PriceFeed {
            price: 0,
            ..f.clone()
        };
        assert!(!zero.is_fresh(1_000, 60)); // zero price never fresh
    }

    // ---- deviation bound ----

    #[test]
    fn test_deviation_bound_rejects_single_jump() {
        let mut accounts = vec![feed_acct(), authority()];
        init(&mut accounts, 1_000, 0); // max 10% per update
        run(
            &mut accounts,
            &OracleInstruction::UpdatePrice { price: 100_000 },
            1,
        )
        .unwrap();
        // +50% in one print → rejected; the feed keeps its last good price.
        assert_eq!(
            run(
                &mut accounts,
                &OracleInstruction::UpdatePrice { price: 150_000 },
                2
            ),
            Err(ProgramError::PriceOutOfBounds),
        );
        assert_eq!(feed(&accounts).price, 100_000);
    }

    #[test]
    fn test_deviation_bound_allows_walking_to_new_level() {
        let mut accounts = vec![feed_acct(), authority()];
        init(&mut accounts, 1_000, 0); // max 10% per update
        run(
            &mut accounts,
            &OracleInstruction::UpdatePrice { price: 100_000 },
            1,
        )
        .unwrap();
        // A genuine 21% crash arrives as bounded steps: -10%, -10%, then the rest.
        for (ts, p) in [(2u64, 90_000u64), (3, 81_000), (4, 79_000)] {
            run(
                &mut accounts,
                &OracleInstruction::UpdatePrice { price: p },
                ts,
            )
            .unwrap();
        }
        assert_eq!(feed(&accounts).price, 79_000);
    }

    #[test]
    fn test_first_price_not_deviation_bound() {
        let mut accounts = vec![feed_acct(), authority()];
        init(&mut accounts, 100, 0); // tight 1% bound
                                     // Bootstrap print is exempt (there is nothing to deviate from).
        run(
            &mut accounts,
            &OracleInstruction::UpdatePrice { price: 42 },
            1,
        )
        .unwrap();
        assert_eq!(feed(&accounts).price, 42);
    }

    // ---- multi-publisher median ----

    fn three_publisher_feed() -> Vec<AccountInfo> {
        let mut accounts = vec![feed_acct(), authority()];
        init(&mut accounts, 0, 0);
        for p in [b"pub-b".as_ref(), b"pub-c".as_ref()] {
            run(
                &mut accounts,
                &OracleInstruction::AddPublisher {
                    publisher: Pubkey::from_seed(p),
                },
                0,
            )
            .unwrap();
        }
        accounts
    }

    fn publish(accounts: &mut [AccountInfo], who: &[u8], price: u64, ts: u64) {
        accounts[1] = signer(who);
        run(accounts, &OracleInstruction::UpdatePrice { price }, ts).unwrap();
    }

    #[test]
    fn test_median_of_three_publishers() {
        let mut accounts = three_publisher_feed();
        publish(&mut accounts, b"oracle-auth", 100, 10);
        publish(&mut accounts, b"pub-b", 110, 11);
        publish(&mut accounts, b"pub-c", 90, 12);
        // median(100, 110, 90) = 100
        assert_eq!(feed(&accounts).price, 100);
    }

    #[test]
    fn test_one_rogue_publisher_cannot_move_the_median() {
        let mut accounts = three_publisher_feed();
        publish(&mut accounts, b"oracle-auth", 100, 10);
        publish(&mut accounts, b"pub-b", 101, 11);
        // pub-c prints a manipulated 10x price; the median barely moves.
        publish(&mut accounts, b"pub-c", 1_000, 12);
        assert_eq!(feed(&accounts).price, 101);
    }

    #[test]
    fn test_stale_submission_drops_out_of_median() {
        let mut accounts = vec![feed_acct(), authority()];
        init(&mut accounts, 0, 60); // submissions expire after 60s
        run(
            &mut accounts,
            &OracleInstruction::AddPublisher {
                publisher: Pubkey::from_seed(b"pub-b"),
            },
            0,
        )
        .unwrap();
        publish(&mut accounts, b"oracle-auth", 100, 10);
        // 200s later only pub-b's print is fresh — the authority's 100 no
        // longer counts, so the aggregate is pub-b's price alone.
        publish(&mut accounts, b"pub-b", 130, 210);
        assert_eq!(feed(&accounts).price, 130);
    }

    // ---- publisher management ----

    #[test]
    fn test_add_publisher_requires_authority() {
        let mut accounts = vec![feed_acct(), authority()];
        init(&mut accounts, 0, 0);
        accounts[1] = signer(b"intruder");
        assert_eq!(
            run(
                &mut accounts,
                &OracleInstruction::AddPublisher {
                    publisher: Pubkey::from_seed(b"pub-x"),
                },
                0,
            ),
            Err(ProgramError::Unauthorized),
        );
    }

    #[test]
    fn test_removed_publisher_cannot_publish() {
        let mut accounts = three_publisher_feed();
        run(
            &mut accounts,
            &OracleInstruction::RemovePublisher {
                publisher: Pubkey::from_seed(b"pub-c"),
            },
            0,
        )
        .unwrap();
        accounts[1] = signer(b"pub-c");
        assert_eq!(
            run(
                &mut accounts,
                &OracleInstruction::UpdatePrice { price: 1 },
                1
            ),
            Err(ProgramError::Unauthorized),
        );
    }

    #[test]
    fn test_authority_cannot_be_removed() {
        let mut accounts = three_publisher_feed();
        assert_eq!(
            run(
                &mut accounts,
                &OracleInstruction::RemovePublisher {
                    publisher: Pubkey::from_seed(b"oracle-auth"),
                },
                0,
            ),
            Err(ProgramError::InvalidInstruction),
        );
    }

    #[test]
    fn test_publisher_cap() {
        let mut accounts = vec![feed_acct(), authority()];
        init(&mut accounts, 0, 0);
        for i in 1..MAX_PUBLISHERS {
            run(
                &mut accounts,
                &OracleInstruction::AddPublisher {
                    publisher: Pubkey::from_seed(format!("pub-{i}").as_bytes()),
                },
                0,
            )
            .unwrap();
        }
        assert_eq!(
            run(
                &mut accounts,
                &OracleInstruction::AddPublisher {
                    publisher: Pubkey::from_seed(b"one-too-many"),
                },
                0,
            ),
            Err(ProgramError::InvalidInstruction),
        );
    }
}
