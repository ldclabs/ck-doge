use candid::Principal;
use ciborium::{from_reader, into_writer};
use ck_doge_types::{canister, chainparams::KeyBits};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::RefCell, collections::BTreeMap};

use crate::types;

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct State {
    /// The Dogecoin network that the minter will connect to
    pub chain: KeyBits,

    /// The name of the [EcdsaKeyId]. Use "dfx_test_key" for local replica and "test_key_1" for
    /// a testing key for testnet and mainnet
    pub ecdsa_key_name: String,

    /// The total amount of ckDOGE minted.
    pub tokens_minted: u64,

    /// The total amount of ckDOGE burned.
    pub tokens_burned: u64,

    /// The CanisterId of the ckDOGE Ledger.
    pub ledger_id: Option<Principal>,
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

#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Utxo(pub u64, pub [u8; 32], pub u32, pub u64);

impl Storable for Utxo {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode Utxo data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode Utxo data")
    }
}

// principal -> MintedUtxos
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct MintedUtxos(BTreeMap<Utxo, (u64, u64)>);

impl From<MintedUtxos> for Vec<types::MintedUtxo> {
    fn from(utxos: MintedUtxos) -> Self {
        utxos
            .0
            .into_iter()
            .map(|(k, v)| types::MintedUtxo {
                block_index: v.0,
                minted_at: v.1,
                utxo: canister::Utxo {
                    height: k.0,
                    txid: canister::Txid(k.1),
                    vout: k.2,
                    value: k.3,
                },
            })
            .collect()
    }
}

impl Storable for MintedUtxos {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode MintedUtxos data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode MintedUtxos data")
    }
}

// block_index -> BurnedUtxo
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct BurnedUtxo((Utxo, u64, canister::Address, u64, canister::Txid));

impl Storable for BurnedUtxo {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode BurnedUtxo data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode BurnedUtxo data")
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const MINTED_UTXOS_MEMORY_ID: MemoryId = MemoryId::new(1);
const COLLECTED_UTXOS_MEMORY_ID: MemoryId = MemoryId::new(2);
const BURNED_UTXOS_MEMORY_ID: MemoryId = MemoryId::new(3);

thread_local! {
    static STATE_HEAP: RefCell<State> = RefCell::new(State::default());

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            State::default()
        ).expect("failed to init STATE store")
    );

    // address -> unspent output
    static MINTED_UTXOS: RefCell<StableBTreeMap<Principal, MintedUtxos, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(MINTED_UTXOS_MEMORY_ID)),
        )
    );

    static COLLECTED_UTXOS: RefCell<StableBTreeMap<Utxo, (Principal, u64), Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(COLLECTED_UTXOS_MEMORY_ID)),
        )
    );

    static BURNED_UTXOS: RefCell<StableBTreeMap<u64, BurnedUtxo, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(BURNED_UTXOS_MEMORY_ID)),
        )
    );
}
