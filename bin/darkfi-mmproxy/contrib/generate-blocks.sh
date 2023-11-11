#!/bin/sh
set -e

DATA='
    {
      "jsonrpc": "2.0",
      "id": 0,
      "method": "generateblocks",
      "params": {
        "amount_of_blocks": 2000000,
        "wallet_address": "44AFFq5kSiGBoZ4NMDwYtN18obc8AemS33DBLWs3H7otXft3XjrpDtQGv7SqSsaBYBb98uNbr2VBBEt7f2wfn3RVGQBEP3A",
        "starting_nonce": 0
      }
    }
'

curl "http://127.0.0.1:18081/json_rpc" -d "$DATA" \
	-H 'Content-Type: application/json' &

cat <<EOF
Now monerod should be generating the blocks.
You can periodically type 'print_height' in the monerod console to
see the progress. At 2M blocks, RandomX activates and the proxy is
usable.
EOF
