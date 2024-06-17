use dogecoin::{
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
        tip_height: store::state::with(|s| s.tip_height),
        instructions: ic_cdk::api::performance_counter(1),
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn create_signed_transaction(
    input: canister::CreateSignedTransactionInput,
) -> Result<canister::CreateSignedTransactionOutput, String> {
    let receiver = script::Address::from_str(&input.address)?;
    let sender = Account {
        owner: ic_cdk::caller(),
        subaccount: input.from_subaccount.map(|v| *v),
    };
    let sender_key_path = ecdsa::account_path(&sender);

    let (chain, key_name) = store::state::with(|s| (s.chain_params(), s.ecdsa_key_name.clone()));
    let sender_key = store::get_public_key(sender_key_path.clone())?;
    let myaddr = script::p2pkh_address(&sender_key.public_key, chain)?;
    let pubkey = PublicKey::from_slice(&sender_key.public_key).map_err(err_string)?;
    let script_pubkey = myaddr.to_script(chain);

    let utxos = store::list_utxos(&myaddr.0.into(), 1000, false);
    let total_value: u64 = utxos.iter().map(|u| u.value).sum();

    let mut send_tx = Transaction {
        version: Transaction::CURRENT_VERSION,
        lock_time: 0,
        input: utxos.into_iter().map(|u| u.into()).collect(),
        output: vec![
            TxOut {
                value: input.amount,
                script_pubkey: receiver.to_script(chain),
            },
            TxOut {
                value: total_value.saturating_sub(input.amount),
                script_pubkey: script_pubkey.clone(),
            },
        ],
    };

    let fee = amount::fee_by_size(send_tx.estimate_size() as u64, input.fee_rate);
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
    let mut sighasher = SighashCache::new(&mut send_tx);

    for i in 0..input_len {
        let hash = sighasher.signature_hash(i, &script_pubkey, EcdsaSighashType::All)?;
        let sig = ecdsa::sign_with(&key_name, sender_key_path.clone(), *hash).await?;
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
        tip_height: store::state::with(|s| s.tip_height),
        instructions: ic_cdk::api::performance_counter(1),
    })
}
