use candid::{CandidType, Principal};
use dogecoin::canister;
use std::collections::BTreeSet;

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
    pub accounts: u64,
    pub collected_utxos: u64,
    pub burned_utxos: u64,
    pub ledger_canister: Option<Principal>,
    pub chain_canister: Option<Principal>,
    pub managers: BTreeSet<Principal>,
    // manager info
    pub ecdsa_key_name: Option<String>,
    pub utxos_retry_burning_queue: Vec<(u64, canister::Address, u64, u64, u8)>,
    pub minter_address: Option<String>,
    pub minter_subaddress: Option<String>,
}

#[ic_cdk::query]
fn get_state() -> Result<State, ()> {
    Ok(store::state::with(|s| {
        let mut res = State {
            chain: s.chain_params().chain_name.to_string(),
            tokens_minted: s.tokens_minted,
            tokens_burned: s.tokens_burned,
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
            res.utxos_retry_burning_queue = s.utxos_retry_burning_queue.clone().into();
            res.minter_address = s.get_address(&minter_account()).map(|v| v.to_string()).ok();
            res.minter_subaddress = s
                .get_address(&user_account(&ic_cdk::id()))
                .map(|v| v.to_string())
                .ok();
        }
        res
    }))
}

#[ic_cdk::query]
fn list_minted_utxos(principal: Option<Principal>) -> Result<Vec<types::MintedUtxo>, String> {
    let principal = principal.unwrap_or(ic_cdk::caller());
    Ok(store::list_minted_utxos(principal))
}

#[ic_cdk::query(guard = "is_authenticated")]
fn get_address() -> Result<String, String> {
    let addr = store::get_address(&user_account(&ic_cdk::caller()))?;
    Ok(addr.to_string())
}
