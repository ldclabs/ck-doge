use candid::CandidType;
use ck_doge_types::block::BlockHash;
use serde::Deserialize;
use std::{str::FromStr, time::Duration};

use crate::api_admin::{
    sync_job_fetch_block, update_proxy_token_interval, UPDATE_PROXY_TOKEN_INTERVAL,
};
use crate::store;

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArg {
    chain: u8,              // TEST_NET_DOGE: 32, MAIN_NET_DOGE: 16
    min_confirmations: u32, // recommended: 42
    ecdsa_key_name: String, // Use "dfx_test_key" for local replica and "test_key_1" for a testing key for testnet and mainnet
    prev_start_height: u64,
    prev_start_blockhash: String,
}

#[ic_cdk::init]
fn init(arg: InitArg) {
    store::state::with_mut(|s| {
        s.chain = arg.chain;
        s.min_confirmations = arg.min_confirmations;
        s.ecdsa_key_name = arg.ecdsa_key_name;
        s.tip_height = arg.prev_start_height;
        if arg.prev_start_height > 0 || !arg.prev_start_blockhash.is_empty() {
            s.tip_blockhash =
                *BlockHash::from_str(&arg.prev_start_blockhash).expect("invalid blockhash");
        }
    });

    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
        ic_cdk::spawn(store::state::init_ecdsa_public_key())
    });
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    store::state::save();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    store::state::load();

    store::state::runtime_mut(|s| {
        ic_cdk_timers::set_timer(Duration::from_secs(0), || {
            ic_cdk::spawn(sync_job_fetch_block())
        });

        s.update_proxy_token_interval = Some(ic_cdk_timers::set_timer_interval(
            Duration::from_secs(UPDATE_PROXY_TOKEN_INTERVAL),
            || ic_cdk::spawn(update_proxy_token_interval()),
        ));
    });
}
