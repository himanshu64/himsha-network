use anyhow::Result;
use borsh::BorshDeserialize;
use himsha_runtime::{
    account::{AccountInfo, StoredAccount},
    pubkey::Pubkey,
    utxo::UtxoMeta,
};
use redb::{Database, ReadableTable, TableDefinition};
use std::sync::Arc;

// All values stored as raw byte slices (redb 1.x Value impl for &[u8])
const ACCOUNTS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("accounts");
const PROGRAMS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("programs");
const IMAGE_IDS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("image_ids");
const UTXO_ANCHORS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("utxo_anchors");
const BLOCKS: TableDefinition<u64, &[u8]> = TableDefinition::new("blocks");
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");

// ---- secondary indexes (the "indexer DB" backing the explorer & breadth RPCs) ----
// owner(32)‖key(32) -> []  : range-scan an owner prefix to list its accounts in O(matches)
const OWNER_IDX: TableDefinition<&[u8], &[u8]> = TableDefinition::new("owner_idx");
// himsha_txid(32) -> slot   : O(1) tx → block lookup, no block scan
const TX_IDX: TableDefinition<&[u8], u64> = TableDefinition::new("tx_idx");
// bitcoin_txid(32) -> himsha_txid(32) : map a settlement's on-chain txid back to its L2 tx
const BTC_TX_IDX: TableDefinition<&[u8], &[u8]> = TableDefinition::new("btc_tx_idx");

/// Thread-safe node state.  `redb::Database` is `Send + Sync`.
#[derive(Clone)]
pub struct NodeState {
    db: Arc<Database>,
}

impl NodeState {
    pub fn open(path: &str) -> Result<Self> {
        let db = Database::create(path)?;
        let w = db.begin_write()?;
        w.open_table(ACCOUNTS)?;
        w.open_table(PROGRAMS)?;
        w.open_table(IMAGE_IDS)?;
        w.open_table(UTXO_ANCHORS)?;
        w.open_table(BLOCKS)?;
        w.open_table(META)?;
        w.open_table(OWNER_IDX)?;
        w.open_table(TX_IDX)?;
        w.open_table(BTC_TX_IDX)?;
        w.commit()?;
        let s = Self { db: Arc::new(db) };
        s.backfill_owner_index()?; // migrate pre-index DBs (no-op when already built)
        Ok(s)
    }

    /// Populate `OWNER_IDX` + `account_count` from a one-time scan of `ACCOUNTS`, but
    /// only when the index is empty and accounts exist (i.e. a DB created before the
    /// index, or first boot after upgrade). Steady-state this is a cheap empty-check.
    fn backfill_owner_index(&self) -> Result<()> {
        let rtx = self.db.begin_read()?;
        let oi_len = rtx.open_table(OWNER_IDX)?.len()?;
        let acc_len = rtx.open_table(ACCOUNTS)?.len()?;
        if oi_len > 0 || acc_len == 0 {
            return Ok(()); // already indexed, or nothing to index
        }
        let mut pairs: Vec<([u8; 32], [u8; 32])> = Vec::new();
        {
            let acc = rtx.open_table(ACCOUNTS)?;
            for entry in acc.iter()? {
                let (k, v) = entry?;
                let stored = StoredAccount::try_from_slice(v.value())?;
                let mut key = [0u8; 32];
                key.copy_from_slice(k.value());
                pairs.push((stored.owner, key));
            }
        }
        drop(rtx);
        let count = pairs.len() as u64;
        let w = self.db.begin_write()?;
        {
            let mut oi = w.open_table(OWNER_IDX)?;
            for (owner, key) in &pairs {
                let composite = [owner.as_slice(), key.as_slice()].concat();
                oi.insert(composite.as_slice(), [].as_slice())?;
            }
            let mut m = w.open_table(META)?;
            m.insert("account_count", count)?;
        }
        w.commit()?;
        Ok(())
    }

    // Helper: read raw bytes for a key, return owned Vec<u8> or None.
    fn read_bytes(
        db: &Database,
        tbl: TableDefinition<&[u8], &[u8]>,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let rtx = db.begin_read()?;
        let table = rtx.open_table(tbl)?;
        let guard = table.get(key)?; // Option<AccessGuard>
        let bytes = guard.map(|ag| ag.value().to_vec()); // owned copy
        drop(table); // release borrow of rtx
        drop(rtx);
        Ok(bytes)
    }

    fn read_u64_tbl(
        db: &Database,
        tbl: TableDefinition<u64, &[u8]>,
        key: u64,
    ) -> Result<Option<Vec<u8>>> {
        let rtx = db.begin_read()?;
        let table = rtx.open_table(tbl)?;
        let guard = table.get(key)?;
        let bytes = guard.map(|ag| ag.value().to_vec());
        drop(table);
        drop(rtx);
        Ok(bytes)
    }

    // ---- accounts ----

    pub fn save_account(&self, key: &Pubkey, account: &StoredAccount) -> Result<()> {
        let bytes = borsh::to_vec(account)?;
        let new_owner = account.owner;
        let w = self.db.begin_write()?;
        {
            // Insert the account, capturing the prior owner (if any) for index upkeep.
            let prior_owner: Option<[u8; 32]> = {
                let mut t = w.open_table(ACCOUNTS)?;
                let prior = {
                    let g = t.get(key.as_ref())?;
                    g.and_then(|ag| StoredAccount::try_from_slice(ag.value()).ok())
                        .map(|s| s.owner)
                };
                t.insert(key.as_ref(), bytes.as_slice())?;
                prior
            };

            // Maintain OWNER_IDX (owner‖key). Drop a stale entry if the owner changed.
            {
                let mut oi = w.open_table(OWNER_IDX)?;
                if let Some(po) = prior_owner {
                    if po != new_owner {
                        let old = [po.as_slice(), key.as_ref()].concat();
                        oi.remove(old.as_slice())?;
                    }
                }
                let new = [new_owner.as_slice(), key.as_ref()].concat();
                oi.insert(new.as_slice(), [].as_slice())?;
            }

            // Count distinct accounts (only on first insert of a key).
            if prior_owner.is_none() {
                let mut m = w.open_table(META)?;
                let c = m.get("account_count")?.map(|ag| ag.value()).unwrap_or(0);
                m.insert("account_count", c + 1)?;
            }
        }
        w.commit()?;
        Ok(())
    }

    pub fn load_account(&self, key: &Pubkey) -> Result<Option<StoredAccount>> {
        let bytes = Self::read_bytes(&self.db, ACCOUNTS, key.as_ref())?;
        Ok(match bytes {
            Some(b) => Some(StoredAccount::try_from_slice(&b)?),
            None => None,
        })
    }

    pub fn account_exists(&self, key: &Pubkey) -> bool {
        self.load_account(key).ok().flatten().is_some()
    }

    /// Return all accounts owned by `owner`, using the `OWNER_IDX` prefix range —
    /// O(matches), not a full-table scan. Backs the `himsha_getProgramAccounts` RPC.
    pub fn accounts_by_owner(&self, owner: &Pubkey) -> Result<Vec<AccountInfo>> {
        let owner_bytes: [u8; 32] = (*owner).into();
        let lower = [owner_bytes.as_slice(), [0u8; 32].as_slice()].concat();
        let upper = [owner_bytes.as_slice(), [0xffu8; 32].as_slice()].concat();

        let mut keys: Vec<Pubkey> = Vec::new();
        {
            let rtx = self.db.begin_read()?;
            let idx = rtx.open_table(OWNER_IDX)?;
            for entry in idx.range::<&[u8]>(lower.as_slice()..=upper.as_slice())? {
                let (k, _) = entry?;
                let kb = k.value();
                if kb.len() == 64 && kb[..32] == owner_bytes {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&kb[32..64]);
                    keys.push(Pubkey::from(key));
                }
            }
        }

        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(stored) = self.load_account(&key)? {
                out.push(stored.into_account(key));
            }
        }
        Ok(out)
    }

    /// All accounts (bounded by `limit`, 0 = unbounded). Backs `himsha_getAllAccounts`.
    /// This is an explicit full scan — the explorer pages it; programs should use
    /// [`accounts_by_owner`] (indexed) instead.
    pub fn all_accounts(&self, limit: usize) -> Result<Vec<AccountInfo>> {
        let rtx = self.db.begin_read()?;
        let table = rtx.open_table(ACCOUNTS)?;
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (k, v) = entry?;
            let stored = StoredAccount::try_from_slice(v.value())?;
            let mut key = [0u8; 32];
            key.copy_from_slice(k.value());
            out.push(stored.into_account(Pubkey::from(key)));
            if limit != 0 && out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    // ---- programs ----

    pub fn deploy_program(&self, id: &Pubkey, elf: &[u8], image_id: [u8; 32]) -> Result<()> {
        let w = self.db.begin_write()?;
        {
            let mut progs = w.open_table(PROGRAMS)?;
            progs.insert(id.as_ref(), elf)?;
            let mut imgs = w.open_table(IMAGE_IDS)?;
            imgs.insert(id.as_ref(), image_id.as_slice())?;
        }
        w.commit()?;
        Ok(())
    }

    pub fn load_program_elf(&self, id: &Pubkey) -> Result<Option<Vec<u8>>> {
        Self::read_bytes(&self.db, PROGRAMS, id.as_ref())
    }

    pub fn load_image_id(&self, id: &Pubkey) -> Result<Option<[u8; 32]>> {
        let bytes = Self::read_bytes(&self.db, IMAGE_IDS, id.as_ref())?;
        Ok(bytes.map(|b| {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }))
    }

    // ---- UTXO anchors ----

    pub fn set_utxo_anchor(&self, key: &Pubkey, utxo: &UtxoMeta) -> Result<()> {
        let bytes = borsh::to_vec(utxo)?;
        let w = self.db.begin_write()?;
        {
            let mut t = w.open_table(UTXO_ANCHORS)?;
            t.insert(key.as_ref(), bytes.as_slice())?;
        }
        w.commit()?;
        Ok(())
    }

    pub fn get_utxo_anchor(&self, key: &Pubkey) -> Result<Option<UtxoMeta>> {
        let bytes = Self::read_bytes(&self.db, UTXO_ANCHORS, key.as_ref())?;
        bytes
            .map(|b| UtxoMeta::try_from_slice(&b).map_err(Into::into))
            .transpose()
    }

    // ---- blocks / slots ----

    pub fn current_slot(&self) -> Result<u64> {
        let rtx = self.db.begin_read()?;
        let tbl = rtx.open_table(META)?;
        let slot = tbl.get("slot")?.map(|ag| ag.value()).unwrap_or(0);
        Ok(slot)
    }

    pub fn advance_slot(&self) -> Result<u64> {
        let w = self.db.begin_write()?;
        let new_slot = {
            let mut t = w.open_table(META)?;
            let cur = t.get("slot")?.map(|ag| ag.value()).unwrap_or(0);
            let next = cur + 1;
            t.insert("slot", next)?;
            next
        };
        w.commit()?;
        Ok(new_slot)
    }

    pub fn save_block(&self, slot: u64, block_bytes: Vec<u8>) -> Result<()> {
        let w = self.db.begin_write()?;
        {
            let mut t = w.open_table(BLOCKS)?;
            t.insert(slot, block_bytes.as_slice())?;
        }
        w.commit()?;
        Ok(())
    }

    pub fn load_block(&self, slot: u64) -> Result<Option<Vec<u8>>> {
        Self::read_u64_tbl(&self.db, BLOCKS, slot)
    }

    /// Persist a whole batch of accounts in a **single** write transaction —
    /// all-or-nothing. Used to commit a transaction's state changes atomically so a
    /// crash can never leave a partially-applied multi-instruction transaction.
    /// Mirrors [`save_account`](Self::save_account)'s OWNER_IDX / count upkeep.
    pub fn save_accounts_atomic(
        &self,
        accounts: &std::collections::HashMap<Pubkey, StoredAccount>,
    ) -> Result<()> {
        if accounts.is_empty() {
            return Ok(());
        }
        let w = self.db.begin_write()?;
        {
            let mut acc_tbl = w.open_table(ACCOUNTS)?;
            let mut owner_idx = w.open_table(OWNER_IDX)?;
            let mut added = 0u64;
            for (key, account) in accounts {
                let bytes = borsh::to_vec(account)?;
                let new_owner = account.owner;
                let prior_owner: Option<[u8; 32]> = {
                    let g = acc_tbl.get(key.as_ref())?;
                    g.and_then(|ag| StoredAccount::try_from_slice(ag.value()).ok())
                        .map(|s| s.owner)
                };
                acc_tbl.insert(key.as_ref(), bytes.as_slice())?;

                if let Some(po) = prior_owner {
                    if po != new_owner {
                        let old = [po.as_slice(), key.as_ref()].concat();
                        owner_idx.remove(old.as_slice())?;
                    }
                } else {
                    added += 1;
                }
                let new = [new_owner.as_slice(), key.as_ref()].concat();
                owner_idx.insert(new.as_slice(), [].as_slice())?;
            }
            if added > 0 {
                let mut m = w.open_table(META)?;
                let c = m.get("account_count")?.map(|ag| ag.value()).unwrap_or(0);
                m.insert("account_count", c + added)?;
            }
        }
        w.commit()?;
        Ok(())
    }

    /// The set of block hashes within the last `max_age` slots of the tip — the
    /// blockhashes a transaction may legally reference for replay protection. Older
    /// hashes have aged out and are rejected; the genesis block (slot 0) is included
    /// while the chain is younger than `max_age` blocks.
    pub fn recent_blockhashes(&self, max_age: u64) -> Result<std::collections::HashSet<[u8; 32]>> {
        let tip = self.current_slot()?;
        let mut set = std::collections::HashSet::new();
        for slot in tip.saturating_sub(max_age)..=tip {
            if let Some(bytes) = self.load_block(slot)? {
                if let Ok(block) =
                    serde_json::from_slice::<himsha_runtime::transaction::Block>(&bytes)
                {
                    set.insert(block.blockhash);
                }
            }
        }
        Ok(set)
    }

    // ---- transaction index ----

    /// Record `himsha_txid -> slot` and bump the tx counter. Idempotent per txid.
    pub fn index_transaction(&self, txid: &[u8; 32], slot: u64) -> Result<()> {
        let w = self.db.begin_write()?;
        {
            let mut t = w.open_table(TX_IDX)?;
            let is_new = t.get(txid.as_slice())?.is_none();
            t.insert(txid.as_slice(), slot)?;
            if is_new {
                let mut m = w.open_table(META)?;
                let c = m.get("tx_count")?.map(|ag| ag.value()).unwrap_or(0);
                m.insert("tx_count", c + 1)?;
            }
        }
        w.commit()?;
        Ok(())
    }

    /// Slot a transaction was included in, via the index (no block scan).
    pub fn tx_slot(&self, txid: &[u8; 32]) -> Result<Option<u64>> {
        let rtx = self.db.begin_read()?;
        let t = rtx.open_table(TX_IDX)?;
        let slot = t.get(txid.as_slice())?.map(|ag| ag.value());
        drop(t);
        drop(rtx);
        Ok(slot)
    }

    // ---- bitcoin settlement index ----

    /// Map an on-chain settlement `bitcoin_txid -> himsha_txid` (the L2 tx that caused it).
    pub fn index_btc_settlement(&self, btc_txid: &[u8; 32], himsha_txid: &[u8; 32]) -> Result<()> {
        let w = self.db.begin_write()?;
        {
            let mut t = w.open_table(BTC_TX_IDX)?;
            t.insert(btc_txid.as_slice(), himsha_txid.as_slice())?;
        }
        w.commit()?;
        Ok(())
    }

    /// HIMSHA txid that produced a given Bitcoin settlement txid (settlement lookup).
    pub fn himsha_txid_for_btc(&self, btc_txid: &[u8; 32]) -> Result<Option<[u8; 32]>> {
        let bytes = Self::read_bytes(&self.db, BTC_TX_IDX, btc_txid.as_slice())?;
        Ok(bytes.and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()))
    }

    // ---- explorer stats ----

    /// (accounts, transactions, tip_slot) counters for the explorer overview.
    pub fn stats(&self) -> Result<(u64, u64, u64)> {
        let rtx = self.db.begin_read()?;
        let m = rtx.open_table(META)?;
        let accounts = m.get("account_count")?.map(|ag| ag.value()).unwrap_or(0);
        let txs = m.get("tx_count")?.map(|ag| ag.value()).unwrap_or(0);
        let slot = m.get("slot")?.map(|ag| ag.value()).unwrap_or(0);
        drop(m);
        drop(rtx);
        Ok((accounts, txs, slot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_state(tag: &str) -> (NodeState, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("himsha-state-{tag}-{}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        (NodeState::open(path.to_str().unwrap()).unwrap(), path)
    }

    fn acct(owner: Pubkey, lamports: u64) -> StoredAccount {
        StoredAccount {
            lamports,
            data: vec![],
            owner: owner.into(),
            executable: false,
            utxo_txid: None,
            utxo_vout: None,
        }
    }

    #[test]
    fn test_owner_index_lists_program_accounts() {
        let (s, path) = tmp_state("owner");
        let prog_a = Pubkey::from_seed(b"prog-a");
        let prog_b = Pubkey::from_seed(b"prog-b");
        let k1 = Pubkey::from_seed(b"k1");
        let k2 = Pubkey::from_seed(b"k2");
        let k3 = Pubkey::from_seed(b"k3");
        s.save_account(&k1, &acct(prog_a, 10)).unwrap();
        s.save_account(&k2, &acct(prog_a, 20)).unwrap();
        s.save_account(&k3, &acct(prog_b, 30)).unwrap();

        let a = s.accounts_by_owner(&prog_a).unwrap();
        assert_eq!(a.len(), 2);
        assert!(a.iter().all(|x| x.owner == prog_a));
        assert_eq!(s.accounts_by_owner(&prog_b).unwrap().len(), 1);
        assert_eq!(s.all_accounts(0).unwrap().len(), 3);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_owner_index_follows_owner_change_and_counts_once() {
        let (s, path) = tmp_state("reassign");
        let a = Pubkey::from_seed(b"owner-a");
        let b = Pubkey::from_seed(b"owner-b");
        let k = Pubkey::from_seed(b"acct");
        s.save_account(&k, &acct(a, 1)).unwrap();
        s.save_account(&k, &acct(a, 2)).unwrap(); // same owner, balance bump
        assert_eq!(s.accounts_by_owner(&a).unwrap().len(), 1);

        s.save_account(&k, &acct(b, 3)).unwrap(); // reassign to owner b
        assert_eq!(
            s.accounts_by_owner(&a).unwrap().len(),
            0,
            "stale entry dropped"
        );
        assert_eq!(s.accounts_by_owner(&b).unwrap().len(), 1);

        let (accounts, _, _) = s.stats().unwrap();
        assert_eq!(accounts, 1, "the same key must count once across updates");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_save_accounts_atomic_batch() {
        let (s, path) = tmp_state("atomic");
        let prog = Pubkey::from_seed(b"prog");
        let mut batch = std::collections::HashMap::new();
        batch.insert(Pubkey::from_seed(b"a1"), acct(prog, 1));
        batch.insert(Pubkey::from_seed(b"a2"), acct(prog, 2));
        batch.insert(Pubkey::from_seed(b"a3"), acct(prog, 3));
        s.save_accounts_atomic(&batch).unwrap();

        // All three landed in one commit, indexed and counted.
        assert_eq!(s.accounts_by_owner(&prog).unwrap().len(), 3);
        assert_eq!(s.stats().unwrap().0, 3);
        assert_eq!(
            s.load_account(&Pubkey::from_seed(b"a2"))
                .unwrap()
                .unwrap()
                .lamports,
            2
        );

        // Re-committing existing keys (one reassigned to a new owner) keeps the count
        // stable and updates the owner index — same upkeep as save_account.
        let prog2 = Pubkey::from_seed(b"prog2");
        let mut update = std::collections::HashMap::new();
        update.insert(Pubkey::from_seed(b"a1"), acct(prog2, 9)); // reassign owner
        update.insert(Pubkey::from_seed(b"a2"), acct(prog, 20)); // same owner, bump
        s.save_accounts_atomic(&update).unwrap();
        assert_eq!(s.accounts_by_owner(&prog).unwrap().len(), 2); // a2, a3
        assert_eq!(s.accounts_by_owner(&prog2).unwrap().len(), 1); // a1
        assert_eq!(s.stats().unwrap().0, 3, "no double count on update");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_tx_and_btc_indexes() {
        let (s, path) = tmp_state("txidx");
        let tx1 = [1u8; 32];
        let tx2 = [2u8; 32];
        s.index_transaction(&tx1, 5).unwrap();
        s.index_transaction(&tx1, 5).unwrap(); // idempotent
        s.index_transaction(&tx2, 7).unwrap();
        assert_eq!(s.tx_slot(&tx1).unwrap(), Some(5));
        assert_eq!(s.tx_slot(&tx2).unwrap(), Some(7));
        assert_eq!(s.tx_slot(&[9u8; 32]).unwrap(), None);

        let btc = [0xabu8; 32];
        s.index_btc_settlement(&btc, &tx1).unwrap();
        assert_eq!(s.himsha_txid_for_btc(&btc).unwrap(), Some(tx1));

        let (_, txs, _) = s.stats().unwrap();
        assert_eq!(txs, 2, "tx counter dedupes by txid");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_backfill_owner_index_on_open() {
        // Write accounts directly to ACCOUNTS only (simulate a pre-index DB), then
        // reopen and confirm the owner index was backfilled.
        let path =
            std::env::temp_dir().join(format!("himsha-backfill-{}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let owner = Pubkey::from_seed(b"legacy-prog");
        let key = Pubkey::from_seed(b"legacy-acct");
        {
            let db = Database::create(path.to_str().unwrap()).unwrap();
            let w = db.begin_write().unwrap();
            {
                let mut t = w.open_table(ACCOUNTS).unwrap();
                t.insert(
                    key.as_ref(),
                    borsh::to_vec(&acct(owner, 42)).unwrap().as_slice(),
                )
                .unwrap();
            }
            w.commit().unwrap();
        }
        let s = NodeState::open(path.to_str().unwrap()).unwrap(); // triggers backfill
        assert_eq!(s.accounts_by_owner(&owner).unwrap().len(), 1);
        assert_eq!(s.stats().unwrap().0, 1);
        let _ = std::fs::remove_file(&path);
    }
}
