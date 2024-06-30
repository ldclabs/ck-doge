use candid::{utils::ArgumentEncoder, Nat, Principal};
use ciborium::into_writer;
use dogecoin::canister;
use num_traits::cast::ToPrimitive;
use serde::Serialize;
use std::collections::BTreeSet;

use crate::api_init::MinterArgs;
use crate::api_query::State;
pub use icrc_ledger_types::icrc1::account::Account;

mod api_admin;
mod api_init;
mod api_query;
mod api_update;
mod chain;
mod ecdsa;
mod ledger;
mod store;
mod task;
mod types;

static ANONYMOUS: Principal = Principal::anonymous();
const MILLISECONDS: u64 = 1_000_000u64;

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::caller();
    if ic_cdk::api::is_controller(&caller) {
        Ok(())
    } else {
        Err("user is not a controller".to_string())
    }
}

fn is_controller_or_manager() -> Result<(), String> {
    let caller = ic_cdk::caller();
    if ic_cdk::api::is_controller(&caller) || store::state::is_manager(&caller) {
        Ok(())
    } else {
        Err("user is not a controller or manager".to_string())
    }
}

fn is_authenticated() -> Result<(), String> {
    if ic_cdk::caller() == ANONYMOUS {
        Err("anonymous user is not allowed".to_string())
    } else {
        Ok(())
    }
}

fn nat_to_u64(nat: &Nat) -> u64 {
    nat.0.to_u64().expect("nat does not fit into u64")
}

fn minter_account() -> Account {
    Account {
        owner: ic_cdk::id(),
        subaccount: None,
    }
}

fn user_account(owner: &Principal) -> Account {
    Account {
        owner: ic_cdk::id(),
        subaccount: canister::sha3_256(owner.as_slice()).into(),
    }
}

fn to_cbor_bytes(obj: &impl Serialize) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    into_writer(obj, &mut buf).expect("failed to encode in CBOR format");
    buf
}

async fn call<In, Out>(id: Principal, method: &str, args: In) -> Result<Out, String>
where
    In: ArgumentEncoder + Send,
    Out: candid::CandidType + for<'a> candid::Deserialize<'a>,
{
    let (res,): (Out,) = ic_cdk::call(id, method, args)
        .await
        .map_err(|(code, msg)| {
            format!(
                "failed to call {} on {:?}, code: {}, message: {}",
                method, &id, code as u32, msg
            )
        })?;
    Ok(res)
}

#[cfg(all(
    target_arch = "wasm32",
    target_vendor = "unknown",
    target_os = "unknown"
))]
/// A getrandom implementation that always fails
pub fn always_fail(_buf: &mut [u8]) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}

#[cfg(all(
    target_arch = "wasm32",
    target_vendor = "unknown",
    target_os = "unknown"
))]
getrandom::register_custom_getrandom!(always_fail);

ic_cdk::export_candid!();
