use bitcoin::hashes::sha256d;
use candid::{CandidType, Principal};
use ck_doge_types::canister::*;
use std::{collections::BTreeSet, str::FromStr};

use crate::{store, Account};

#[ic_cdk::query]
fn api_version() -> u16 {
    1
}

#[derive(CandidType)]
pub struct State {
    pub chain: String,
    pub min_confirmations: u32,
    pub rpc_proxy_public_key: String,
    pub tip_height: u64,
    pub tip_blockhash: String,
    pub processed_height: u64,
    pub processed_blockhash: String,
    pub confirmed_height: u64,
    pub start_height: u64,
    pub start_blockhash: String,
    pub pull_block_retries: u32,
    pub last_errors: Vec<String>,
    pub managers: BTreeSet<Principal>,
}

#[ic_cdk::query]
fn query_state() -> Result<State, ()> {
    Ok(store::state::with(|s| State {
        chain: s.chain_params().chain_name.to_string(),
        min_confirmations: s.min_confirmations,
        rpc_proxy_public_key: s.rpc_proxy_public_key.clone(),
        tip_height: s.tip_height,
        tip_blockhash: sha256d::Hash::from_bytes_ref(&s.tip_blockhash).to_string(),
        processed_height: s.processed_height,
        processed_blockhash: sha256d::Hash::from_bytes_ref(&s.processed_blockhash).to_string(),
        confirmed_height: s.confirmed_height,
        start_height: s.start_height,
        start_blockhash: sha256d::Hash::from_bytes_ref(&s.start_blockhash).to_string(),
        pull_block_retries: s.pull_block_retries,
        last_errors: s.last_errors.clone(),
        managers: s.managers.clone(),
    }))
}

#[ic_cdk::query(composite = true)]
fn get_tip() -> Result<BlockRef, String> {
    store::state::with(|s| {
        if s.tip_height == 0 {
            return Err("no tip".to_string());
        }

        Ok(BlockRef {
            hash: sha256d::Hash::from_bytes_ref(&s.tip_blockhash).to_string(),
            height: s.tip_height,
        })
    })
}

#[ic_cdk::query]
fn query_address() -> Result<String, String> {
    let addr = store::get_address(&Account {
        owner: ic_cdk::caller(),
        subaccount: None,
    })?;

    Ok(addr.to_string())
}

#[ic_cdk::query(composite = true)]
fn get_address() -> Result<String, String> {
    let addr = store::get_address(&Account {
        owner: ic_cdk::caller(),
        subaccount: None,
    })?;

    Ok(addr.to_string())
}

#[ic_cdk::query]
fn query_tx(id: String) -> Result<UnspentTx, String> {
    let txid = Txid::from_str(&id)?;
    store::get_tx(&txid.0).ok_or(format!("tx {id} not found"))
}

#[ic_cdk::query(composite = true)]
fn get_tx(id: String) -> Result<UnspentTx, String> {
    let txid = Txid::from_str(&id)?;
    store::get_tx(&txid.0).ok_or(format!("tx {id} not found"))
}

#[ic_cdk::query]
fn query_uxtos(addr: String, take: u16, confirmed: bool) -> Result<Vec<Utxo>, String> {
    let address = Address::from_str(&addr)?;
    Ok(store::list_uxtos(
        &address.0,
        take.max(10).min(10000) as usize,
        confirmed,
    ))
}

#[ic_cdk::query(composite = true)]
fn list_uxtos(addr: String, take: u16, confirmed: bool) -> Result<Vec<Utxo>, String> {
    let address = Address::from_str(&addr)?;
    Ok(store::list_uxtos(
        &address.0,
        take.max(10).min(10000) as usize,
        confirmed,
    ))
}
