use candid::{CandidType, Principal};
use dogecoin::canister;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    is_authenticated, is_controller_or_manager, minter_account, store, types, user_account,
};

#[ic_cdk::query]
fn api_version() -> u16 {
    1
}

#[derive(CandidType, Default)]
pub struct State {
    pub chain: String,

    pub tokens_minted: u64,
    pub tokens_burned: u64,
    pub tokens_minted_count: u64,
    pub tokens_burned_count: u64,
    pub accounts: u64,
    pub collected_utxos: u64,
    pub burned_utxos: u64,
    pub ledger_canister: Option<Principal>,
    pub chain_canister: Option<Principal>,
    pub managers: BTreeSet<Principal>,
    // manager info
    pub ecdsa_key_name: Option<String>,
    pub burning_utxos: BTreeMap<u64, (Principal, canister::Address, u64, u64, String)>,
    pub minter_address: Option<String>,
    pub minter_subaddress: Option<String>,
}

#[ic_cdk::query]
fn get_state() -> Result<State, ()> {
    store::state::with(|s| {
        let mut res = State {
            chain: s.chain_params().chain_name.to_string(),
            tokens_minted: s.tokens_minted,
            tokens_burned: s.tokens_burned,
            tokens_minted_count: s.tokens_minted_count,
            tokens_burned_count: s.tokens_burned_count,
            accounts: store::state::get_accounts_len(),
            collected_utxos: store::state::get_collected_utxos_len(),
            burned_utxos: store::state::get_burned_utxos_len(),
            ledger_canister: s.ledger_canister,
            chain_canister: s.chain_canister,
            managers: s.managers.clone(),
            ..Default::default()
        };

        if is_controller_or_manager().is_ok() {
            res.ecdsa_key_name = Some(s.ecdsa_key_name.clone());
            res.burning_utxos = s.burning_utxos.clone();
            res.minter_address = s.get_address(&minter_account()).map(|v| v.to_string()).ok();
            res.minter_subaddress = s
                .get_address(&user_account(&ic_cdk::id()))
                .map(|v| v.to_string())
                .ok();
        }
        Ok(res)
    })
}

#[ic_cdk::query(guard = "is_authenticated")]
fn get_address() -> Result<String, String> {
    let addr = store::get_address(&user_account(&ic_cdk::caller()))?;
    Ok(addr.to_string())
}

#[ic_cdk::query]
fn list_minted_utxos(principal: Option<Principal>) -> Result<Vec<types::MintedUtxo>, String> {
    let principal = principal.unwrap_or(ic_cdk::caller());
    Ok(store::list_minted_utxos(principal))
}

#[ic_cdk::query]
fn list_collected_utxos(start: u64, take: u16) -> Vec<types::CollectedUtxo> {
    store::list_collected_utxos(start, take.clamp(1, 1000) as usize)
}

#[ic_cdk::query]
fn list_burned_utxos(start: u64, take: u16) -> Vec<types::BurnedUtxos> {
    store::list_burned_utxos(start, take.clamp(1, 1000) as usize)
}
