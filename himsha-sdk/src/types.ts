// ============================================================
// HIMSHA Network SDK — Core Types
// ============================================================

export interface PublicKey {
  toBase58(): string;
  toBytes(): Uint8Array;
}

export interface UtxoMeta {
  txid: Uint8Array; // 32 bytes, little-endian
  vout: number;
}

export interface UtxoInfo {
  meta: UtxoMeta;
  value: bigint;          // satoshis
  scriptPubkey: string;   // hex
  confirmations: number;
}

export interface AccountMeta {
  pubkey: string; // base58
  isSigner: boolean;
  isWritable: boolean;
}

export interface Instruction {
  programId: string;   // base58
  accounts: AccountMeta[];
  data: Uint8Array;
}

export interface Message {
  signers: string[];       // base58 pubkeys
  instructions: Instruction[];
  timestamp: bigint;       // unix seconds
}

export interface Signature {
  bytes: Uint8Array; // 64 bytes
  toHex(): string;
}

export interface RuntimeTransaction {
  version: number;
  signatures: Signature[];
  message: Message;
}

export interface AccountInfo {
  key: string;          // base58
  lamports: bigint;
  data: Uint8Array;
  owner: string;        // base58
  executable: boolean;
  utxo?: UtxoMeta;
}

export interface Block {
  slot: bigint;
  parentSlot: bigint;
  transactions: RuntimeTransaction[];
  blockhash: Uint8Array; // 32 bytes
  timestamp: bigint;
}

export interface TransactionStatus {
  confirmed: boolean;
  slot?: bigint;
  error?: string;
}

// ---- Subscription events ----

export interface SlotInfo {
  slot: bigint;
  timestamp: bigint;
}

export type AccountChangeCallback = (info: AccountInfo) => void;
export type SlotChangeCallback    = (info: SlotInfo)    => void;
