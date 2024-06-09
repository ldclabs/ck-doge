use candid::Principal;
use ck_doge_types::canister;
use std::collections::BTreeSet;
use std::time::Duration;

use crate::{ecdsa, is_controller, is_controller_or_manager, store, syncing, ANONYMOUS, SECONDS};

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
async fn admin_set_agent(mut arg: canister::RPCAgent) -> Result<(), String> {
    let token = ecdsa::sign_proxy_token(
        &store::state::with(|s| s.ecdsa_key_name.clone()),
        (ic_cdk::api::time() / SECONDS) + syncing::REFRESH_PROXY_TOKEN_INTERVAL * 2,
        &arg.name,
    )
    .await?;

    arg.proxy_token = Some(token);
    store::state::with_mut(|s| {
        s.rpc_agent = arg;
    });

    if store::syncing::with(|s| s.refresh_proxy_token_timer.is_none()) {
        store::syncing::with_mut(|s| {
            s.refresh_proxy_token_timer = Some(ic_cdk_timers::set_timer_interval(
                Duration::from_secs(syncing::REFRESH_PROXY_TOKEN_INTERVAL),
                || ic_cdk::spawn(syncing::refresh_proxy_token()),
            ));
        });
    }
    Ok(())
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
async fn admin_restart_syncing() -> Result<(), String> {
    store::state::with_mut(|s| s.last_errors.clear());
    match store::syncing::with_mut(|s| s.status) {
        0 | -1 => {
            ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                ic_cdk::spawn(syncing::fetch_block())
            });
        }
        -2 => {
            store::clear_for_restart_process_block();
            ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                ic_cdk::spawn(syncing::fetch_block())
            });
        }
        -3 => {
            store::clear_for_restart_confirm_utxos();
            ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                ic_cdk::spawn(syncing::fetch_block())
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
