use bitcoin::hashes::sha256d;
use candid::Principal;
use ciborium::{from_reader, into_writer};
use ck_doge_types::{
    block::{Block, BlockHash},
    canister::*,
    chainparams::{chain_from_key_bits, ChainParams, KeyBits},
    script, transaction,
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
};

use crate::{
    ecdsa::{
        account_path, derive_public_key, proxy_token_public_key, public_key_with, ECDSAPublicKey,
    },
    Account,
};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct State {
    /// The Dogecoin network that the minter will connect to
    pub chain: KeyBits,

    /// The name of the [EcdsaKeyId]. Use "dfx_test_key" for local replica and "test_key_1" for
    /// a testing key for testnet and mainnet
    pub ecdsa_key_name: String,

    /// The Minter ECDSA public key
    pub ecdsa_public_key: Option<ECDSAPublicKey>,

    pub rpc_proxy_public_key: String,
    pub rpc_task_id: Option<u8>,

    /// The minimum number of confirmations on the Dogecoin chain.
    pub min_confirmations: u32,

    pub tip_height: u64,
    pub tip_blockhash: [u8; 32],

    pub processed_height: u64,
    pub processed_blockhash: [u8; 32],

    pub confirmed_height: u64,
    pub confirmed_blockhash: [u8; 32],

    pub start_height: u64,
    pub start_blockhash: [u8; 32],

    pub pull_block_retries: u32,
    pub last_errors: Vec<String>,

    pub managers: BTreeSet<Principal>,
    pub rpc_agent: RPCAgent,

    unconfirmed_utxs: BTreeMap<[u8; 32], UnspentTxState>,
    unconfirmed_utxos: BTreeMap<[u8; 21], (UtxoStates, UtxoStates)>,
}

impl State {
    pub fn chain_params(&self) -> &'static ChainParams {
        chain_from_key_bits(self.chain)
    }
}

impl Storable for State {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode MinterState data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode MinterState data")
    }
}

// unspent transaction outputs (UTXOs)
// txid -> UnspentTx
#[derive(Clone, Deserialize, Serialize)]
pub struct UnspentTxState(pub u64, pub Vec<ByteBuf>, pub Vec<Option<(u64, Txid)>>);

impl From<UnspentTxState> for UnspentTx {
    fn from(uts: UnspentTxState) -> Self {
        UnspentTx {
            height: uts.0,
            output: uts.1,
            spent: uts.2.into_iter().map(|v| v.map(|(_, id)| id)).collect(),
        }
    }
}

impl Storable for UnspentTxState {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode MinterState data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode MinterState data")
    }
}

// address -> UnspentOutput
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtxoState(pub u64, pub u64, pub [u8; 32], pub u32, pub u64);

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct UtxoStates(BTreeSet<UtxoState>);

impl From<UtxoState> for Utxo {
    fn from(uts: UtxoState) -> Self {
        Utxo {
            height: uts.0, // uts.1: spent height
            txid: Txid(uts.2),
            vout: uts.3,
            value: uts.4,
        }
    }
}

impl From<UtxoStates> for Vec<Utxo> {
    fn from(uts: UtxoStates) -> Self {
        uts.0.into_iter().map(Utxo::from).collect()
    }
}

impl Storable for UtxoStates {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode MinterState data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode MinterState data")
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const UT_MEMORY_ID: MemoryId = MemoryId::new(1);
const XO_MEMORY_ID: MemoryId = MemoryId::new(2);

#[derive(Default)]
pub struct RuntimeState {
    pub sync_job_running: i8, // 0: not running, > 0: running, < 0: stop because of error
    pub update_proxy_token_interval: Option<ic_cdk_timers::TimerId>,
}

thread_local! {
    static STATE_HEAP: RefCell<State> = RefCell::new(State::default());

    static RUNTIME_STATE: RefCell<RuntimeState> = RefCell::new(RuntimeState::default());

    static UNPROCESSED_BLOCKS: RefCell<VecDeque<(u64, BlockHash, Block)>> =const { RefCell::new(VecDeque::new()) };

    static PROCESSED_BLOCKS: RefCell<VecDeque<(u64, BlockHash)>> =const { RefCell::new(VecDeque::new()) };

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            State::default()
        ).expect("failed to init STATE store")
    );

    // txid -> unspent tx
    static UT: RefCell<StableBTreeMap<[u8; 32], UnspentTxState, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(UT_MEMORY_ID)),
        )
    );

    // address -> unspent output
    static XO: RefCell<StableBTreeMap<[u8; 21], UtxoStates, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(XO_MEMORY_ID)),
        )
    );
}

pub mod state {
    use super::*;

    pub fn is_manager(caller: &Principal) -> bool {
        STATE_HEAP.with(|r| r.borrow().managers.contains(caller))
    }

    pub fn get_agent() -> RPCAgent {
        STATE_HEAP.with(|r| r.borrow().rpc_agent.clone())
    }

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE_HEAP.with(|r| f(&r.borrow()))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE_HEAP.with(|r| f(&mut r.borrow_mut()))
    }

    pub fn runtime<R>(f: impl FnOnce(&RuntimeState) -> R) -> R {
        RUNTIME_STATE.with(|r| f(&r.borrow()))
    }

    pub fn runtime_mut<R>(f: impl FnOnce(&mut RuntimeState) -> R) -> R {
        RUNTIME_STATE.with(|r| f(&mut r.borrow_mut()))
    }

    pub async fn init_ecdsa_public_key() {
        let ecdsa_key_name = with(|r| {
            if r.ecdsa_public_key.is_none() {
                Some(r.ecdsa_key_name.clone())
            } else {
                None
            }
        });

        if let Some(ecdsa_key_name) = ecdsa_key_name {
            let ecdsa_public_key = public_key_with(&ecdsa_key_name, vec![])
                .await
                .unwrap_or_else(|err| {
                    ic_cdk::trap(&format!("failed to retrieve ECDSA public key: {err}"))
                });
            with_mut(|r| {
                r.rpc_proxy_public_key = proxy_token_public_key(&ecdsa_public_key);
                r.ecdsa_public_key = Some(ecdsa_public_key);
            });
        }
    }

    pub fn load() {
        STATE.with(|r| {
            let s = r.borrow_mut().get().clone();
            STATE_HEAP.with(|h| {
                *h.borrow_mut() = s;
                let mut s = h.borrow_mut();
                // reset the tip to the last processed block
                s.tip_height = s.processed_height;
                s.tip_blockhash = s.processed_blockhash;
            });
        });
    }

    pub fn save() {
        STATE_HEAP.with(|h| {
            STATE.with(|r| {
                r.borrow_mut()
                    .set(h.borrow().clone())
                    .expect("failed to set STATE data");
            });
        });
    }
}

pub fn get_public_key(acc: &Account) -> Result<ECDSAPublicKey, String> {
    state::with(|s| {
        let pk = s.ecdsa_public_key.as_ref().ok_or("no ecdsa_public_key")?;
        Ok(derive_public_key(pk, account_path(acc)))
    })
}

pub fn get_address(acc: &Account) -> Result<Address, String> {
    state::with(|s| {
        let pk = s.ecdsa_public_key.as_ref().ok_or("no ecdsa_public_key")?;
        let pk = derive_public_key(pk, account_path(acc));
        script::p2pkh_address(&pk.public_key, s.chain_params()).map(|addr| addr.into())
    })
}

pub fn append_block(height: u64, hash: BlockHash, block: Block) -> Result<(), String> {
    state::with_mut(|s| {
        if height != 0 && s.tip_height + 1 != height {
            return Err(format!(
                "invalid block height, expected {}, got {}",
                s.tip_height + 1,
                height
            ));
        }
        if s.tip_blockhash != *block.header.prev_blockhash {
            return Err(format!(
                "invalid prev_blockhash at {}, expected {:?}, got {:?}",
                height,
                sha256d::Hash::from_bytes_ref(&s.tip_blockhash),
                block.header.prev_blockhash,
            ));
        }
        let h = block.block_hash();
        if hash != h {
            return Err(format!(
                "invalid block hash at {}, expected {:?}, got {:?}",
                height, hash, h,
            ));
        }

        UNPROCESSED_BLOCKS.with(|r| r.borrow_mut().push_back((height, hash, block)));
        s.tip_height = height;
        s.tip_blockhash = *hash;
        Ok(())
    })
}

// we should clear the unprocessed blocks because the block may be invalid to process
pub fn clear_for_restart_process_block() {
    UNPROCESSED_BLOCKS.with(|r| r.borrow_mut().clear());
    state::with_mut(|s| {
        s.tip_height = s.processed_height;
        s.tip_blockhash = s.processed_blockhash;
    });
}

// return true if a block processed
pub fn process_block() -> Result<bool, String> {
    state::with_mut(|s| {
        UNPROCESSED_BLOCKS.with(|r| {
            let mut q = r.borrow_mut();
            match q.pop_front() {
                None => Ok(false),
                Some((height, hash, block)) => {
                    if s.processed_height != 0 && height != s.processed_height + 1 {
                        return Err(format!(
                            "invalid block height to process, expected {}, got {}",
                            s.tip_height, height
                        ));
                    }

                    let chain = chain_from_key_bits(s.chain);
                    UT.with(|utr| {
                        let utm = utr.borrow();

                        for tx in block.txdata.iter().skip(1) {
                            let txid = Txid(*tx.compute_txid());

                            // process spent utxos
                            process_spent_tx(
                                &mut s.unconfirmed_utxs,
                                &mut s.unconfirmed_utxos,
                                &utm,
                                chain,
                                tx,
                                txid.clone(),
                                height,
                            )?;

                            // add unspent txouts
                            add_unspent_txouts(
                                &mut s.unconfirmed_utxs,
                                &mut s.unconfirmed_utxos,
                                chain,
                                tx,
                                txid,
                                height,
                            )?;
                        }
                        Ok::<(), String>(())
                    })?;

                    PROCESSED_BLOCKS.with(|r| r.borrow_mut().push_back((height, hash)));
                    s.processed_height = height;
                    s.processed_blockhash = *hash;
                    Ok(true)
                }
            }
        })
    })
}

// we should clear all unconfirmed states.
pub fn clear_for_restart_confirm_utxos() {
    UNPROCESSED_BLOCKS.with(|r| r.borrow_mut().clear());
    PROCESSED_BLOCKS.with(|r| r.borrow_mut().clear());
    state::with_mut(|s| {
        s.unconfirmed_utxs.clear();
        s.unconfirmed_utxos.clear();
        s.processed_height = s.confirmed_height;
        s.processed_blockhash = s.confirmed_blockhash;
        s.tip_height = s.processed_height;
        s.tip_blockhash = s.processed_blockhash;
    });
}

// return true if there are more blocks wait to process
pub fn confirm_utxos() -> Result<bool, String> {
    state::with_mut(|s| {
        let confirmed_height = s
            .processed_height
            .saturating_sub(s.min_confirmations as u64);
        if s.confirmed_height >= confirmed_height {
            return Ok(UNPROCESSED_BLOCKS.with(|r| !r.borrow().is_empty()));
        }

        let confirmed_blockhash = PROCESSED_BLOCKS.with(|r| {
            let mut q = r.borrow_mut();
            while let Some((height, hash)) = q.pop_front() {
                if height == confirmed_height {
                    return Ok(hash);
                }
            }
            Err(format!(
                "no processed blockhash at height {}",
                confirmed_height
            ))
        })?;

        UT.with(|utr| {
            XO.with(|xor| {
                flush_confirmed_utxos(
                    &mut s.unconfirmed_utxs,
                    &mut s.unconfirmed_utxos,
                    &mut utr.borrow_mut(),
                    &mut xor.borrow_mut(),
                    confirmed_height,
                )?;
                s.confirmed_height = confirmed_height;
                s.confirmed_blockhash = *confirmed_blockhash;
                Ok(UNPROCESSED_BLOCKS.with(|r| !r.borrow().is_empty()))
            })
        })
    })
}

pub fn get_tx(txid: &[u8; 32]) -> Option<UnspentTx> {
    state::with(|s| match s.unconfirmed_utxs.get(txid) {
        Some(utx) => Some(UnspentTx::from(utx.clone())),
        None => UT.with(|r| r.borrow().get(txid).map(UnspentTx::from)),
    })
}

pub fn list_uxtos(addr: &[u8; 21], take: usize, confirmed: bool) -> Vec<Utxo> {
    let mut res = XO.with(|r| r.borrow().get(addr).unwrap_or_default()).0;
    if !confirmed {
        state::with(|s| {
            if let Some((uts, sts)) = s.unconfirmed_utxos.get(addr) {
                for ts in sts.0.iter() {
                    res.remove(&UtxoState(ts.0, 0, ts.2, ts.3, ts.4));
                }
                res.append(&mut uts.0.clone());
            }
        });
    }

    res.into_iter().take(take).map(Utxo::from).collect()
}

fn process_spent_tx(
    unconfirmed_utxs: &mut BTreeMap<[u8; 32], UnspentTxState>,
    unconfirmed_utxos: &mut BTreeMap<[u8; 21], (UtxoStates, UtxoStates)>,
    utm: &StableBTreeMap<[u8; 32], UnspentTxState, Memory>,
    chain: &ChainParams,
    tx: &transaction::Transaction,
    txid: Txid,
    height: u64,
) -> Result<(), String> {
    for txin in tx.input.iter() {
        let previd: [u8; 32] = *txin.prevout.txid;
        if let std::collections::btree_map::Entry::Vacant(e) = unconfirmed_utxs.entry(previd) {
            if let Some(utx) = utm.get(&previd) {
                // load unspent tx from stable storage
                e.insert(utx);
            }
        }

        if let Some(utx) = unconfirmed_utxs.get_mut(&previd) {
            let txout = utx
                .1
                .get(txin.prevout.vout as usize)
                .ok_or("unexpected vout")?;
            let txout = transaction::TxOut::try_from(txout.as_ref())?;
            let (_, addr) = script::classify_script(txout.script_pubkey.as_bytes(), chain);

            // move spent utxo
            if let Some(addr) = addr {
                match unconfirmed_utxos.get_mut(&addr.0) {
                    Some((uts, sts)) => {
                        uts.0
                            .remove(&UtxoState(utx.0, 0, previd, txin.prevout.vout, txout.value));
                        sts.0.insert(UtxoState(
                            utx.0,
                            height,
                            previd,
                            txin.prevout.vout,
                            txout.value,
                        ));
                    }
                    None => {
                        unconfirmed_utxos.insert(
                            addr.0,
                            (
                                UtxoStates(BTreeSet::new()),
                                UtxoStates(BTreeSet::from([UtxoState(
                                    utx.0,
                                    height,
                                    previd,
                                    txin.prevout.vout,
                                    txout.value,
                                )])),
                            ),
                        );
                    }
                };
            }

            // mark the utxo as spent in the unspent tx
            *utx.2
                .get_mut(txin.prevout.vout as usize)
                .ok_or("unexpected vout")? = Some((height, txid.clone()));
        }
    }

    Ok(())
}

fn add_unspent_txouts(
    unconfirmed_utxs: &mut BTreeMap<[u8; 32], UnspentTxState>,
    unconfirmed_utxos: &mut BTreeMap<[u8; 21], (UtxoStates, UtxoStates)>,
    chain: &ChainParams,
    tx: &transaction::Transaction,
    txid: Txid,
    height: u64,
) -> Result<(), String> {
    unconfirmed_utxs.insert(
        txid.0,
        UnspentTxState(
            height,
            tx.output
                .iter()
                .map(|txout| ByteBuf::from(txout.to_bytes()))
                .collect(),
            vec![None; tx.output.len()],
        ),
    );

    for (vout, txout) in tx.output.iter().enumerate() {
        let (_, addr) = script::classify_script(txout.script_pubkey.as_bytes(), chain);

        if let Some(addr) = addr {
            let utxo = UtxoState(height, 0, txid.0, vout as u32, txout.value);
            match unconfirmed_utxos.get_mut(&addr.0) {
                Some((uts, _sts)) => {
                    uts.0.insert(utxo);
                }
                None => {
                    unconfirmed_utxos.insert(
                        addr.0,
                        (
                            UtxoStates(BTreeSet::from([utxo])),
                            UtxoStates(BTreeSet::new()),
                        ),
                    );
                }
            };
        }
    }

    Ok(())
}

fn flush_confirmed_utxos(
    unconfirmed_utxs: &mut BTreeMap<[u8; 32], UnspentTxState>,
    unconfirmed_utxos: &mut BTreeMap<[u8; 21], (UtxoStates, UtxoStates)>,
    utm: &mut StableBTreeMap<[u8; 32], UnspentTxState, Memory>,
    xom: &mut StableBTreeMap<[u8; 21], UtxoStates, Memory>,
    confirmed_height: u64,
) -> Result<(), String> {
    let confirmed_txids: Vec<[u8; 32]> = unconfirmed_utxs
        .iter()
        .filter_map(|(txid, utx)| {
            // remove the tx if all outputs are spent
            if utx
                .2
                .iter()
                .all(|spent| matches!(spent, Some(v) if v.0 <= confirmed_height))
            {
                utm.remove(txid);
                Some(*txid)
            } else {
                let confirmed_utx = UnspentTxState(
                    utx.0,
                    utx.1.clone(),
                    utx.2
                        .iter()
                        .map(|spent| match spent {
                            Some(v) if v.0 <= confirmed_height => Some(v.to_owned()),
                            _ => None,
                        })
                        .collect(),
                );
                utm.insert(*txid, confirmed_utx);
                None
            }
        })
        .collect();

    for txid in confirmed_txids {
        unconfirmed_utxs.remove(&txid);
    }

    let empty_addrs: Vec<[u8; 21]> = unconfirmed_utxos
        .iter_mut()
        .filter_map(|(addr, (uts, sts))| {
            let mut confirmed_utxos: BTreeSet<UtxoState> = BTreeSet::new();
            uts.0.retain(|ts| {
                if ts.0 <= confirmed_height {
                    confirmed_utxos.insert(ts.clone());
                    false
                } else {
                    true
                }
            });

            let mut confirmed_stxos: BTreeSet<UtxoState> = BTreeSet::new();
            sts.0.retain(|ts| {
                if ts.1 <= confirmed_height {
                    confirmed_stxos.insert(ts.clone());
                    false
                } else {
                    true
                }
            });

            if !confirmed_utxos.is_empty() || !confirmed_stxos.is_empty() {
                match xom.get(addr) {
                    Some(mut uts) => {
                        for mut ts in confirmed_stxos {
                            ts.1 = 0;
                            uts.0.remove(&ts);
                        }
                        uts.0.append(&mut confirmed_utxos);
                        xom.insert(*addr, uts);
                    }
                    None => {
                        if !confirmed_utxos.is_empty() {
                            xom.insert(*addr, UtxoStates(confirmed_utxos));
                        }
                    }
                };
            }

            if uts.0.is_empty() && sts.0.is_empty() {
                Some(*addr)
            } else {
                None
            }
        })
        .collect();

    for addr in empty_addrs {
        unconfirmed_utxos.remove(&addr);
    }

    Ok(())
}
