use candid::{CandidType, Principal};
use serde::Deserialize;
use std::time::Duration;

use crate::{store, task};

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum MinterArgs {
    Init(InitArgs),
    Upgrade(UpgradeArgs),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    chain: u8,              // TEST_NET_DOGE: 32, MAIN_NET_DOGE: 16
    ecdsa_key_name: String, // Use "dfx_test_key" for local replica and "test_key_1" for a testing key for testnet and mainnet
    ledger_canister: Option<Principal>,
    chain_canister: Option<Principal>,
}

#[ic_cdk::init]
fn init(args: Option<MinterArgs>) {
    match args.expect("Init args is missing") {
        MinterArgs::Init(args) => {
            store::state::with_mut(|s| {
                s.chain = args.chain;
                s.ecdsa_key_name = args.ecdsa_key_name;
                s.ledger_canister = args.ledger_canister;
                s.chain_canister = args.chain_canister;
            });
        }
        MinterArgs::Upgrade(_) => {
            ic_cdk::trap(
                "Cannot initialize the canister with an Upgrade args. Please provide an Init args.",
            );
        }
    }

    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
        ic_cdk::spawn(store::state::init_ecdsa_public_key())
    });

    ic_cdk_timers::set_timer_interval(Duration::from_secs(task::FINALIZE_BURNING_INTERVAL), || {
        ic_cdk::spawn(task::finalize_burning())
    });

    ic_cdk_timers::set_timer_interval(Duration::from_secs(task::CLEAR_UTXOS_INTERVAL), || {
        ic_cdk::spawn(task::collect_and_clear_utxos())
    });
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    store::state::save();
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct UpgradeArgs {
    ledger_canister: Option<Principal>,
    chain_canister: Option<Principal>,
}

#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<MinterArgs>) {
    store::state::load();

    match args {
        Some(MinterArgs::Upgrade(args)) => {
            store::state::with_mut(|s| {
                if let Some(ledger_canister) = args.ledger_canister {
                    s.ledger_canister = Some(ledger_canister);
                }
                if let Some(chain_canister) = args.chain_canister {
                    s.chain_canister = Some(chain_canister);
                }
            });
        }
        Some(MinterArgs::Init(_)) => {
            ic_cdk::trap(
                "Cannot upgrade the canister with an Init args. Please provide an Upgrade args.",
            );
        }
        _ => {}
    }

    ic_cdk_timers::set_timer_interval(Duration::from_secs(task::FINALIZE_BURNING_INTERVAL), || {
        ic_cdk::spawn(task::finalize_burning())
    });

    ic_cdk_timers::set_timer_interval(Duration::from_secs(task::CLEAR_UTXOS_INTERVAL), || {
        ic_cdk::spawn(task::collect_and_clear_utxos())
    });
}
