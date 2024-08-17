use candid::Principal;
use ciborium::{from_reader, into_writer};
use dogecoin::{
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
use serde_bytes::{ByteArray, ByteBuf};
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

    /// The canister ECDSA public key
    pub ecdsa_public_key: Option<ECDSAPublicKey>,

    pub rpc_proxy_public_key: String,

    /// The minimum number of confirmations on the Dogecoin chain.
    pub min_confirmations: u32,

    pub tip_height: u64,
    pub tip_blockhash: ByteArray<32>,

    pub processed_height: u64,
    pub processed_blockhash: ByteArray<32>,

    pub confirmed_height: u64,
    pub confirmed_blockhash: ByteArray<32>,

    pub start_height: u64,
    pub start_blockhash: ByteArray<32>,

    pub last_errors: VecDeque<String>,

    pub managers: BTreeSet<Principal>,
    pub rpc_agents: Vec<RPCAgent>,

    pub unconfirmed_utxs: BTreeMap<ByteArray<32>, UnspentTxState>,
    pub unconfirmed_utxos: BTreeMap<ByteArray<21>, (UtxoStates, SpentUtxos)>,
    processed_blocks: VecDeque<(u64, ByteArray<32>)>,
}

impl State {
    pub fn chain_params(&self) -> &'static ChainParams {
        chain_from_key_bits(self.chain)
    }

    pub fn append_error(&mut self, err: String) {
        self.last_errors.push_back(err);
        if self.last_errors.len() > 7 {
            self.last_errors.pop_front();
        }
    }
}

impl Storable for State {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode State data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode State data")
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
        into_writer(self, &mut buf).expect("failed to encode UnspentTxState data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode UnspentTxState data")
    }
}

// address -> UnspentOutput
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtxoState(pub u64, pub ByteArray<32>, pub u32, pub u64);
impl Storable for UtxoState {
    const BOUND: Bound = Bound::Bounded {
        max_size: 58,
        is_fixed_size: false,
    };

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode UtxoState data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode UtxoState data")
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct UtxoStates(BTreeSet<UtxoState>);

// UtxoState -> spent height
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct SpentUtxos(BTreeMap<UtxoState, u64>);

impl From<UtxoState> for Utxo {
    fn from(uts: UtxoState) -> Self {
        Utxo {
            height: uts.0,
            txid: uts.1.into(),
            vout: uts.2,
            value: uts.3,
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
        into_writer(self, &mut buf).expect("failed to encode UtxoStates data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode UtxoStates data")
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const UT_MEMORY_ID: MemoryId = MemoryId::new(1);
const XO_MEMORY_ID: MemoryId = MemoryId::new(2);

#[derive(Default)]
pub struct SyncingState {
    pub status: i8, // 0: not running, > 0: running, < 0: stop because of error
    pub timer: Option<ic_cdk_timers::TimerId>,
    pub refresh_proxy_token_timer: Option<ic_cdk_timers::TimerId>,
}

thread_local! {
    static STATE_HEAP: RefCell<State> = RefCell::new(State::default());

    static SYNCING_STATE: RefCell<SyncingState> = RefCell::new(SyncingState::default());

    static UNPROCESSED_BLOCKS: RefCell<VecDeque<(u64, BlockHash, Block)>> =const { RefCell::new(VecDeque::new()) };

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            State::default()
        ).expect("failed to init STATE store")
    );

    // txid -> unspent tx
    static UTXS: RefCell<StableBTreeMap<[u8; 32], UnspentTxState, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(UT_MEMORY_ID)),
        )
    );

    // address -> unspent output
    static UTXOS: RefCell<StableBTreeMap<[u8; 21], UtxoStates, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(XO_MEMORY_ID)),
        )
    );
}

pub mod syncing {
    use super::*;

    pub fn with<R>(f: impl FnOnce(&SyncingState) -> R) -> R {
        SYNCING_STATE.with(|r| f(&r.borrow()))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut SyncingState) -> R) -> R {
        SYNCING_STATE.with(|r| f(&mut r.borrow_mut()))
    }
}

pub mod state {
    use super::*;

    pub fn is_manager(caller: &Principal) -> bool {
        STATE_HEAP.with(|r| r.borrow().managers.contains(caller))
    }

    pub fn get_agent() -> RPCAgent {
        STATE_HEAP.with(|r| r.borrow().rpc_agents.first().expect("no RPCAgent").clone())
    }

    pub fn get_attest_agents() -> Vec<RPCAgent> {
        STATE_HEAP.with(|r| {
            r.borrow()
                .rpc_agents
                .split_first()
                .map(|(_, v)| v.to_vec())
                .unwrap_or_default()
        })
    }

    pub fn get_unprocessed_blocks_len() -> u64 {
        UNPROCESSED_BLOCKS.with(|r| r.borrow().len() as u64)
    }

    pub fn get_confirmed_utxs_len() -> u64 {
        UTXS.with(|r| r.borrow().len())
    }

    pub fn get_confirmed_utxos_len() -> u64 {
        UTXOS.with(|r| r.borrow().len())
    }

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE_HEAP.with(|r| f(&r.borrow()))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE_HEAP.with(|r| f(&mut r.borrow_mut()))
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
                if s.processed_height > 0 {
                    s.tip_height = s.processed_height;
                    s.tip_blockhash = s.processed_blockhash;
                }
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

pub fn get_public_key(derivation_path: Vec<Vec<u8>>) -> Result<ECDSAPublicKey, String> {
    state::with(|s| {
        let pk = s.ecdsa_public_key.as_ref().ok_or("no ecdsa_public_key")?;
        Ok(derive_public_key(pk, derivation_path))
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
        if *s.tip_blockhash != *block.header.prev_blockhash {
            return Err(format!(
                "invalid prev_blockhash at {}, expected {:?}, got {:?}",
                height,
                BlockHash::from(*s.tip_blockhash),
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
        s.tip_blockhash = (*hash).into();
        Ok(())
    })
}

// we should clear the unprocessed blocks because the block may be invalid to process
pub fn clear_for_restart_process_block() {
    UNPROCESSED_BLOCKS.with(|r| r.borrow_mut().clear());
    state::with_mut(|s| {
        // reset the tip to the last processed block
        if s.processed_height > 0 {
            s.tip_height = s.processed_height;
            s.tip_blockhash = s.processed_blockhash;
        }
    });
}

// return true if a block processed
pub fn process_block() -> Result<bool, String> {
    state::with_mut(|s| {
        UNPROCESSED_BLOCKS.with(|r| {
            match r.borrow_mut().pop_front() {
                None => Ok(false),
                Some((height, hash, block)) => {
                    if s.processed_height != 0 && height != s.processed_height + 1 {
                        return Err(format!(
                            "invalid block height to process, expected {}, got {}",
                            s.tip_height, height
                        ));
                    }

                    let chain = chain_from_key_bits(s.chain);
                    UTXS.with(|utr| {
                        let utm = utr.borrow();

                        for tx in block.txdata.iter().skip(1) {
                            let txid = Txid::from(tx.compute_txid());

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

                    s.processed_height = height;
                    s.processed_blockhash = (*hash).into();
                    s.processed_blocks.push_back((height, (*hash).into()));
                    if s.start_height == 0 && *s.start_blockhash == [0u8; 32] {
                        s.start_height = s.processed_height;
                        s.start_blockhash = s.processed_blockhash;
                    }
                    Ok(true)
                }
            }
        })
    })
}

// we should clear all unconfirmed states.
pub fn clear_for_restart_confirm_utxos() {
    UNPROCESSED_BLOCKS.with(|r| r.borrow_mut().clear());
    state::with_mut(|s| {
        s.unconfirmed_utxs.clear();
        s.unconfirmed_utxos.clear();
        s.processed_blocks.clear();
        s.processed_height = s.confirmed_height;
        s.processed_blockhash = s.confirmed_blockhash;
        // reset the tip to the last processed block
        if s.processed_height > 0 {
            s.tip_height = s.processed_height;
            s.tip_blockhash = s.processed_blockhash;
        }
    });
}

// return true if there are more blocks wait to process
pub fn confirm_utxos() -> Result<bool, String> {
    state::with_mut(|s| {
        let confirmed_height = s
            .processed_height
            .saturating_sub(s.min_confirmations as u64);
        if confirmed_height < s.start_height || confirmed_height <= s.confirmed_height {
            return Ok(UNPROCESSED_BLOCKS.with(|r| !r.borrow().is_empty()));
        }

        let mut confirmed_blockhash = None;
        while let Some((height, hash)) = s.processed_blocks.pop_front() {
            if height == confirmed_height {
                confirmed_blockhash = Some(hash);
                break;
            }
        }

        let confirmed_blockhash = confirmed_blockhash
            .ok_or_else(|| format!("no processed blockhash at height {}", confirmed_height))?;

        UTXS.with(|utr| {
            UTXOS.with(|xor| {
                flush_confirmed_utxos(
                    &mut s.unconfirmed_utxs,
                    &mut s.unconfirmed_utxos,
                    &mut utr.borrow_mut(),
                    &mut xor.borrow_mut(),
                    confirmed_height,
                )?;
                s.confirmed_height = confirmed_height;
                s.confirmed_blockhash = confirmed_blockhash;
                Ok(UNPROCESSED_BLOCKS.with(|r| !r.borrow().is_empty()))
            })
        })
    })
}

pub fn get_utx(txid: &ByteArray<32>) -> Option<UnspentTx> {
    state::with(|s| match s.unconfirmed_utxs.get(txid) {
        Some(utx) => Some(UnspentTx::from(utx.clone())),
        None => UTXS.with(|r| r.borrow().get(txid).map(UnspentTx::from)),
    })
}

pub fn get_tx_block_height(txid: &ByteArray<32>) -> Option<u64> {
    state::with(|s| match s.unconfirmed_utxs.get(txid) {
        Some(utx) => Some(utx.0),
        None => UTXS.with(|r| r.borrow().get(txid).map(|utx| utx.0)),
    })
}

pub fn get_balance(addr: &ByteArray<21>) -> u64 {
    let mut res = UTXOS.with(|r| r.borrow().get(addr).unwrap_or_default()).0;
    state::with(|s| {
        if let Some((uts, sts)) = s.unconfirmed_utxos.get(addr) {
            for tx in sts.0.keys() {
                res.remove(tx);
            }
            res.append(&mut uts.0.clone());
        }
    });
    res.into_iter().map(|v| v.3).sum()
}

pub fn list_utxos(addr: &ByteArray<21>, take: usize, confirmed: bool) -> Vec<Utxo> {
    let mut res = UTXOS.with(|r| r.borrow().get(addr).unwrap_or_default()).0;
    if !confirmed {
        state::with(|s| {
            if let Some((uts, sts)) = s.unconfirmed_utxos.get(addr) {
                for tx in sts.0.keys() {
                    res.remove(tx);
                }
                res.append(&mut uts.0.clone());
            }
        });
    }

    res.into_iter().take(take).map(Utxo::from).collect()
}

fn process_spent_tx(
    unconfirmed_utxs: &mut BTreeMap<ByteArray<32>, UnspentTxState>,
    unconfirmed_utxos: &mut BTreeMap<ByteArray<21>, (UtxoStates, SpentUtxos)>,
    utm: &StableBTreeMap<[u8; 32], UnspentTxState, Memory>,
    chain: &ChainParams,
    tx: &transaction::Transaction,
    txid: Txid,
    height: u64,
) -> Result<(), String> {
    for txin in tx.input.iter() {
        let previd: ByteArray<32> = (*txin.prevout.txid).into();
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
                let addr: ByteArray<21> = addr.0.into();
                let utxo = UtxoState(utx.0, previd, txin.prevout.vout, txout.value);
                match unconfirmed_utxos.get_mut(&addr) {
                    Some((uts, sts)) => {
                        uts.0.remove(&utxo);
                        sts.0.insert(utxo, height);
                    }
                    None => {
                        unconfirmed_utxos.insert(
                            addr,
                            (
                                UtxoStates(BTreeSet::new()),
                                SpentUtxos(BTreeMap::from([(utxo, height)])),
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
    unconfirmed_utxs: &mut BTreeMap<ByteArray<32>, UnspentTxState>,
    unconfirmed_utxos: &mut BTreeMap<ByteArray<21>, (UtxoStates, SpentUtxos)>,
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
            let addr: ByteArray<21> = addr.0.into();
            let utxo = UtxoState(height, txid.0, vout as u32, txout.value);
            match unconfirmed_utxos.get_mut(&addr) {
                Some((uts, _sts)) => {
                    uts.0.insert(utxo);
                }
                None => {
                    unconfirmed_utxos.insert(
                        addr,
                        (
                            UtxoStates(BTreeSet::from([utxo])),
                            SpentUtxos(BTreeMap::new()),
                        ),
                    );
                }
            };
        }
    }

    Ok(())
}

fn flush_confirmed_utxos(
    unconfirmed_utxs: &mut BTreeMap<ByteArray<32>, UnspentTxState>,
    unconfirmed_utxos: &mut BTreeMap<ByteArray<21>, (UtxoStates, SpentUtxos)>,
    utm: &mut StableBTreeMap<[u8; 32], UnspentTxState, Memory>,
    xom: &mut StableBTreeMap<[u8; 21], UtxoStates, Memory>,
    confirmed_height: u64,
) -> Result<(), String> {
    let confirmed_txids: Vec<ByteArray<32>> = unconfirmed_utxs
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
                utm.insert(**txid, confirmed_utx);
                None
            }
        })
        .collect();

    for txid in confirmed_txids {
        unconfirmed_utxs.remove(&txid);
    }

    let empty_addrs: Vec<ByteArray<21>> = unconfirmed_utxos
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
            sts.0.retain(|tx, height| {
                if *height <= confirmed_height {
                    confirmed_stxos.insert(tx.clone());
                    false
                } else {
                    true
                }
            });

            if !confirmed_utxos.is_empty() || !confirmed_stxos.is_empty() {
                match xom.get(addr) {
                    Some(mut uts) => {
                        for ts in confirmed_stxos {
                            uts.0.remove(&ts);
                        }
                        uts.0.append(&mut confirmed_utxos);
                        xom.insert(**addr, uts);
                    }
                    None => {
                        if !confirmed_utxos.is_empty() {
                            xom.insert(**addr, UtxoStates(confirmed_utxos));
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
