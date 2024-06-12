use candid::{Nat, Principal};
use icrc_ledger_types::{
    icrc1::transfer::{Memo, TransferArg, TransferError},
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};

use crate::{call, minter_account, nat_to_u64, Account};

pub struct Ledger(Principal);

impl Ledger {
    pub fn new(principal: Principal) -> Self {
        Self(principal)
    }

    pub async fn balance_of(&self, account: Account) -> Result<u64, String> {
        let res: Nat = call(self.0, "icrc1_balance_of", (account,)).await?;
        Ok(nat_to_u64(&res))
    }

    pub async fn transfer(&self, args: TransferArg) -> Result<u64, String> {
        let res: Result<Nat, TransferError> = call(self.0, "icrc1_transfer", (args,)).await?;
        let res = res.map_err(|err| err.to_string())?;
        Ok(nat_to_u64(&res))
    }

    pub async fn mint(&self, amount: u64, to: Account, memo: Memo) -> Result<u64, String> {
        self.transfer(TransferArg {
            from_subaccount: None,
            to,
            fee: None,
            created_at_time: None,
            memo: Some(memo),
            amount: Nat::from(amount),
        })
        .await
    }

    pub async fn burn(&self, amount: u64, from: Account, memo: Memo) -> Result<u64, String> {
        let res: Result<Nat, TransferFromError> = call(
            self.0,
            "icrc2_transfer_from",
            (TransferFromArgs {
                spender_subaccount: None,
                from,
                to: minter_account(),
                amount: Nat::from(amount),
                fee: None,
                memo: Some(memo),
                created_at_time: None,
            },),
        )
        .await?;
        let res = res.map_err(|err| err.to_string())?;
        Ok(nat_to_u64(&res))
    }
}
