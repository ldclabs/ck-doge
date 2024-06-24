use candid::CandidType;
use dogecoin::block::BlockHash;
use serde::Deserialize;
use std::{str::FromStr, time::Duration};

use crate::{store, syncing};

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ChainArgs {
    Init(InitArgs),
    Upgrade(UpgradeArgs),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    chain: u8,              // TEST_NET_DOGE: 32, MAIN_NET_DOGE: 16
    min_confirmations: u32, // recommended: 42
    ecdsa_key_name: String, // Use "dfx_test_key" for local replica and "test_key_1" for a testing key for testnet and mainnet
    prev_start_height: u64,
    prev_start_blockhash: String,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct UpgradeArgs {
    min_confirmations: Option<u32>, // recommended: 42
}

#[ic_cdk::init]
fn init(args: Option<ChainArgs>) {
    match args.expect("Init args is missing") {
        ChainArgs::Init(args) => {
            store::state::with_mut(|s| {
                s.chain = args.chain;
                s.min_confirmations = args.min_confirmations;
                s.ecdsa_key_name = args.ecdsa_key_name;
                s.tip_height = args.prev_start_height;
                if args.prev_start_height > 0 || !args.prev_start_blockhash.is_empty() {
                    s.tip_blockhash = (*BlockHash::from_str(&args.prev_start_blockhash)
                        .expect("invalid blockhash"))
                    .into();
                }
            });
        }
        ChainArgs::Upgrade(_) => {
            ic_cdk::trap(
                "Cannot initialize the canister with an Upgrade args. Please provide an Init args.",
            );
        }
    }

    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
        ic_cdk::spawn(store::state::init_ecdsa_public_key())
    });
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    store::state::save();
}

#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<ChainArgs>) {
    store::state::load();

    match args {
        Some(ChainArgs::Upgrade(args)) => {
            store::state::with_mut(|s| {
                if let Some(min_confirmations) = args.min_confirmations {
                    s.min_confirmations = min_confirmations;
                }
            });
        }
        Some(ChainArgs::Init(_)) => {
            ic_cdk::trap(
                "Cannot upgrade the canister with an Init args. Please provide an Upgrade args.",
            );
        }
        _ => {}
    }

    store::syncing::with_mut(|s| {
        s.timer = Some(ic_cdk_timers::set_timer(Duration::from_secs(0), || {
            ic_cdk::spawn(async {
                syncing::refresh_proxy_token().await;
                syncing::fetch_block().await;
            })
        }));

        s.refresh_proxy_token_timer = Some(ic_cdk_timers::set_timer_interval(
            Duration::from_secs(syncing::REFRESH_PROXY_TOKEN_INTERVAL),
            || ic_cdk::spawn(syncing::refresh_proxy_token()),
        ));
    });
}
