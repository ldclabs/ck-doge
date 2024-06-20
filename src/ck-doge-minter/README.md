# CK-Doge: `ck-doge-minter`
Mint and burn `ckDOGE` <-> `DOGE` on the [Internet Computer](https://internetcomputer.org/).

## Running the project locally

If you want to test your project locally, you can use the following commands:

```bash
# start the replica
dfx start

# deploy minter canister
dfx deploy ck-doge-minter --argument "(opt variant {Init =
  record {
    chain = 32;
    ecdsa_key_name = \"dfx_test_key\";
    ledger_canister = null;
    chain_canister = opt principal \"be2us-64aaa-aaaaa-qaabq-cai\";
  }
})"
# canister: bw4dl-smaaa-aaaaa-qaacq-cai

dfx canister call ck-doge-minter get_state '()'

# deploy ckDOGE canister
dfx deploy icrc1_ledger_canister --argument "(variant {Init =
  record {
    token_symbol = \"ckDOGETEST\";
    token_name = \"Dogecoin TEST\";
    decimals = opt 8;
    max_memo_length = opt 80;
    transfer_fee = 100_000;
    minting_account = record { owner = principal \"bw4dl-smaaa-aaaaa-qaacq-cai\" };
    fee_collector_account = opt record { owner = principal \"i2gam-uue3y-uxwyd-mzyhb-nirhd-hz3l4-2hw3f-4fzvw-lpvvc-dqdrg-7qe\" };
    metadata = vec {};
    feature_flags = opt record{icrc2 = true};
    initial_balances = vec {};
    archive_options = record {
        num_blocks_to_archive = 10000;
        trigger_threshold = 2000;
        controller_id = principal \"i2gam-uue3y-uxwyd-mzyhb-nirhd-hz3l4-2hw3f-4fzvw-lpvvc-dqdrg-7qe\";
    };
  }
})"
# canister: b77ix-eeaaa-aaaaa-qaada-cai

# upgrade the minter canister with the ledger_canister principal
dfx deploy ck-doge-minter --argument "(opt variant {Upgrade =
  record {
    ledger_canister = opt principal \"b77ix-eeaaa-aaaaa-qaada-cai\";
    chain_canister = null;
  }
})"

dfx canister call ck-doge-minter get_state '()'

# get my DOGE address
dfx canister call ck-doge-minter get_address '()'

# mint ckDOGE after transferring DOGE to my DOGE address
dfx canister call ck-doge-minter mint_ckdoge '()'

# approve the minter canister to burn ckDOGE
dfx canister call b77ix-eeaaa-aaaaa-qaada-cai icrc2_approve '(record {spender=record {owner=principal "bw4dl-smaaa-aaaaa-qaacq-cai"; subaccount=null}; fee=null; memo=null; from_subaccount=null; created_at_time=null; amount=1_000_000_000})'

# burn ckDOGE
dfx canister call ck-doge-minter burn_ckdoge '(record {
  address = "nehsFf3XXG7PfFCJoUcgTkCE9xRMR8XQaF";
  amount = 200_000_000;
  fee_rate = 1000;
})'

# list my minted utxos
dfx canister call ck-doge-minter list_minted_utxos '(null)'
```

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).

`ldclabs/ck-doge` is licensed under the MIT License. See [LICENSE](LICENSE-MIT) for the full license text.