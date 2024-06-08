use candid::Principal;
use ck_doge_types::{canister, jsonrpc::DogecoinRPC};
use std::collections::BTreeSet;
use std::time::Duration;

use crate::{ecdsa, is_controller, is_controller_or_manager, store, ANONYMOUS, SECONDS};

pub const UPDATE_PROXY_TOKEN_INTERVAL: u64 = 60 * 3; // 3 minute
const FETCH_BLOCK_INTERVAL: u64 = 20; // 30 seconds

#[ic_cdk::update(guard = "is_controller")]
fn admin_set_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    store::state::with_mut(|r| {
        r.managers = args;
    });
    Ok(())
}

#[ic_cdk::update]
fn validate_admin_set_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    if args.is_empty() {
        return Err("managers cannot be empty".to_string());
    }
    if args.contains(&ANONYMOUS) {
        return Err("anonymous user is not allowed".to_string());
    }
    Ok(())
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
async fn admin_set_agent(arg: canister::RPCAgent) -> Result<(), String> {
    let mut rpc_agent = arg;
    let token = ecdsa::sign_proxy_token(
        &store::state::with(|s| s.ecdsa_key_name.clone()),
        (ic_cdk::api::time() / SECONDS) + UPDATE_PROXY_TOKEN_INTERVAL * 2,
        &rpc_agent.name,
    )
    .await?;

    rpc_agent.proxy_token = Some(token);
    store::state::with_mut(|s| {
        s.rpc_agent = rpc_agent;
    });

    if store::state::runtime(|s| s.update_proxy_token_interval.is_none()) {
        store::state::runtime_mut(|s| {
            s.update_proxy_token_interval = Some(ic_cdk_timers::set_timer_interval(
                Duration::from_secs(UPDATE_PROXY_TOKEN_INTERVAL),
                || ic_cdk::spawn(update_proxy_token_interval()),
            ));
        });
    }
    Ok(())
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
async fn admin_restart_sync_job() -> Result<(), String> {
    store::state::with_mut(|s| s.last_errors.clear());
    match store::state::runtime(|s| s.sync_job_running) {
        0 | -1 => {
            ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                ic_cdk::spawn(sync_job_fetch_block())
            });
        }
        -2 => {
            store::clear_for_restart_process_block();
            ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                ic_cdk::spawn(sync_job_fetch_block())
            });
        }
        -3 => {
            store::clear_for_restart_confirm_utxos();
            ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                ic_cdk::spawn(sync_job_fetch_block())
            });
        }
        status => {
            return Err(format!(
                "sync job is already running, currently at {}",
                status
            ));
        }
    }
    Ok(())
}

pub async fn update_proxy_token_interval() {
    let (ecdsa_key_name, mut rpc_agent) =
        store::state::with(|s| (s.ecdsa_key_name.clone(), s.rpc_agent.clone()));
    let token = ecdsa::sign_proxy_token(
        &ecdsa_key_name,
        (ic_cdk::api::time() / SECONDS) + UPDATE_PROXY_TOKEN_INTERVAL * 2,
        &rpc_agent.name,
    )
    .await
    .expect("failed to sign proxy token");

    rpc_agent.proxy_token = Some(token);
    store::state::with_mut(|r| {
        r.rpc_agent = rpc_agent;
    });
}

enum FetchBlockError {
    ShouldWait(String),
    Other(String),
}

pub async fn sync_job_fetch_block() {
    store::state::runtime_mut(|s| s.sync_job_running = 1);
    let res: Result<(), FetchBlockError> = async {
        let agent = store::state::get_agent();
        let (tip_height, tip_blockhash) = store::state::with(|s| (s.tip_height, s.tip_blockhash));
        let height = if tip_blockhash == [0u8; 32] {
            0
        } else {
            tip_height + 1
        };

        let blockhash = DogecoinRPC::get_blockhash(&agent, "", height)
            .await
            .map_err(FetchBlockError::ShouldWait)?;
        let block = DogecoinRPC::get_block(&agent, "", &blockhash)
            .await
            .map_err(FetchBlockError::Other)?;
        store::append_block(height, blockhash, block).map_err(FetchBlockError::Other)?;
        Ok(())
    }
    .await;

    match res {
        Err(FetchBlockError::Other(err)) => {
            store::state::runtime_mut(|s| s.sync_job_running = -1);
            store::state::with_mut(|s| s.last_errors.push(err.clone()));
            ic_cdk::trap(&err);
        }
        Err(FetchBlockError::ShouldWait(err)) => {
            store::state::with_mut(|s| s.last_errors.push(err.clone()));
            ic_cdk_timers::set_timer(Duration::from_secs(FETCH_BLOCK_INTERVAL), || {
                ic_cdk::spawn(sync_job_fetch_block())
            });
        }
        Ok(_) => {
            store::state::with_mut(|s| s.last_errors.clear());
            ic_cdk_timers::set_timer(Duration::from_secs(0), sync_job_process_block);
        }
    }
}

fn sync_job_process_block() {
    store::state::runtime_mut(|s| s.sync_job_running = 2);
    match store::process_block() {
        Err(err) => {
            store::state::runtime_mut(|s| s.sync_job_running = -2);
            store::state::with_mut(|s| s.last_errors.push(err.clone()));
            ic_cdk::trap(&err);
        }
        Ok(res) => {
            if res {
                ic_cdk_timers::set_timer(Duration::from_secs(0), sync_job_confirm_utxos);
            } else {
                ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                    ic_cdk::spawn(sync_job_fetch_block())
                });
            }
        }
    }
}

fn sync_job_confirm_utxos() {
    store::state::runtime_mut(|s| s.sync_job_running = 3);
    match store::confirm_utxos() {
        Err(err) => {
            store::state::runtime_mut(|s| s.sync_job_running = -3);
            store::state::with_mut(|s| s.last_errors.push(err.clone()));
            ic_cdk::trap(&err);
        }
        Ok(res) => {
            if res {
                ic_cdk_timers::set_timer(Duration::from_secs(0), sync_job_process_block);
            } else {
                ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                    ic_cdk::spawn(sync_job_fetch_block())
                });
            }
        }
    }
}
