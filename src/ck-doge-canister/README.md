# CK-Doge: `ck-doge-canister`
Interact with Dogecoin network from the [Internet Computer](https://internetcomputer.org/).

## Running the project locally

If you want to test your project locally, you can use the following commands:

```bash
# start the replica
dfx start

# deploy the canister
dfx deploy ck-doge-canister --argument "(opt variant {Init =
  record {
    chain = 32;
    min_confirmations = 42;
    ecdsa_key_name = \"dfx_test_key\";
    prev_start_height = 6265990;
    prev_start_blockhash = \"8a2da40730d43d9ef53f85b1143ae7362294add9d92cc7660a6483b75a75527c\";
  }
})"

dfx canister call ck-doge-canister get_state '()'

# upgrade
dfx deploy ck-doge-canister --argument "(opt variant {Upgrade =
  record {
    min_confirmations = opt 6;
  }
})"

dfx canister call ck-doge-canister get_state '()'

# set RPC agent
dfx canister call ck-doge-canister admin_set_agent '
  (vec {
    record {
      name = "ICPanda";
      endpoint = "https://doge-test-rpc.panda.fans/URL_DOGE_TEST";
      max_cycles = 10000000000;
      proxy_token = null;
      api_token = opt "HEADER_API_TOKEN"
    }; record {
      name = "ICPanda";
      endpoint = "https://doge-test-rpc.panda.fans/URL_DOGE_TEST";
      max_cycles = 10000000000;
      proxy_token = null;
      api_token = opt "HEADER_API_TOKEN"
    }
  })
'

dfx canister call ck-doge-canister get_state '()'

# start sync jobs to sync Dogecoin blocks and process transactions
dfx canister call ck-doge-canister admin_restart_syncing '(null)'

dfx canister call ck-doge-canister get_tip '()'

dfx canister call ck-doge-canister get_address '()'

dfx canister call ck-doge-canister get_balance '("nYmJMro1rtZvHWm5a4WxTE77bGYtRYrfao")'

dfx canister call ck-doge-canister list_utxos '("nYmJMro1rtZvHWm5a4WxTE77bGYtRYrfao", 100, false)'

dfx canister call ck-doge-canister create_tx '(record {
  address = "nae7FWZjATd91o5nutvQXToQZFrXjWV1wb";
  amount = 100000000;
  fee_rate = 1000;
  utxos = vec {};
})'
# tx: .....

dfx canister call ck-doge-canister send_tx '(record {
  tx = blob "\01\00\00\00\02\27\76\b1\60\77\7e\98\ce\a7\7a\d6\70\5d\04\91\19\64\88\25\40\aa\de\ec\65\46\7f\5e\61\a4\38\ce\e8\00\00\00\00\6a\47\30\44\02\20\12\cf\82\29\7f\48\28\7a\cb\05\c7\32\2a\17\52\21\3c\e9\2b\b3\a1\45\1f\3b\6f\88\25\2f\cf\6e\fa\7d\02\20\62\5d\5c\3e\d2\1a\b8\15\3a\4c\a6\8c\4a\7b\86\44\0e\8d\ec\db\6e\7c\43\34\52\de\7e\a0\83\66\55\54\01\21\02\7a\1c\df\0e\b9\c2\a8\c2\a0\b5\7f\2a\62\a4\e7\1d\58\b6\10\a8\39\22\fc\c8\81\39\0f\74\a0\1d\3b\0e\ff\ff\ff\ff\98\7a\d2\55\e0\29\28\0e\1c\f6\dc\f5\aa\21\fe\bc\0f\8e\d6\02\08\b8\03\56\2e\db\b4\7e\e4\79\62\ba\00\00\00\00\6b\48\30\45\02\21\00\c5\eb\46\fd\32\b3\43\d9\5a\21\0d\ca\f5\1e\a5\9a\25\43\6d\57\25\a6\7e\70\11\d3\90\40\a9\9d\5c\90\02\20\2d\ff\b5\44\51\79\63\47\68\d5\2a\01\7f\fc\46\62\51\94\ac\0b\d8\30\a0\8d\98\71\b2\30\49\20\82\bc\01\21\02\7a\1c\df\0e\b9\c2\a8\c2\a0\b5\7f\2a\62\a4\e7\1d\58\b6\10\a8\39\22\fc\c8\81\39\0f\74\a0\1d\3b\0e\ff\ff\ff\ff\02\00\e1\f5\05\00\00\00\00\19\76\a9\14\46\b9\0d\f2\e8\66\35\0f\69\47\af\9d\b3\2d\4a\70\9e\e4\71\3d\88\ac\c0\70\30\71\00\00\00\00\19\76\a9\14\46\b9\0d\f2\e8\66\35\0f\69\47\af\9d\b3\2d\4a\70\9e\e4\71\3d\88\ac\00\00\00\00";
})'
# https://sochain.com/tx/DOGETEST/b229a483bf55a0bcd26763368c03304981ed32afb8f54be0ab635895d86d7d39
```

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).

`ldclabs/ck-doge` is licensed under the MIT License. See [LICENSE](LICENSE-MIT) for the full license text.