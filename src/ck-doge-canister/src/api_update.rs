use ck_doge_types::{
    amount, canister, err_string, jsonrpc::DogecoinRPC, script, sighash::*, transaction::*,
};
use serde_bytes::ByteBuf;
use std::str::FromStr;

use crate::{ecdsa, is_authenticated, store, Account};

#[ic_cdk::update(guard = "is_authenticated")]
async fn send_signed_transaction(
    input: canister::SendSignedTransactionInput,
) -> Result<canister::SendSignedTransactionOutput, String> {
    let tx = Transaction::try_from(input.tx.as_ref())?;
    let txid = tx.compute_txid();
    let agent = store::state::get_agent();
    let txid = DogecoinRPC::send_transaction(&agent, txid.to_string(), &tx).await?;
    Ok(canister::SendSignedTransactionOutput {
        txid: txid.into(),
        instructions: ic_cdk::api::performance_counter(1),
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn create_signed_transaction(
    input: canister::CreateSignedTransactionInput,
) -> Result<canister::CreateSignedTransactionOutput, String> {
    let addr = script::Address::from_str(&input.address)?;
    let account = Account {
        owner: ic_cdk::caller(),
        subaccount: None,
    };
    let (chain, key_name) = store::state::with(|s| (s.chain_params(), s.ecdsa_key_name.clone()));
    let mykey = store::get_public_key(&account)?;
    let myaddr = script::p2pkh_address(&mykey.public_key, chain)?;
    let pubkey = PublicKey::from_slice(&mykey.public_key).map_err(err_string)?;
    let script_pubkey = myaddr.to_script(chain);

    let uxtos = store::list_uxtos(&myaddr.0, 1000, false);
    let total_value: u64 = uxtos.iter().map(|u| u.value).sum();

    let mut send_tx = Transaction {
        version: Transaction::CURRENT_VERSION,
        lock_time: 0,
        input: uxtos.into_iter().map(|u| u.into()).collect(),
        output: vec![
            TxOut {
                value: input.amount,
                script_pubkey: addr.to_script(chain),
            },
            TxOut {
                value: total_value.saturating_sub(input.amount + input.fee),
                script_pubkey: script_pubkey.clone(),
            },
        ],
    };

    let size = send_tx.estimate_size();
    let fee = amount::fee_by_size(size).max(input.fee);
    if total_value < input.amount + fee {
        return Err(format!(
            "insufficient balance, expected: {}, got {}",
            input.amount + fee,
            total_value
        ));
    }

    let change = total_value.saturating_sub(input.amount + fee);
    if change < amount::DUST_LIMIT {
        send_tx.output.pop();
    } else {
        send_tx.output[1].value = change;
    }

    let input_len = send_tx.input.len();
    let path = ecdsa::account_path(&account);
    let mut sighasher = SighashCache::new(&mut send_tx);

    for i in 0..input_len {
        let hash = sighasher.signature_hash(i, &script_pubkey, EcdsaSighashType::All)?;
        let sig = ecdsa::sign_with(&key_name, path.clone(), &hash).await?;
        let signature = Signature::from_compact(&sig).map_err(err_string)?;
        sighasher
            .set_input_script(
                i,
                &SighashSignature {
                    signature,
                    sighash_type: EcdsaSighashType::All,
                },
                &pubkey,
            )
            .map_err(err_string)?;
    }

    Ok(canister::CreateSignedTransactionOutput {
        tx: ByteBuf::from(sighasher.transaction().to_bytes()),
        instructions: ic_cdk::api::performance_counter(1),
    })
}
