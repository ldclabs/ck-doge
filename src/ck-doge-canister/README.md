# ckDOGE: ck-doge-canister
Chain-key Dogecoin (ckDOGE) is an ICRC-2-compliant token that is backed 1:1 by DOGE held 100% on the [ICP](https://internetcomputer.org/).

The `ck-doge-canister` enables other canisters deployed on the Internet Computer to use Dogecoin and interact with the Dogecoin network

## Running the project locally

If you want to test your project locally, you can use the following commands:

```bash
# start the replica
dfx start

# deploy the canister
dfx deploy ck-doge-canister --argument "
  (opt record {
    chain = 32;
    min_confirmations = 42;
    ecdsa_key_name = \"dfx_test_key\";
    prev_start_height = 0;
    prev_start_blockhash = \"\";
  })
"

# set RPC agent
dfx canister call ck-doge-canister admin_set_agent '
  (record {
    name = "ICPanda";
    endpoint = "https://doge-test-rpc.panda.fans/URL_DOGE_TEST";
    max_cycles = 10000000000;
    proxy_token = null;
    api_token = opt "HEADER_API_TOKEN"
  })
'

# start sync jobs to sync Dogecoin blocks and process transactions
dfx canister call ck-doge-canister admin_restart_syncing '(false)'
```

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).

`ldclabs/ck-doge` is licensed under the MIT License. See [LICENSE](LICENSE-MIT) for the full license text.