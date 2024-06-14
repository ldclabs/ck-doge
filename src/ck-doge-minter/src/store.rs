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
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Duration,
};

use crate::{
    chain,
    ecdsa::{account_path, derive_public_key, public_key_with, sign_with, ECDSAPublicKey},
    ledger, to_cbor_bytes, types, user_account, Account, MILLISECONDS,
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

    // (block_index, receiver, amount, fee_rate, retry)
    #[serde(default)]
    pub utxos_retry_burning_queue: VecDeque<(u64, canister::Address, u64, u64, u8)>,
}

impl State {
    pub fn chain_params(&self) -> &'static ChainParams {
        chain_from_key_bits(self.chain)
    }

    pub fn get_chain(&self) -> Result<chain::Chain, String> {
        self.chain_canister
            .map(chain::Chain::new)
            .ok_or("no chain_canister".to_string())
    }

    pub fn get_ledger(&self) -> Result<ledger::Ledger, String> {
        self.ledger_canister
            .map(ledger::Ledger::new)
            .ok_or("no ledger_canister".to_string())
    }

    pub fn get_address(&self, acc: &Account) -> Result<script::Address, String> {
        let pk = self
            .ecdsa_public_key
            .as_ref()
            .ok_or("no ecdsa_public_key")?;
        let pk = derive_public_key(pk, account_path(acc));
        script::p2pkh_address(&pk.public_key, self.chain_params())
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

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtxoState(pub u64, pub canister::ByteN<32>, pub u32, pub u64);

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

// principal -> MintedUtxos
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct MintedUtxos(BTreeMap<UtxoState, (u64, u64)>);

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
                    txid: k.1.into(),
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
        Vec<(UtxoState, Principal)>,
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

    static COLLECTED_UTXOS: RefCell<StableBTreeMap<UtxoState, (Principal, u64, u64), Memory>> = RefCell::new(
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

pub fn get_address(acc: &Account) -> Result<canister::Address, String> {
    state::with(|s| s.get_address(acc).map(|addr| addr.into()))
}

pub async fn mint_ckdoge(caller: Principal) -> Result<u64, String> {
    let ckdoge_acc = Account {
        owner: caller,
        subaccount: None,
    };

    let doge_acc = user_account(&caller);
    let (addr, chain, ledger) = state::with(|s| {
        Ok::<_, String>((s.get_address(&doge_acc)?, s.get_chain()?, s.get_ledger()?))
    })?;
    let utxos = chain.list_utxos(&addr.into()).await?.utxos;
    if utxos.is_empty() {
        return Err("no utxos found".to_string());
    }

    let mut minted_utxos = MINTED_UTXOS.with(|r| r.borrow().get(&caller).unwrap_or_default());
    let utxos = utxos
        .into_iter()
        .filter_map(|tx| {
            let utxo = UtxoState(tx.height, tx.txid.0, tx.vout, tx.value);
            if minted_utxos.0.contains_key(&utxo) {
                None
            } else {
                Some(utxo)
            }
        })
        .collect::<Vec<_>>();
    if utxos.is_empty() {
        return Err("no utxos found".to_string());
    }

    let minted_at = ic_cdk::api::time() / MILLISECONDS;
    let mut total_amount = 0;
    let res: Result<(), String> = async {
        for tx in utxos {
            let memo = to_cbor_bytes(&types::MintMemo {
                txid: tx.1.into(),
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
                Err(format!(
                    "minted {total_amount} ckDOGE, and some UTXOs failed: {err}"
                ))
            } else {
                Err(err)
            }
        }
    }
}

const MIN_RETRIEVE_BATCH_SIZE: usize = 10;
const MAX_RETRIEVE_BATCH_SIZE: usize = 100;

pub async fn burn_ckdoge(
    caller: Principal,
    address: String,
    amount: u64,
    fee_rate: u64,
) -> Result<canister::SendSignedTransactionOutput, String> {
    if amount < DUST_LIMIT * 10 {
        return Err("amount is too small".to_string());
    }

    let ckdoge_acc = Account {
        owner: caller,
        subaccount: None,
    };

    let chain_params = state::with(|s| s.chain_params());
    let ledger = state::with(|s| s.get_ledger())?;
    let receiver = script::Address::from_str(&address)?;
    if !receiver.is_p2pkh(chain_params) {
        return Err("invalid p2pkh address".to_string());
    }

    let balance = ledger.balance_of(ckdoge_acc).await?;
    if amount > balance {
        return Err(format!(
            "insufficient ckDOGE balance, expected: {amount}, got {balance}"
        ));
    }

    let (utxos, total) = COLLECTED_UTXOS.with(|r| {
        let m = r.borrow();
        let mut total: u64 = 0;
        let mut utxos: Vec<(UtxoState, Principal)> = vec![];
        for (utxo, v) in m.iter() {
            if v.1 == 0 {
                total += utxo.3;
                utxos.push((utxo, v.0));
                if (utxos.len() >= MIN_RETRIEVE_BATCH_SIZE && total >= amount)
                    || utxos.len() >= MAX_RETRIEVE_BATCH_SIZE
                {
                    break;
                }
            }
        }

        (utxos, total)
    });

    if total < amount {
        let size = utxos.len();
        return Err(format!(
            "The latest batch of {size} UTXOs has a total balance of {total}. This withdrawal cannot exceed the limit."
        ));
    }

    let memo = to_cbor_bytes(&types::BurnMemo {
        address: receiver.clone().into(),
    });
    let blk = ledger
        .burn(amount, ckdoge_acc, Memo(ByteBuf::from(memo)))
        .await?;

    state::with_mut(|s| {
        s.tokens_burned = s.tokens_burned.saturating_add(amount);
    });

    COLLECTED_UTXOS.with(|r| {
        let mut m = r.borrow_mut();
        // mark utxos as used
        for utxo in utxos.iter() {
            m.insert(utxo.0.clone(), (utxo.1, blk, 0));
        }
    });

    match burn_utxos(blk, receiver.clone(), amount, fee_rate, utxos).await {
        Ok(res) => Ok(res),
        Err(err) => {
            state::with_mut(|s| {
                s.utxos_retry_burning_queue
                    .push_back((blk, receiver.into(), amount, fee_rate, 0));
            });

            // retry burn utxos after 30 seconds
            ic_cdk_timers::set_timer(
                Duration::from_secs(30),
                || ic_cdk::spawn(retry_burn_utxos()),
            );
            Err(err)
        }
    }
}

// we can retry burn utxos if it failed in the previous burn_ckdoge call
pub async fn retry_burn_utxos() {
    if let Some((blk, receiver, amount, fee_rate, retry)) =
        state::with_mut(|s| s.utxos_retry_burning_queue.pop_front())
    {
        ic_cdk::print(format!(
            "retry burn utxos after 30 seconds, block index: {blk}, receiver: {receiver:?}, amount: {amount}"
        ));

        let utxos = COLLECTED_UTXOS.with(|r| {
            let m = r.borrow();
            let mut utxos: Vec<(UtxoState, Principal)> = vec![];
            for (utxo, v) in m.iter() {
                if v.1 == blk && v.2 == 0 {
                    utxos.push((utxo, v.0));
                }
            }
            utxos
        });

        if burn_utxos(blk, receiver.clone().into(), amount, fee_rate, utxos)
            .await
            .is_err()
            && retry < 3
        {
            state::with_mut(|s| {
                s.utxos_retry_burning_queue
                    .push_back((blk, receiver, amount, fee_rate, retry + 1));
            });
        }

        // retry burn utxos after 30 seconds
        ic_cdk_timers::set_timer(
            Duration::from_secs(30),
            || ic_cdk::spawn(retry_burn_utxos()),
        );
    }
}

pub async fn collect_and_clear_utxos() -> Result<u64, String> {
    let minter = ic_cdk::id();
    let acc = user_account(&minter);
    let (addr, chain) = state::with(|s| Ok::<_, String>((s.get_address(&acc)?, s.get_chain()?)))?;

    let res = chain.list_utxos(&addr.into()).await?;
    if res.utxos.is_empty() {
        return Ok(0);
    }

    let confirmed_height = res.confirmed_height;
    COLLECTED_UTXOS.with(|r| {
        let mut m = r.borrow_mut();
        let mut total: u64 = 0;
        for utxo in res.utxos {
            let utxo = UtxoState(utxo.height, utxo.txid.0, utxo.vout, utxo.value);
            if !m.contains_key(&utxo) {
                total += utxo.3;
                m.insert(utxo, (minter, 0, 0));
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

async fn burn_utxos(
    block_index: u64,
    receiver: script::Address,
    amount: u64,
    fee_rate: u64,
    utxos: Vec<(UtxoState, Principal)>,
) -> Result<canister::SendSignedTransactionOutput, String> {
    let (chain_params, key_name, ecdsa_public_key) = state::with(|s| {
        (
            s.chain_params(),
            s.ecdsa_key_name.clone(),
            s.ecdsa_public_key.clone(),
        )
    });
    let ecdsa_public_key = ecdsa_public_key.ok_or("no ecdsa_public_key")?;
    let chain = state::with(|s| s.get_chain())?;
    let mut kc = KeysCache::new(&ecdsa_public_key, chain_params);
    let minter = kc.get_or_set(ic_cdk::id())?;

    let total: u64 = utxos.iter().map(|u| u.0 .3).sum();
    let mut send_tx = Transaction {
        version: Transaction::CURRENT_VERSION,
        lock_time: 0,
        input: utxos
            .iter()
            .map(|tx| {
                TxIn::with_outpoint(OutPoint {
                    txid: canister::Txid::from(tx.0 .1).into(),
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
                script_pubkey: minter.script_pubkey.clone(),
            },
        ],
    };

    let fee = fee_by_size(send_tx.estimate_size() as u64, fee_rate);
    send_tx.output[0].value = amount.saturating_sub(fee);
    send_tx.output[1].value = total.saturating_sub(amount);
    if send_tx.output[1].value <= DUST_LIMIT {
        send_tx.output.pop();
    }

    let mut sighasher = SighashCache::new(&mut send_tx);
    for (i, utxo) in utxos.iter().enumerate() {
        let acc = kc.get_or_set(utxo.1)?;
        let hash = sighasher.signature_hash(i, &acc.script_pubkey, EcdsaSighashType::All)?;
        let sig = sign_with(&key_name, acc.key_path.clone(), *hash).await?;
        let signature = Signature::from_compact(&sig).map_err(err_string)?;
        sighasher
            .set_input_script(
                i,
                &SighashSignature {
                    signature,
                    sighash_type: EcdsaSighashType::All,
                },
                &acc.public_key,
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
            m.insert(utxo.0.clone(), (utxo.1, block_index, res.tip_height));
        }
    });

    let burned_at = ic_cdk::api::time() / MILLISECONDS;
    BURNED_UTXOS.with(|r| {
        r.borrow_mut().insert(
            block_index,
            BurnedUtxo((utxos, receiver.into(), res.txid.clone(), burned_at)),
        );
    });

    Ok(res)
}

struct KeyInfo {
    key_path: Vec<Vec<u8>>,
    public_key: PublicKey,
    script_pubkey: script::ScriptBuf,
}

struct KeysCache<'a> {
    chain_params: &'a ChainParams,
    ecdsa_public_key: &'a ECDSAPublicKey,
    keys: BTreeMap<Principal, KeyInfo>,
}

impl<'a> KeysCache<'a> {
    fn new(ecdsa_public_key: &'a ECDSAPublicKey, chain_params: &'a ChainParams) -> Self {
        Self {
            chain_params,
            ecdsa_public_key,
            keys: BTreeMap::new(),
        }
    }

    fn get_or_set(&mut self, caller: Principal) -> Result<&KeyInfo, String> {
        let ok = self.keys.contains_key(&caller);
        if !ok {
            let account = user_account(&caller);
            let key_path = account_path(&account);
            let pk = derive_public_key(self.ecdsa_public_key, key_path.clone());
            let address = script::p2pkh_address(&pk.public_key, self.chain_params)?;
            let public_key = PublicKey::from_slice(&pk.public_key).map_err(err_string)?;
            let script_pubkey = address.to_script(self.chain_params);
            let info = KeyInfo {
                key_path,
                public_key,
                script_pubkey,
            };
            self.keys.insert(caller, info);
        }

        Ok(self.keys.get(&caller).unwrap())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use hex::DisplayHex;

    #[test]
    fn test_bound_max_size() {
        let v = UtxoState(
            u64::MAX,
            canister::ByteN::from([255u8; 32]),
            u32::MAX,
            u64::MAX,
        );
        let v = v.to_bytes();
        println!(
            "UtxoState max_size: {}, {}",
            v.len(),
            v.to_lower_hex_string()
        );
        // UtxoState max_size: 58, 841bffffffffffffffff5820ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1affffffff1bffffffffffffffff

        let v = UtxoState(0, canister::ByteN::from([0u8; 32]), 0, 0);
        let v = v.to_bytes();
        println!(
            "UtxoState min_size: {}, {}",
            v.len(),
            v.to_lower_hex_string()
        );
        // UtxoState min_size: 38, 8400582000000000000000000000000000000000000000000000000000000000000000000000
    }
}
