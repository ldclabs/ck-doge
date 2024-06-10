use bitcoin::hashes::sha256d;
use candid::{CandidType, Principal};
use ck_doge_types::canister::*;
use std::{collections::BTreeSet, str::FromStr};

use crate::{is_authenticated, is_controller_or_manager, store, Account};

#[ic_cdk::query]
fn api_version() -> u16 {
    1
}

#[derive(CandidType, Default)]
pub struct State {
    pub chain: String,
    pub min_confirmations: u32,
    pub tip_height: u64,
    pub tip_blockhash: String,
    pub processed_height: u64,
    pub processed_blockhash: String,
    pub confirmed_height: u64,
    pub start_height: u64,
    pub start_blockhash: String,
    pub unprocessed_blocks: u64,
    pub unconfirmed_utxs: u64,
    pub unconfirmed_utxos: u64,
    pub confirmed_utxs: u64,
    pub confirmed_utxos: u64,
    pub last_errors: Vec<String>,
    pub managers: BTreeSet<Principal>,
    // manager info
    pub rpc_proxy_public_key: Option<String>,
    pub rpc_agent: Option<RPCAgent>,
    pub ecdsa_key_name: Option<String>,
}

#[ic_cdk::query]
fn get_state() -> Result<State, ()> {
    Ok(store::state::with(|s| {
        let mut res = State {
            chain: s.chain_params().chain_name.to_string(),
            min_confirmations: s.min_confirmations,
            tip_height: s.tip_height,
            tip_blockhash: sha256d::Hash::from_bytes_ref(&s.tip_blockhash).to_string(),
            processed_height: s.processed_height,
            processed_blockhash: sha256d::Hash::from_bytes_ref(&s.processed_blockhash).to_string(),
            confirmed_height: s.confirmed_height,
            start_height: s.start_height,
            start_blockhash: sha256d::Hash::from_bytes_ref(&s.start_blockhash).to_string(),
            unprocessed_blocks: store::state::get_unprocessed_blocks_len(),
            unconfirmed_utxs: s.unconfirmed_utxs.len() as u64,
            unconfirmed_utxos: s.unconfirmed_utxos.len() as u64,
            confirmed_utxs: store::state::get_confirmed_utxs_len(),
            confirmed_utxos: store::state::get_confirmed_utxos_len(),
            last_errors: s.last_errors.clone().into(),
            managers: s.managers.clone(),
            ..Default::default()
        };

        if is_controller_or_manager().is_ok() {
            res.ecdsa_key_name = Some(s.ecdsa_key_name.clone());
            res.rpc_proxy_public_key = Some(s.rpc_proxy_public_key.clone());
            res.rpc_agent = Some(s.rpc_agent.clone());
        }
        res
    }))
}

#[ic_cdk::query]
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

#[ic_cdk::query(guard = "is_authenticated")]
fn get_address() -> Result<String, String> {
    let addr = store::get_address(&Account {
        owner: ic_cdk::caller(),
        subaccount: None,
    })?;

    Ok(addr.to_string())
}

#[ic_cdk::query]
fn get_utx(id: String) -> Result<UnspentTx, String> {
    let txid = Txid::from_str(&id)?;
    store::get_utx(&txid.0).ok_or(format!("tx {id} not found"))
}

#[ic_cdk::query]
fn get_utx_b(txid: [u8; 32]) -> Option<UnspentTx> {
    store::get_utx(&txid)
}

#[ic_cdk::query]
fn list_utxos(addr: String, take: u16, confirmed: bool) -> Result<Vec<Utxo>, String> {
    let address = Address::from_str(&addr)?;
    Ok(store::list_utxos(
        &address.0,
        take.max(10).min(10000) as usize,
        confirmed,
    ))
}

#[ic_cdk::query]
fn list_utxos_b(address: [u8; 21], take: u16, confirmed: bool) -> Vec<Utxo> {
    store::list_utxos(&address, take.max(10).min(10000) as usize, confirmed)
}

#[ic_cdk::query]
fn get_balance(addr: String) -> Result<u64, String> {
    let address = Address::from_str(&addr)?;
    Ok(store::get_balance(&address.0))
}

#[ic_cdk::query]
fn get_balance_b(address: [u8; 21]) -> u64 {
    store::get_balance(&address)
}
