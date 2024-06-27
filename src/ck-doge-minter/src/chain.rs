use candid::Principal;
use dogecoin::{canister, transaction::Transaction};
use serde_bytes::ByteBuf;

use crate::call;

pub struct Chain(Principal);

impl Chain {
    pub fn new(principal: Principal) -> Self {
        Self(principal)
    }

    pub async fn send_tx(&self, tx: &Transaction) -> Result<canister::SendTxOutput, String> {
        call(
            self.0,
            "send_tx",
            (canister::SendTxInput {
                tx: ByteBuf::from(tx.to_bytes()),
                from_subaccount: None,
            },),
        )
        .await?
    }

    pub async fn get_tx_block_height(&self, txid: &canister::Txid) -> Result<Option<u64>, String> {
        call(self.0, "get_tx_block_height", (*txid.0,)).await
    }

    pub async fn list_utxos(
        &self,
        address: &canister::Address,
    ) -> Result<canister::UtxosOutput, String> {
        call(self.0, "list_utxos_b", (*address.0, 1000u16, true)).await?
    }
}
