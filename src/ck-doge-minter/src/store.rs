use candid::Principal;
use ciborium::{from_reader, into_writer};
use ck_doge_types::{
    amount::{fee_by_size, DUST_LIMIT},
    canister,
    chainparams::{chain_from_key_bits, ChainParams, KeyBits},
    err_string, script,
    sighash::*,
    transaction::{OutPoint, Transaction, TxIn, TxOut},
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use icrc_ledger_types::icrc1::transfer::Memo;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::str::FromStr;
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use crate::{
    chain,
    ecdsa::{account_path, derive_public_key, public_key_with, sign_with, ECDSAPublicKey},
    ledger, minter_account, to_cbor_bytes, types, user_account, Account, MILLISECONDS,
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

    /// The total amount of ckDOGE minted.
    pub tokens_minted: u64,

    /// The total amount of ckDOGE burned.
    pub tokens_burned: u64,

    /// The CanisterId of the ckDOGE Ledger.
    pub ledger_canister: Option<Principal>,

    pub chain_canister: Option<Principal>,

    pub managers: BTreeSet<Principal>,
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
pub struct BurnedUtxo(
    (
        Vec<(Utxo, Principal)>,
        canister::Address,
        canister::Txid,
        u64,
    ),
);

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

    static COLLECTED_UTXOS: RefCell<StableBTreeMap<Utxo, (Principal, u64, u64), Memory>> = RefCell::new(
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

pub mod state {
    use super::*;

    pub fn is_manager(caller: &Principal) -> bool {
        STATE_HEAP.with(|r| r.borrow().managers.contains(caller))
    }

    pub fn get_chain() -> Result<chain::Chain, String> {
        STATE_HEAP.with(|r| {
            r.borrow()
                .chain_canister
                .map(chain::Chain::new)
                .ok_or("no chain_canister".to_string())
        })
    }

    pub fn get_ledger() -> Result<ledger::Ledger, String> {
        STATE_HEAP.with(|r| {
            r.borrow()
                .ledger_canister
                .map(ledger::Ledger::new)
                .ok_or("no ledger_canister".to_string())
        })
    }

    pub fn get_accounts_len() -> u64 {
        MINTED_UTXOS.with(|r| r.borrow().len())
    }

    pub fn get_collected_utxos_len() -> u64 {
        COLLECTED_UTXOS.with(|r| r.borrow().len())
    }

    pub fn get_burned_utxos_len() -> u64 {
        BURNED_UTXOS.with(|r| r.borrow().len())
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
                r.ecdsa_public_key = Some(ecdsa_public_key);
            });
        }
    }

    pub fn load() {
        STATE.with(|r| {
            let s = r.borrow_mut().get().clone();
            STATE_HEAP.with(|h| {
                *h.borrow_mut() = s;
                // let mut s = h.borrow_mut();
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

pub fn get_address(acc: &Account) -> Result<canister::Address, String> {
    state::with(|s| {
        let pk = s.ecdsa_public_key.as_ref().ok_or("no ecdsa_public_key")?;
        let pk = derive_public_key(pk, account_path(acc));
        script::p2pkh_address(&pk.public_key, s.chain_params()).map(|addr| addr.into())
    })
}

pub async fn mint_ckdoge(caller: Principal) -> Result<u64, String> {
    let ckdoge_acc = Account {
        owner: caller,
        subaccount: None,
    };

    let doge_acc = user_account(&caller);
    let addr = get_address(&doge_acc)?;
    let chain = state::get_chain()?;
    let ledger = state::get_ledger()?;
    let utxos = chain.list_utxos(&addr).await?.utxos;
    if utxos.is_empty() {
        return Err("no utxos found".to_string());
    }

    let mut minted_utxos = MINTED_UTXOS.with(|r| r.borrow().get(&caller).unwrap_or_default());
    let utxos = utxos
        .into_iter()
        .filter_map(|tx| {
            let utxo = Utxo(tx.height, tx.txid.0, tx.vout, tx.value);
            if minted_utxos.0.contains_key(&utxo) {
                None
            } else {
                Some(utxo)
            }
        })
        .collect::<Vec<_>>();

    let minted_at = ic_cdk::api::time() / MILLISECONDS;
    let mut total_amount = 0;
    let res: Result<(), String> = async {
        for tx in utxos {
            let memo = to_cbor_bytes(&types::MintMemo {
                txid: canister::Txid(tx.1),
                vout: tx.2,
            });
            let blk = ledger
                .mint(tx.3, ckdoge_acc, Memo(ByteBuf::from(memo)))
                .await?;

            // save every minted utxo
            total_amount += tx.3;
            state::with_mut(|s| {
                s.tokens_minted = s.tokens_minted.saturating_add(tx.3);
            });
            minted_utxos.0.insert(tx.clone(), (blk, minted_at));
            MINTED_UTXOS.with(|r| {
                r.borrow_mut().insert(caller, minted_utxos.clone());
            });
            COLLECTED_UTXOS.with(|r| {
                r.borrow_mut().insert(tx, (caller, 0, 0));
            });
        }
        Ok(())
    }
    .await;

    match res {
        Ok(_) => Ok(total_amount),
        Err(err) => {
            if total_amount > 0 {
                Err(format!("minted {total_amount} ckDOGE, error: {err}"))
            } else {
                Err(err)
            }
        }
    }
}

const RETRIEVE_BATCH_SIZE: usize = 100;

pub async fn burn_ckdoge(
    caller: Principal,
    address: String,
    amount: u64,
) -> Result<canister::SendSignedTransactionOutput, String> {
    if amount < DUST_LIMIT * 10 {
        return Err("amount is too small".to_string());
    }

    let ckdoge_acc = Account {
        owner: caller,
        subaccount: None,
    };

    let (chain_params, key_name, ecdsa_public_key) = state::with(|s| {
        (
            s.chain_params(),
            s.ecdsa_key_name.clone(),
            s.ecdsa_public_key.clone(),
        )
    });
    let ecdsa_public_key = ecdsa_public_key.ok_or("no ecdsa_public_key")?;

    let chain = state::get_chain()?;
    let ledger = state::get_ledger()?;
    let receiver = script::Address::from_str(&address)?;
    if !receiver.is_p2pkh(chain_params) {
        return Err("invalid p2pkh address".to_string());
    }
    let minter: script::Address = get_address(&minter_account())?.into();

    let balance = ledger.balance_of(ckdoge_acc).await?;
    if amount > balance {
        return Err(format!(
            "insufficient ckDOGE balance, expected: {amount}, got {balance}"
        ));
    }

    let (utxos, total) = COLLECTED_UTXOS.with(|r| {
        let m = r.borrow();
        let mut total: u64 = 0;
        let mut utxos: Vec<(Utxo, Principal)> = vec![];
        for (utxo, v) in m.iter() {
            if v.1 == 0 {
                total += utxo.3;
                utxos.push((utxo, v.0));
                if utxos.len() >= RETRIEVE_BATCH_SIZE {
                    break;
                }
            }
        }

        (utxos, total)
    });

    if total < amount {
        let size = utxos.len();
        return Err(format!(
            "The latest batch of UTXOs ({size}) has a total balance of {total}. This withdrawal cannot exceed the limit."
        ));
    }

    let memo = to_cbor_bytes(&types::BurnMemo {
        address: receiver.clone().into(),
    });
    let burned_at = ic_cdk::api::time() / MILLISECONDS;
    let blk = ledger
        .burn(amount, ckdoge_acc, Memo(ByteBuf::from(memo)))
        .await?;

    COLLECTED_UTXOS.with(|r| {
        let mut m = r.borrow_mut();
        // mark utxos as used
        for utxo in utxos.iter() {
            m.insert(utxo.0.clone(), (utxo.1, blk, 0));
        }
    });

    let mut send_tx = Transaction {
        version: Transaction::CURRENT_VERSION,
        lock_time: 0,
        input: utxos
            .iter()
            .map(|tx| {
                TxIn::with_outpoint(OutPoint {
                    txid: canister::Txid(tx.0 .1).into(),
                    vout: tx.0 .2,
                })
            })
            .collect(),
        output: vec![
            TxOut {
                value: amount,
                script_pubkey: receiver.to_script(chain_params),
            },
            TxOut {
                value: total.saturating_sub(amount),
                script_pubkey: minter.to_script(chain_params),
            },
        ],
    };

    let fee = fee_by_size(send_tx.estimate_size());
    send_tx.output[0].value = amount.saturating_sub(fee);
    send_tx.output[1].value = total.saturating_sub(amount);
    if send_tx.output[1].value <= DUST_LIMIT {
        send_tx.output.pop();
    }

    let mut sighasher = SighashCache::new(&mut send_tx);
    for (i, utxo) in utxos.iter().enumerate() {
        let key_path = account_path(&user_account(&utxo.1));
        let pk = derive_public_key(&ecdsa_public_key, key_path.clone());
        let addr = script::p2pkh_address(&pk.public_key, chain_params)?;
        let hash =
            sighasher.signature_hash(i, &addr.to_script(chain_params), EcdsaSighashType::All)?;
        let sig = sign_with(&key_name, key_path, *hash).await?;
        let signature = Signature::from_compact(&sig).map_err(err_string)?;
        sighasher
            .set_input_script(
                i,
                &SighashSignature {
                    signature,
                    sighash_type: EcdsaSighashType::All,
                },
                &PublicKey::from_slice(&pk.public_key).map_err(err_string)?,
            )
            .map_err(err_string)?;
    }

    let res = chain
        .send_signed_transaction(sighasher.transaction())
        .await?;

    COLLECTED_UTXOS.with(|r| {
        let mut m = r.borrow_mut();
        // mark utxos as burned
        for utxo in utxos.iter() {
            m.insert(utxo.0.clone(), (utxo.1, blk, res.tip_height));
        }
    });

    BURNED_UTXOS.with(|r| {
        r.borrow_mut().insert(
            blk,
            BurnedUtxo((utxos, receiver.into(), res.txid.clone(), burned_at)),
        );
    });

    Ok(res)
}

pub async fn collect_and_clear_utxos() -> Result<u64, String> {
    let acc = minter_account();
    let addr = get_address(&acc)?;

    let chain = state::get_chain()?;
    let res = chain.list_utxos(&addr).await?;
    if res.utxos.is_empty() {
        return Ok(0);
    }

    let confirmed_height = res.confirmed_height;
    COLLECTED_UTXOS.with(|r| {
        let mut m = r.borrow_mut();
        let mut total: u64 = 0;
        for utxo in res.utxos {
            let utxo = Utxo(utxo.height, utxo.txid.0, utxo.vout, utxo.value);
            if !m.contains_key(&utxo) {
                total += utxo.3;
                m.insert(utxo, (acc.owner, 0, 0));
            }
        }

        let mut remove_utxos = vec![];
        for (utxo, v) in m.iter() {
            if v.2 > 0 && v.2 < confirmed_height {
                remove_utxos.push(utxo);
            }
        }

        // remove retrieved utxos in previous burned
        for utxo in remove_utxos {
            m.remove(&utxo);
        }

        Ok(total)
    })
}

pub fn list_minted_utxos(caller: Principal) -> Vec<types::MintedUtxo> {
    MINTED_UTXOS.with(|r| {
        r.borrow()
            .get(&caller)
            .map_or_else(Vec::new, |utxos| utxos.into())
    })
}
