use crate::{is_authenticated, store, types};

#[ic_cdk::update(guard = "is_authenticated")]
async fn mint_ckdoge() -> Result<types::MintOutput, String> {
    let amount = store::mint_ckdoge(ic_cdk::caller()).await?;
    Ok(types::MintOutput {
        amount,
        instructions: ic_cdk::api::performance_counter(1),
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn burn_ckdoge(args: types::BurnInput) -> Result<types::BurnOutput, String> {
    let res =
        store::burn_ckdoge(ic_cdk::caller(), args.address, args.amount, args.fee_rate).await?;

    Ok(types::BurnOutput {
        txid: res.txid,
        tip_height: res.tip_height,
        instructions: ic_cdk::api::performance_counter(1),
    })
}
