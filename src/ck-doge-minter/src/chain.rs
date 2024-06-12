use candid::Principal;
use ck_doge_types::{canister, transaction::Transaction};
use serde_bytes::ByteBuf;

use crate::call;

pub struct Chain(Principal);

impl Chain {
    pub fn new(principal: Principal) -> Self {
        Self(principal)
    }

    pub async fn send_signed_transaction(
        &self,
        tx: &Transaction,
    ) -> Result<canister::SendSignedTransactionOutput, String> {
        call(
            self.0,
            "send_signed_transaction",
            (canister::SendSignedTransactionInput {
                tx: ByteBuf::from(tx.to_bytes()),
            },),
        )
        .await?
    }

    pub async fn list_utxos(
        &self,
        address: &canister::Address,
    ) -> Result<canister::UtxosOutput, String> {
        call(self.0, "list_utxos_b", (address.0, 1000u16, true)).await?
    }
}
