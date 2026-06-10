import WebSocket from 'ws';
import { HimshaPublicKey } from './pubkey';
import { HimshaTransaction } from './transaction';

interface RpcRequest {
  jsonrpc: '2.0';
  id: number;
  method: string;
  params: unknown[];
}

interface RpcResponse<T> {
  jsonrpc: string;
  id: number;
  result?: T;
  error?: { code: number; message: string };
}

export interface AccountInfo {
  key: string;
  lamports: string;   // bigint serialized as string
  data: string;       // hex
  owner: string;
  executable: boolean;
  utxo?: { txid: string; vout: number };
}

export interface UtxoInfo {
  meta: { txid: string; vout: number };
  value: string;
  scriptPubkey: string;
  confirmations: number;
}

export interface Block {
  slot: string;
  parentSlot: string;
  blockhash: string;
  timestamp: string;
  transactions: unknown[];
}

export interface NodeStats {
  accounts: number;
  transactions: number;
  tip_slot: number;
  programs: number;
}

/** Execution status of a submitted transaction (himsha_getSignatureStatus). */
export interface SignatureStatus {
  status: 'pending' | 'succeeded' | 'failed';
  slot?: number | null;
  error?: string | null;
}

type SubscriptionId = number;
type AccountCallback = (info: AccountInfo) => void;
type SlotCallback    = (slot: bigint)      => void;

/**
 * HIMSHA Network JSON-RPC connection.
 *
 * @example
 * ```ts
 * const conn = new HimshaConnection('http://localhost:9100');
 * const ready = await conn.isNodeReady();
 * console.log('ready:', ready);
 * ```
 */
export class HimshaConnection {
  private nextId = 1;
  private ws?: WebSocket;
  private wsCallbacks = new Map<SubscriptionId, AccountCallback | SlotCallback>();

  constructor(private readonly endpoint: string) {}

  // ---- low-level RPC ----

  private async call<T>(method: string, params: unknown[] = []): Promise<T> {
    const id = this.nextId++;
    const body: RpcRequest = { jsonrpc: '2.0', id, method, params };

    const res = await fetch(this.endpoint, {
      method:  'POST',
      headers: { 'Content-Type': 'application/json' },
      body:    JSON.stringify(body),
    });

    if (!res.ok) {
      throw new Error(`HTTP ${res.status}: ${res.statusText}`);
    }

    const json = (await res.json()) as RpcResponse<T>;
    if (json.error) {
      throw new Error(`RPC error ${json.error.code}: ${json.error.message}`);
    }
    if (json.result === undefined) {
      throw new Error('RPC returned null result');
    }
    return json.result;
  }

  // ---- Node queries ----

  /** Returns true when the node is fully initialized and ready to serve requests. */
  async isNodeReady(): Promise<boolean> {
    return this.call<boolean>('himsha_isNodeReady');
  }

  /** Returns the current committed slot number. */
  async getSlot(): Promise<bigint> {
    const slot = await this.call<string>('himsha_getSlot');
    return BigInt(slot);
  }

  /** Fetch a block by slot. */
  async getBlock(slot: bigint): Promise<Block | null> {
    return this.call<Block | null>('himsha_getBlock', [slot.toString()]);
  }

  /** List all deployed program IDs. */
  async listPrograms(): Promise<string[]> {
    return this.call<string[]>('himsha_listPrograms');
  }

  // ---- Account queries ----

  /** Fetch account state by public key. Returns null if not found. */
  async getAccountInfo(pubkey: HimshaPublicKey | string): Promise<AccountInfo | null> {
    const key = typeof pubkey === 'string' ? pubkey : pubkey.toBase58();
    return this.call<AccountInfo | null>('himsha_getAccountInfo', [key]);
  }

  /** Return all accounts owned by a program. */
  async getProgramAccounts(programId: HimshaPublicKey | string): Promise<AccountInfo[]> {
    const id = typeof programId === 'string' ? programId : programId.toBase58();
    return this.call<AccountInfo[]>('himsha_getProgramAccounts', [id]);
  }

  /** Check whether an account exists. */
  async accountExists(pubkey: HimshaPublicKey | string): Promise<boolean> {
    const info = await this.getAccountInfo(pubkey);
    return info !== null;
  }

  /** Parse account data as JSON (helper — caller must know the layout). */
  async getAccountData<T>(pubkey: HimshaPublicKey | string): Promise<T | null> {
    const info = await this.getAccountInfo(pubkey);
    if (!info) return null;
    const bytes = Buffer.from(info.data, 'hex');
    return JSON.parse(bytes.toString('utf8')) as T;
  }

  // ---- Bitcoin ----

  /** Fetch UTXO info from the Bitcoin indexer. */
  async getUtxo(txid: string, vout: number): Promise<UtxoInfo | null> {
    return this.call<UtxoInfo | null>('himsha_getUtxo', [txid, vout]);
  }

  // ---- breadth: batch reads, faucet, tx lookup, introspection ----

  /** Dev faucet: credit `lamports` to `pubkey`. Returns the new balance. */
  async requestAirdrop(pubkey: HimshaPublicKey | string, lamports: bigint): Promise<bigint> {
    const key = typeof pubkey === 'string' ? pubkey : pubkey.toBase58();
    return this.call<bigint>('himsha_requestAirdrop', [key, lamports]);
  }

  /** Dev faucet: create a fresh funded account (`space` bytes). Fails if it exists. */
  async createAccountWithFaucet(
    pubkey: HimshaPublicKey | string, lamports: bigint, space: bigint = 0n,
  ): Promise<AccountInfo> {
    const key = typeof pubkey === 'string' ? pubkey : pubkey.toBase58();
    return this.call<AccountInfo>('himsha_createAccountWithFaucet', [key, lamports, space]);
  }

  /** Batch account read — one entry per key, `null` where missing. */
  async getMultipleAccounts(pubkeys: Array<HimshaPublicKey | string>): Promise<Array<AccountInfo | null>> {
    const keys = pubkeys.map(p => (typeof p === 'string' ? p : p.toBase58()));
    return this.call<Array<AccountInfo | null>>('himsha_getMultipleAccounts', [keys]);
  }

  /** Look up a processed transaction by id (hex message hash) across recent blocks. */
  async getProcessedTransaction(txid: string): Promise<unknown | null> {
    return this.call<unknown | null>('himsha_getProcessedTransaction', [txid]);
  }

  /** Node software version. */
  async getVersion(): Promise<string> {
    return this.call<string>('himsha_getVersion');
  }

  /** Connected peers (followers/primary). */
  async getPeers(): Promise<string[]> {
    return this.call<string[]>('himsha_getPeers');
  }

  /** Derive the Bitcoin (Taproot) address linked to an account public key. */
  async getAccountAddress(pubkey: HimshaPublicKey | string): Promise<string> {
    const key = typeof pubkey === 'string' ? pubkey : pubkey.toBase58();
    return this.call<string>('himsha_getAccountAddress', [key]);
  }

  /** Recent processed transactions, newest first, up to `limit`. */
  async recentTransactions(limit: number): Promise<unknown[]> {
    return this.call<unknown[]>('himsha_recentTransactions', [limit]);
  }

  /** Block hash (hex) at a slot. */
  async getBlockHash(slot: bigint): Promise<string | null> {
    return this.call<string | null>('himsha_getBlockHash', [slot]);
  }

  /** Hash (hex) of the current tip block. */
  async getBestBlockHash(): Promise<string | null> {
    return this.call<string | null>('himsha_getBestBlockHash');
  }

  /** The node's configured network identity key (hex). */
  async getNetworkPubkey(): Promise<string> {
    return this.call<string>('himsha_getNetworkPubkey');
  }

  // ---- Lightning Network (requires LND configured on the node) ----

  /** Create a BOLT-11 invoice for `amountSat`; returns the payment request. */
  async createInvoice(amountSat: bigint, memo = ''): Promise<string> {
    return this.call<string>('himsha_createInvoice', [amountSat, memo]);
  }

  /** Pay a BOLT-11 invoice; returns the payment hash. */
  async payInvoice(bolt11: string): Promise<string> {
    return this.call<string>('himsha_payInvoice', [bolt11]);
  }

  /** Spendable Lightning channel balance, in sats. */
  async lightningBalance(): Promise<bigint> {
    return this.call<bigint>('himsha_lightningBalance');
  }

  // ---- Indexer-backed queries (explorer / breadth) ----

  /** All accounts in state, bounded by `limit` (0 = unbounded). */
  async getAllAccounts(limit = 0): Promise<AccountInfo[]> {
    return this.call<AccountInfo[]>('himsha_getAllAccounts', [limit]);
  }

  /** HIMSHA txid (hex) that produced a given Bitcoin settlement txid, or null. */
  async getTxidFromBtcTxid(btcTxid: string): Promise<string | null> {
    return this.call<string | null>('himsha_getTxidFromBtcTxid', [btcTxid]);
  }

  /** Aggregate chain stats for dashboards (indexed counters, no scans). */
  async getStats(): Promise<NodeStats> {
    return this.call<NodeStats>('himsha_getStats');
  }

  // ---- Transactions ----

  /**
   * Submit a signed transaction.
   * Returns the transaction ID (hex SHA-256 of the message hash).
   */
  async sendTransaction(tx: HimshaTransaction): Promise<string> {
    return this.call<string>('himsha_sendTransaction', [tx.toJSON()]);
  }

  /** Submit a batch of transactions; returns a tx id per input (in order). */
  async sendTransactions(txs: HimshaTransaction[]): Promise<string[]> {
    return this.call<string[]>('himsha_sendTransactions', [txs.map(t => t.toJSON())]);
  }

  /**
   * Deploy a compiled RISC-V ELF binary as a new HIMSHA program.
   * Returns the new program's public key (base58).
   */
  async deployProgram(elfHex: string, imageIdHex: string): Promise<string> {
    return this.call<string>('himsha_deployProgram', [elfHex, imageIdHex]);
  }

  /**
   * Execution status of a submitted transaction, or `null` if the node has
   * never seen the id. Since execution happens at block production (not at
   * submit time), this is how a client learns the authoritative outcome.
   */
  async getSignatureStatus(txId: string): Promise<SignatureStatus | null> {
    return this.call<SignatureStatus | null>('himsha_getSignatureStatus', [txId]);
  }

  /**
   * Poll until a transaction is executed. Resolves with the slot once it
   * `succeeded`; **rejects with the failure reason** if it `failed` (no more
   * silent timeouts on a rejected tx); keeps waiting while `pending`.
   */
  async confirmTransaction(txId: string, timeoutMs = 30_000): Promise<bigint> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const st = await this.getSignatureStatus(txId);
      if (st?.status === 'succeeded') return BigInt(st.slot ?? 0);
      if (st?.status === 'failed') {
        throw new Error(
          `Transaction ${txId} failed${st.slot != null ? ` at slot ${st.slot}` : ''}: ${st.error ?? 'unknown error'}`,
        );
      }
      await new Promise(r => setTimeout(r, 500));
    }
    throw new Error(`Transaction ${txId} not confirmed within ${timeoutMs}ms`);
  }

  // ---- WebSocket subscriptions ----

  private connectWs(): void {
    if (this.ws) return;
    const wsUrl = this.endpoint.replace(/^http/, 'ws');
    this.ws = new WebSocket(wsUrl);
    this.ws.on('message', (raw) => {
      try {
        const msg = JSON.parse(raw.toString());
        const cb = this.wsCallbacks.get(msg.id);
        if (cb) cb(msg.params);
      } catch { /* ignore */ }
    });
    this.ws.on('close', () => {
      this.ws = undefined;
      // Auto-reconnect after 2 seconds
      setTimeout(() => this.connectWs(), 2000);
    });
  }

  /**
   * Subscribe to account changes. Returns a subscription ID.
   * Call `removeListener(id)` to unsubscribe.
   */
  onAccountChange(pubkey: HimshaPublicKey, callback: AccountCallback): SubscriptionId {
    this.connectWs();
    const id = this.nextId++;
    this.wsCallbacks.set(id, callback);
    this.ws?.send(JSON.stringify({
      jsonrpc: '2.0', id,
      method: 'himsha_subscribeAccount',
      params: [pubkey.toBase58()],
    }));
    return id;
  }

  /** Subscribe to slot changes. */
  onSlotChange(callback: SlotCallback): SubscriptionId {
    this.connectWs();
    const id = this.nextId++;
    this.wsCallbacks.set(id, (slot: unknown) => callback(BigInt(slot as string)));
    this.ws?.send(JSON.stringify({
      jsonrpc: '2.0', id,
      method: 'himsha_subscribeSlot',
      params: [],
    }));
    return id;
  }

  /** Remove a subscription. */
  removeListener(id: SubscriptionId): void {
    this.wsCallbacks.delete(id);
  }

  /** Close the connection. */
  close(): void {
    this.ws?.close();
    this.ws = undefined;
  }
}
