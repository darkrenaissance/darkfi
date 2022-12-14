# Debugging

As a final step, let's quickly turn to the debug output of `dnetview`
which is stored in `.local/darkfi/dnetview.log`.

Run `dnetview` in `verbose` mode to enable debugging.

```bash
./dnetview -v
```

Here's an example output. This is Alice:

```json
[DEBUG] (16) jsonrpc-client: <-- {"jsonrpc":"2.0","id":8105306807249776489,"result":{"external_addr":"tcp://127.0.0.1:51554","session_inbound":{"connected":{"tcp://127.0.0.1:36428":[{"accept_addr":"tcp://127.0.0.1:51554"},{"last_msg":"addr","last_status":"recv","log":[[1659950874808537094,"send","version"],[1659950874810919251,"recv","version"],[1659950874811104471,"send","verack"],[1659950874811491950,"recv","verack"],[1659950874812397628,"send","getaddr"],[1659950874814847748,"recv","getaddr"],[1659950874815100189,"send","addr"],[1659950874816306644,"recv","addr"]],"random_id":2658393884,"remote_node_id":""}]}},"session_manual":{"key":110},"session_outbound":{"slots":[]},"state":"run"}}
```

This is Bob: 

```json
[DEBUG] (16) jsonrpc-client: <-- {"jsonrpc":"2.0","id":17000304364801751931,"result":{"external_addr":null,"session_inbound":{"connected":{}},"session_manual":{"key":110},"session_outbound":{"slots":[{"addr":null,"channel":null,"state":"open"},{"addr":null,"channel":null,"state":"open"},{"addr":"tcp://127.0.0.1:51554","channel":{"last_msg":"addr","last_status":"sent","log":[],"random_id":3924275147,"remote_node_id":""},"state":"connected"},{"addr":null,"channel":null,"state":"open"},{"addr":"tcp://127.0.0.1:50515","channel":{"last_msg":"addr","last_status":"sent","log":[],"random_id":2182348290,"remote_node_id":""},"state":"connected"}]},"state":"run"}}
```

The raw data might come in useful in some cases.

Happy hacking!
