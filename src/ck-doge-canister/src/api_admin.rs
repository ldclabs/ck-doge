use candid::Principal;
use dogecoin::canister;
use std::collections::BTreeSet;
use std::time::Duration;

use crate::{is_controller, is_controller_or_manager, store, syncing, ANONYMOUS};

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
async fn admin_set_agent(agents: Vec<canister::RPCAgent>) -> Result<(), String> {
    if agents.is_empty() {
        return Err("agents cannot be empty".to_string());
    }
    let ecdsa_key_name = store::state::with(|s| s.ecdsa_key_name.clone());
    syncing::update_proxy_token(ecdsa_key_name, agents).await;

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
async fn admin_restart_syncing(for_status: Option<i8>) -> Result<(), String> {
    store::syncing::with_mut(|s| {
        let status = if let Some(status) = for_status {
            if let Some(timer) = s.timer {
                ic_cdk_timers::clear_timer(timer);
            }
            status
        } else {
            s.status
        };

        match status {
            0 | -1 => {
                s.status = 0;
                s.timer = Some(ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                    ic_cdk::spawn(syncing::fetch_block())
                }));
            }
            -2 => {
                store::clear_for_restart_process_block();
                s.status = 0;
                s.timer = Some(ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                    ic_cdk::spawn(syncing::fetch_block())
                }));
            }
            -3 => {
                store::clear_for_restart_confirm_utxos();
                s.status = 0;
                s.timer = Some(ic_cdk_timers::set_timer(Duration::from_secs(0), || {
                    ic_cdk::spawn(syncing::fetch_block())
                }));
            }
            status => {
                return Err(format!("invalid status {}", status));
            }
        }
        Ok(())
    })?;

    syncing::refresh_proxy_token().await;
    Ok(())
}
