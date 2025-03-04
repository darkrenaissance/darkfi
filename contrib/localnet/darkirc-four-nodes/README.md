darkirc Four Nodes Local Testing
================================

A testing scenario which starts four local darkirc clients in a `tmux`
session, and optionally connects `weechat` clients for manual testing.

To merely start the four local darkirc nodes under `tmux`:
``` 
% ./tmux-sessions.sh 
```

To interact with the `weechat` IRC client is installed and in your
PATH:

```
% ./tmux-sessions.sh -a 
``` 

There will be four nodes with chacha identities keyed under 'node1',
'node2', 'node3' and 'node4', with the respective nicks 'Alice',
'Bob', 'Charlie' and 'Dave'.

Each client will join #dev, an unecrypted (public) channel.

Each client will join #test, an encrypted (private) channel

## Testing Scenarios

| # | Description                                                     | Test                                                                                                         | Status |
|---|-----------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------|--------|
| 0 | Normal Messages                                                 | Alice sends a message in #dev; others receive message                                                        | Pass   |
| 1 | Encrypted Channel                                               | Alice sends a message in #test; others receive message                                                       | Pass   |
| 2 | DM                                                              | Alice sends DM to node2; Bob receives it                                                                     | Pass   |
| 3 | No Unpaired DM                                                  | Alice sends a DM to node3; Charlie fails to receive anything                                                 | Pass   |
| 4 | Self-DM                                                         | Alice sends a DM to node1; Alice receives it                                                                 | Pass   |
| 5 | Disconnected Normal Message                                     | Stop Charlie's darkirc; send a message to #dev; restart Charlie's darkirc ; observe Charlie receives message | Pass   |
| 6 | Disconnected Encrypted Channel                                  | Stop Charlie's darkirc; send a message to #test; restart Charlie's darkirc; observe Charlie receives message | Pass   |
| 7 | Disconnected DM                                                 | Stop Bob's darkirc; have Alice send a message to node2; restart Bob's darkirc; observe Bob receives message  | Pass   |
