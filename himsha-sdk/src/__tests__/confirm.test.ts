import { HimshaConnection, SignatureStatus } from '../connection';

// confirmTransaction polls getSignatureStatus; we stub that method so the test
// needs no live node. This pins the async-execution contract: a succeeded tx
// resolves with its slot, a failed tx rejects with the reason (not a timeout).
function connWithStatuses(seq: Array<SignatureStatus | null>): HimshaConnection {
  const conn = new HimshaConnection('http://localhost:0');
  let i = 0;
  jest
    .spyOn(conn, 'getSignatureStatus')
    .mockImplementation(async () => seq[Math.min(i++, seq.length - 1)]);
  return conn;
}

describe('confirmTransaction (async execution status)', () => {
  it('resolves with the slot when the tx succeeds', async () => {
    const conn = connWithStatuses([{ status: 'succeeded', slot: 42 }]);
    await expect(conn.confirmTransaction('abcd', 2_000)).resolves.toBe(42n);
  });

  it('rejects with the failure reason when the tx fails', async () => {
    const conn = connWithStatuses([
      { status: 'failed', slot: 9, error: 'insufficient lamports' },
    ]);
    await expect(conn.confirmTransaction('abcd', 2_000)).rejects.toThrow(
      /failed at slot 9: insufficient lamports/,
    );
  });

  it('keeps polling while pending, then resolves on success', async () => {
    const conn = connWithStatuses([
      { status: 'pending' },
      { status: 'pending' },
      { status: 'succeeded', slot: 7 },
    ]);
    await expect(conn.confirmTransaction('abcd', 5_000)).resolves.toBe(7n);
  });

  it('times out when the status never resolves', async () => {
    const conn = connWithStatuses([null]);
    await expect(conn.confirmTransaction('abcd', 800)).rejects.toThrow(/not confirmed within/);
  });
});
