import { HimshaPublicKey, PROGRAM_IDS } from '../pubkey';
import { SystemProgram }  from '../programs/system';
import { TokenProgram }   from '../programs/token';
import { SwapProgram }    from '../programs/swap';
import { LendingProgram } from '../programs/lending';

function key(seed: string): HimshaPublicKey { return HimshaPublicKey.fromSeed(seed); }

// ============================================================
// System Program
// ============================================================
describe('SystemProgram', () => {
  const payer   = key('payer');
  const newAcc  = key('new-account');
  const owner   = key('owner');

  it('createAccount targets system program', () => {
    const ix = SystemProgram.createAccount(payer, newAcc, 1_000_000n, 128n, owner);
    expect(ix.programId.equals(PROGRAM_IDS.system)).toBe(true);
  });

  it('createAccount has 2 accounts', () => {
    const ix = SystemProgram.createAccount(payer, newAcc, 1_000_000n, 128n, owner);
    expect(ix.accounts).toHaveLength(2);
    expect(ix.accounts[0].isSigner).toBe(true);     // payer signs
    expect(ix.accounts[0].isWritable).toBe(true);    // payer pays lamports
    expect(ix.accounts[1].isWritable).toBe(true);    // new account is writable
  });

  it('createAccount data starts with discriminant 0', () => {
    const ix = SystemProgram.createAccount(payer, newAcc, 5n, 64n, owner);
    expect(ix.data[0]).toBe(0);
  });

  it('transfer targets system program with correct accounts', () => {
    const from = key('from');
    const to   = key('to');
    const ix   = SystemProgram.transfer(from, to, 500n);
    expect(ix.programId.equals(PROGRAM_IDS.system)).toBe(true);
    expect(ix.accounts[0].pubkey.equals(from)).toBe(true);
    expect(ix.accounts[0].isSigner).toBe(true);
    expect(ix.accounts[1].pubkey.equals(to)).toBe(true);
    expect(ix.accounts[1].isSigner).toBe(false);
    // discriminant = 2 (Transfer)
    expect(ix.data[0]).toBe(2);
  });

  it('assign targets correct program', () => {
    const ix = SystemProgram.assign(newAcc, owner);
    expect(ix.data[0]).toBe(3);
    expect(ix.accounts[0].isWritable).toBe(true);
  });

  it('allocate has discriminant 4', () => {
    const ix = SystemProgram.allocate(newAcc, 256n);
    expect(ix.data[0]).toBe(4);
  });

  it('createAccountWithAnchor includes UTXO', () => {
    const txid = new Uint8Array(32).fill(0xaa);
    const ix = SystemProgram.createAccountWithAnchor(payer, newAcc, txid, 0, 64n, owner);
    expect(ix.data[0]).toBe(1);
    // txid starts at byte 1
    expect(ix.data.slice(1, 33)).toEqual(txid);
  });
});

// ============================================================
// Token Program
// ============================================================
describe('TokenProgram', () => {
  const mint      = key('mint');
  const authority = key('authority');
  const account   = key('token-account');
  const owner     = key('owner');
  const dest      = key('destination');

  it('initializeMint targets token program', () => {
    const ix = TokenProgram.initializeMint(mint, authority, 6);
    expect(ix.programId.equals(PROGRAM_IDS.token)).toBe(true);
    expect(ix.data[0]).toBe(0);
    // decimals stored at byte 1
    expect(ix.data[1]).toBe(6);
  });

  it('initializeMint with no freeze authority has 0 flag', () => {
    const ix = TokenProgram.initializeMint(mint, authority, 8);
    // After discriminant(1) + decimals(1) + authority(32) = byte 34
    expect(ix.data[34]).toBe(0); // no freeze authority
  });

  it('initializeMint with freeze authority has 1 flag', () => {
    const freeze = key('freeze');
    const ix = TokenProgram.initializeMint(mint, authority, 8, freeze);
    expect(ix.data[34]).toBe(1); // freeze authority present
  });

  it('initializeAccount has 3 accounts', () => {
    const ix = TokenProgram.initializeAccount(account, mint, owner);
    expect(ix.accounts).toHaveLength(3);
    expect(ix.data[0]).toBe(1);
  });

  it('mintTo targets token program', () => {
    const ix = TokenProgram.mintTo(mint, dest, authority, 1_000_000n);
    expect(ix.programId.equals(PROGRAM_IDS.token)).toBe(true);
    expect(ix.data[0]).toBe(2);
    expect(ix.accounts[2].isSigner).toBe(true); // authority signs
  });

  it('transfer has correct discriminant', () => {
    const ix = TokenProgram.transfer(account, dest, owner, 500n);
    expect(ix.data[0]).toBe(3);
  });

  it('burn reduces supply — discriminant 4', () => {
    const ix = TokenProgram.burn(account, mint, owner, 100n);
    expect(ix.data[0]).toBe(4);
  });

  it('approve sets discriminant 5', () => {
    const delegate = key('delegate');
    const ix = TokenProgram.approve(account, delegate, owner, 1000n);
    expect(ix.data[0]).toBe(5);
  });

  it('revoke is discriminant 6', () => {
    const ix = TokenProgram.revoke(account, owner);
    expect(ix.data[0]).toBe(6);
  });

  it('freezeAccount is discriminant 7', () => {
    const freeze = key('freeze');
    const ix = TokenProgram.freezeAccount(account, freeze);
    expect(ix.data[0]).toBe(7);
  });

  it('thawAccount is discriminant 8', () => {
    const freeze = key('freeze');
    const ix = TokenProgram.thawAccount(account, freeze);
    expect(ix.data[0]).toBe(8);
  });

  it('closeAccount is discriminant 9', () => {
    const ix = TokenProgram.closeAccount(account, dest, owner);
    expect(ix.data[0]).toBe(9);
  });
});

// ============================================================
// Swap Program
// ============================================================
describe('SwapProgram', () => {
  const pool     = key('pool');
  const mintA    = key('mintA');
  const mintB    = key('mintB');
  const resA     = key('reserveA');
  const resB     = key('reserveB');
  const lpMint   = key('lpMint');
  const payer    = key('payer');
  const source   = key('source');
  const dest     = key('dest');
  const user     = key('user');
  const userLp   = key('userLp');
  const userA    = key('userA');
  const userB    = key('userB');

  it('initialize targets swap program', () => {
    const ix = SwapProgram.initialize(pool, mintA, mintB, resA, resB, lpMint, payer, 3n, 1000n);
    expect(ix.programId.equals(PROGRAM_IDS.swap)).toBe(true);
    expect(ix.data[0]).toBe(0);
    expect(ix.accounts).toHaveLength(7);
  });

  it('swap has discriminant 1', () => {
    const ix = SwapProgram.swap(pool, source, dest, resA, resB, user, 100n, 90n);
    expect(ix.data[0]).toBe(1);
    expect(ix.accounts).toHaveLength(6);
  });

  it('deposit has discriminant 2', () => {
    const ix = SwapProgram.deposit(pool, userA, userB, resA, resB, userLp, user, key('lpMint'), 1000n, 1000n, 1n);
    expect(ix.data[0]).toBe(2);
    expect(ix.accounts).toHaveLength(8);
  });

  it('withdraw has discriminant 3', () => {
    const ix = SwapProgram.withdraw(pool, userA, userB, resA, resB, userLp, user, key('lpMint'), 500n, 450n, 450n);
    expect(ix.data[0]).toBe(3);
    expect(ix.accounts).toHaveLength(8);
  });
});

// ============================================================
// Lending Program
// ============================================================
describe('LendingProgram', () => {
  const collection = key('collection');
  const payer      = key('payer');
  const lender     = key('lender');
  const borrower   = key('borrower');
  const txid       = new Uint8Array(32).fill(0xcc);

  it('initCollection targets lending program', () => {
    const ix = LendingProgram.initCollection(collection, payer, 'FrogCollection');
    expect(ix.programId.equals(PROGRAM_IDS.lending)).toBe(true);
    expect(ix.data[0]).toBe(0);
  });

  it('initCollection encodes collection name', () => {
    const ix = LendingProgram.initCollection(collection, payer, 'TestCollection');
    // After discriminant(1), 4-byte length, then UTF-8 bytes
    const nameBytes = new TextEncoder().encode('TestCollection');
    const lengthView = new DataView(ix.data.buffer, 1, 4);
    expect(lengthView.getUint32(0, true)).toBe(nameBytes.length);
  });

  it('placeBid has discriminant 1', () => {
    const ix = LendingProgram.placeBid(
      collection, lender,
      txid, 0,
      100_000n, 2_592_000n, 1000n,
      'tb1q_lender_ordinals', 'tb1q_lender_payments',
    );
    expect(ix.data[0]).toBe(1);
    expect(ix.accounts).toHaveLength(2);
    expect(ix.accounts[1].isSigner).toBe(true); // lender signs
  });

  it('placeBid encodes txid correctly', () => {
    const ix = LendingProgram.placeBid(
      collection, lender,
      txid, 1,
      50_000n, 86_400n, 0n,
      'addr1', 'addr2',
    );
    // After discriminant(1), txid is at bytes 1..33
    expect(ix.data.slice(1, 33)).toEqual(txid);
  });

  it('acceptBid has discriminant 3', () => {
    const ix = LendingProgram.acceptBid(
      collection, borrower,
      'abc123i0', txid, 0,
      'tb1q_borrower_ordinals', 'tb1q_borrower_payments',
    );
    expect(ix.data[0]).toBe(3);
  });

  it('cancelBid has discriminant 2', () => {
    const ix = LendingProgram.cancelBid(collection, lender, txid, 0);
    expect(ix.data[0]).toBe(2);
  });

  it('repayLoan has discriminant 4', () => {
    const repayTxid = new Uint8Array(32).fill(0xdd);
    const ix = LendingProgram.repayLoan(collection, borrower, 'abc123i0', repayTxid, 0, 100_000n);
    expect(ix.data[0]).toBe(4);
  });

  it('claimDefault has discriminant 5', () => {
    const ix = LendingProgram.claimDefault(collection, lender, 'abc123i0');
    expect(ix.data[0]).toBe(5);
    expect(ix.accounts[1].isSigner).toBe(true); // lender signs
  });
});
