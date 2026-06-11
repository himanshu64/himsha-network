import { createHash } from 'crypto';
import { HimshaConnection, CustodyInfo } from '../connection';
import { StateProof, leafHash, verifyAccountInState } from '../stateProof';
import { HimshaPublicKey } from '../pubkey';

// These tests pin two contracts of the new RPC client methods, with no live
// node: (1) the node's snake_case wire shapes are remapped to the SDK's
// camelCase interfaces, and (2) a fetched proof actually verifies against the
// caller's own account bytes via verifyAccountInState.

function toHex(b: Uint8Array): string {
  return Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
}
function nodeHash(left: Uint8Array, right: Uint8Array): Uint8Array {
  const h = createHash('sha256');
  h.update(new Uint8Array([0x01]));
  h.update(left);
  h.update(right);
  return new Uint8Array(h.digest());
}

// Stub the private `call` so we control the exact on-wire JSON the node returns.
function connWith(call: jest.Mock): HimshaConnection {
  const conn = new HimshaConnection('http://localhost:0');
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (conn as any).call = call;
  return conn;
}

describe('getStateProof / getCustodyInfo (wire remap + verification)', () => {
  it('remaps snake_case StateProof to camelCase', async () => {
    const wire = {
      state_root: 'aa',
      leaf: 'bb',
      index: 3,
      siblings: ['cc', 'dd'],
      anchored_slot: 12,
      anchored_state_root: 'ee',
      anchored_btc_txid: 'ff',
    };
    const call = jest.fn().mockResolvedValue(wire);
    const conn = connWith(call);

    const proof = await conn.getStateProof('Hims11111111111111111111111111111111111111');

    expect(call).toHaveBeenCalledWith('himsha_getStateProof', [
      'Hims11111111111111111111111111111111111111',
    ]);
    expect(proof).toEqual<StateProof>({
      stateRoot: 'aa',
      leaf: 'bb',
      index: 3,
      siblings: ['cc', 'dd'],
      anchoredSlot: 12,
      anchoredStateRoot: 'ee',
      anchoredBtcTxid: 'ff',
    });
  });

  it('returns null when the account has no proof', async () => {
    const conn = connWith(jest.fn().mockResolvedValue(null));
    await expect(
      conn.getStateProof('Hims11111111111111111111111111111111111111'),
    ).resolves.toBeNull();
  });

  it('defaults missing anchor fields to null', async () => {
    const conn = connWith(
      jest.fn().mockResolvedValue({ state_root: 'aa', leaf: 'bb', index: 0, siblings: [] }),
    );
    const proof = await conn.getStateProof('Hims11111111111111111111111111111111111111');
    expect(proof?.anchoredSlot).toBeNull();
    expect(proof?.anchoredStateRoot).toBeNull();
    expect(proof?.anchoredBtcTxid).toBeNull();
  });

  it('remaps snake_case CustodyInfo to camelCase', async () => {
    const call = jest.fn().mockResolvedValue({
      threshold: 2,
      total: 3,
      group_key: 'abcd',
      address: 'bc1pexample',
    });
    const conn = connWith(call);

    const info = await conn.getCustodyInfo();

    expect(call).toHaveBeenCalledWith('himsha_getCustodyInfo');
    expect(info).toEqual<CustodyInfo>({
      threshold: 2,
      total: 3,
      groupKey: 'abcd',
      address: 'bc1pexample',
    });
  });

  it('getCustodyInfo returns null when custody is unconfigured', async () => {
    const conn = connWith(jest.fn().mockResolvedValue(null));
    await expect(conn.getCustodyInfo()).resolves.toBeNull();
  });

  it('getAndVerifyAccountProof verifies a fetched proof against the caller bytes', async () => {
    // Build a real 2-leaf Merkle tree so the proof exercises a sibling.
    const key = HimshaPublicKey.fromSeed('account-under-test');
    const accountBytes = new Uint8Array([1, 2, 3, 4]);
    const myLeaf = leafHash(key.toBytes(), accountBytes); // index 0

    const siblingKey = HimshaPublicKey.fromSeed('other-account');
    const siblingLeaf = leafHash(siblingKey.toBytes(), new Uint8Array([9, 9])); // index 1

    const root = nodeHash(myLeaf, siblingLeaf);

    const wire = {
      state_root: toHex(root),
      leaf: toHex(myLeaf),
      index: 0,
      siblings: [toHex(siblingLeaf)],
      anchored_slot: null,
      anchored_state_root: null,
      anchored_btc_txid: null,
    };
    const conn = connWith(jest.fn().mockResolvedValue(wire));

    // sanity: the fetched+remapped proof verifies via the standalone helper too
    const proof = (await conn.getStateProof(key))!;
    expect(verifyAccountInState(key.toBytes(), accountBytes, proof, proof.stateRoot)).toBe(true);

    await expect(conn.getAndVerifyAccountProof(key, accountBytes)).resolves.toBe(true);
    // wrong account bytes must fail verification
    await expect(
      conn.getAndVerifyAccountProof(key, new Uint8Array([5, 5, 5])),
    ).resolves.toBe(false);
  });

  it('getAndVerifyAccountProof returns false when no proof exists', async () => {
    const conn = connWith(jest.fn().mockResolvedValue(null));
    const key = HimshaPublicKey.fromSeed('missing');
    await expect(conn.getAndVerifyAccountProof(key, new Uint8Array([1]))).resolves.toBe(false);
  });
});
