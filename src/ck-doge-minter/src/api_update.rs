use crate::{is_authenticated, is_controller_or_manager, store, types};

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
    store::burn_ckdoge(ic_cdk::caller(), args.address, args.amount, args.fee_rate).await
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn retry_burn_ckdoge(
    block_index: u64,
    fee_rate: Option<u64>,
) -> Result<types::BurnOutput, String> {
    store::retry_burn_utxos(
        block_index,
        fee_rate,
        if is_controller_or_manager().is_ok() {
            None
        } else {
            Some(ic_cdk::caller())
        },
    )
    .await
}
