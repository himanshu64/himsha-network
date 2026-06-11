//! Compute metering — bounds the total work one top-level transaction execution
//! may perform, via cooperative charges at cross-program-invocation boundaries.
//!
//! [`crate::cpi::MAX_CPI_DEPTH`] already bounds CPI *depth* (recursion), but not
//! *width*: four levels deep with a large fan-out at each level is still cheap to
//! request and expensive to run. This meter closes that gap by charging a fixed
//! cost per CPI plus a per-account cost for the window it copies, and aborting
//! with [`ProgramError::ComputeExhausted`] once a fixed budget is spent. The
//! budget is per top-level execution and reset by [`crate::owner::begin_execution`]
//! (the single reset point both native dispatch and the zkVM guest already call).
//!
//! Scope: this bounds the CPI call graph a transaction can build — the dominant
//! native-dispatch fan-out cost. It does **not** bound an unbounded loop *inside*
//! a single program (no per-instruction counting); that remains the job of the
//! RISC Zero cycle limit on the `--features zkvm` path. See docs/zkvm-proving.md.

use std::cell::Cell;

use crate::error::ProgramError;

/// Compute-unit budget for one top-level execution. Sized so a transaction can
/// fan out into a deep, realistic CPI graph (hundreds of invocations over modest
/// account windows) but not an abusive one. Pure metering — unrelated to fees.
pub const COMPUTE_BUDGET: u64 = 1_000_000;

/// Charged once per cross-program invocation.
pub const CPI_BASE_COST: u64 = 1_000;

/// Charged per account in an invocation's window (it is cloned in and written
/// back, so wide windows genuinely cost more).
pub const PER_ACCOUNT_COST: u64 = 100;

thread_local! {
    /// Compute units spent so far this top-level execution.
    static SPENT: Cell<u64> = const { Cell::new(0) };
}

/// Reset the meter for a new top-level execution. Called by
/// [`crate::owner::begin_execution`]; CPI re-entries must not call it.
pub fn reset() {
    SPENT.with(|s| s.set(0));
}

/// Charge `units`, returning [`ProgramError::ComputeExhausted`] if the budget is
/// exceeded. Saturating, so the counter can't wrap.
pub fn charge(units: u64) -> Result<(), ProgramError> {
    let next = SPENT.with(|s| s.get()).saturating_add(units);
    if next > COMPUTE_BUDGET {
        return Err(ProgramError::ComputeExhausted);
    }
    SPENT.with(|s| s.set(next));
    Ok(())
}

/// Compute units spent so far this execution (for inspection / tests).
pub fn spent() -> u64 {
    SPENT.with(|s| s.get())
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_accumulates_until_budget() {
        reset();
        assert_eq!(spent(), 0);
        charge(400_000).unwrap();
        charge(400_000).unwrap();
        assert_eq!(spent(), 800_000);
        // 800k + 300k = 1.1M > 1M budget → rejected, counter unchanged.
        assert_eq!(charge(300_000), Err(ProgramError::ComputeExhausted));
        assert_eq!(spent(), 800_000);
        // A smaller charge that fits still succeeds.
        charge(200_000).unwrap();
        assert_eq!(spent(), COMPUTE_BUDGET);
    }

    #[test]
    fn reset_clears_the_meter() {
        reset();
        charge(500_000).unwrap();
        reset();
        assert_eq!(spent(), 0);
    }

    #[test]
    fn exact_budget_is_allowed() {
        reset();
        assert!(charge(COMPUTE_BUDGET).is_ok());
        assert_eq!(charge(1), Err(ProgramError::ComputeExhausted));
    }
}
