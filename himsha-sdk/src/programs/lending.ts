import { HimshaInstruction } from '../transaction';
import { HimshaPublicKey, PROGRAM_IDS } from '../pubkey';

// Borsh enum variant indices — order must match LendingInstruction in Rust.
const IX = {
  InitCollection: new Uint8Array([0]),
  PlaceBid:       new Uint8Array([1]),
  CancelBid:      new Uint8Array([2]),
  AcceptBid:      new Uint8Array([3]),
  RepayLoan:      new Uint8Array([4]),
  ClaimDefault:   new Uint8Array([5]),
} as const;

function u64Le(n: bigint): Uint8Array {
  const buf = new Uint8Array(8);
  new DataView(buf.buffer).setBigUint64(0, n, true);
  return buf;
}

function u32Le(n: number): Uint8Array {
  const buf = new Uint8Array(4);
  new DataView(buf.buffer).setUint32(0, n, true);
  return buf;
}

function encodeString(s: string): Uint8Array {
  const bytes = new TextEncoder().encode(s);
  const len = new Uint8Array(4);
  new DataView(len.buffer).setUint32(0, bytes.length, true);
  return concat(len, bytes);
}

function concat(...arrays: Uint8Array[]): Uint8Array {
  const len = arrays.reduce((s, a) => s + a.length, 0);
  const out = new Uint8Array(len);
  let offset = 0;
  for (const a of arrays) { out.set(a, offset); offset += a.length; }
  return out;
}

/** Create a new lending market for a named NFT collection. */
export function initCollection(
  collectionAccount: HimshaPublicKey,
  payer:             HimshaPublicKey,
  name:              string,
): HimshaInstruction {
  const data = concat(IX.InitCollection, encodeString(name));
  return new HimshaInstruction(
    PROGRAM_IDS.lending,
    [
      HimshaInstruction.writable(collectionAccount, false),
      HimshaInstruction.writable(payer, true),
    ],
    data,
  );
}

/**
 * Place a lending bid (loan offer) in a collection market.
 * `bidUtxo` is the UTXO containing the funds the lender is offering.
 */
export function placeBid(
  collectionAccount:      HimshaPublicKey,
  lender:                 HimshaPublicKey,
  bidTxid:                Uint8Array,  // 32 bytes
  bidVout:                number,
  loanValueSats:          bigint,
  loanPeriodSecs:         bigint,
  interestRateBps:        bigint,      // flat term interest, e.g. 1000n = 10%
  lenderOrdinalsAddress:  string,
  lenderPaymentsAddress:  string,
): HimshaInstruction {
  const data = concat(
    IX.PlaceBid,
    bidTxid,
    u32Le(bidVout),
    u64Le(loanValueSats),
    u64Le(loanPeriodSecs),
    u64Le(interestRateBps),
    encodeString(lenderOrdinalsAddress),
    encodeString(lenderPaymentsAddress),
  );
  return new HimshaInstruction(
    PROGRAM_IDS.lending,
    [
      HimshaInstruction.writable(collectionAccount, false),
      HimshaInstruction.readonly(lender, true),
    ],
    data,
  );
}

/** Lender withdraws an open (unaccepted) bid. */
export function cancelBid(
  collectionAccount: HimshaPublicKey,
  lender:            HimshaPublicKey,
  bidTxid:           Uint8Array,  // 32 bytes
  bidVout:           number,
): HimshaInstruction {
  const data = concat(IX.CancelBid, bidTxid, u32Le(bidVout));
  return new HimshaInstruction(
    PROGRAM_IDS.lending,
    [
      HimshaInstruction.writable(collectionAccount, false),
      HimshaInstruction.readonly(lender, true),
    ],
    data,
  );
}

/**
 * Borrower accepts the highest bid and pledges an inscription as collateral.
 * The borrower receives the loan amount from the lender's UTXO.
 */
export function acceptBid(
  collectionAccount:       HimshaPublicKey,
  borrower:                HimshaPublicKey,
  inscriptionId:           string,
  inscriptionTxid:         Uint8Array,  // 32 bytes
  inscriptionVout:         number,
  borrowerOrdinalsAddress: string,
  borrowerPaymentsAddress: string,
): HimshaInstruction {
  const data = concat(
    IX.AcceptBid,
    encodeString(inscriptionId),
    inscriptionTxid,
    u32Le(inscriptionVout),
    encodeString(borrowerOrdinalsAddress),
    encodeString(borrowerPaymentsAddress),
  );
  return new HimshaInstruction(
    PROGRAM_IDS.lending,
    [
      HimshaInstruction.writable(collectionAccount, false),
      HimshaInstruction.readonly(borrower, true),
    ],
    data,
  );
}

/**
 * Repay a loan within the agreed period.
 * The inscription returns to the borrower; the loan amount goes to the lender.
 */
export function repayLoan(
  collectionAccount: HimshaPublicKey,
  borrower:          HimshaPublicKey,
  inscriptionId:     string,
  repaymentTxid:     Uint8Array,  // 32 bytes
  repaymentVout:     number,
  amountSats:        bigint,      // sats this UTXO repays (supports partial repay)
): HimshaInstruction {
  const data = concat(
    IX.RepayLoan,
    encodeString(inscriptionId),
    repaymentTxid,
    u32Le(repaymentVout),
    u64Le(amountSats),
  );
  return new HimshaInstruction(
    PROGRAM_IDS.lending,
    [
      HimshaInstruction.writable(collectionAccount, false),
      HimshaInstruction.readonly(borrower, true),
    ],
    data,
  );
}

/**
 * Lender claims the inscription after the borrower defaults (deadline passed).
 */
export function claimDefault(
  collectionAccount: HimshaPublicKey,
  lender:            HimshaPublicKey,
  inscriptionId:     string,
): HimshaInstruction {
  const data = concat(IX.ClaimDefault, encodeString(inscriptionId));
  return new HimshaInstruction(
    PROGRAM_IDS.lending,
    [
      HimshaInstruction.writable(collectionAccount, false),
      HimshaInstruction.readonly(lender, true),
    ],
    data,
  );
}

export const LendingProgram = {
  initCollection, placeBid, cancelBid, acceptBid, repayLoan, claimDefault,
};
