# ZkVM Metering tool
- The aim of this tool is to analyze resource usage of ZkVM opcodes
using [heaptrack](https://github.com/KDE/heaptrack).
- We have a proof generator and verifier to profile each ZkVM Opcode.
- The `generator` generates a proof, verifying key and public inputs
for all the proofs stored in `generator/proof`.
- Each Zk script file in `generator/proof` contains a single ZkVM
opcode.
- The `generator` saves it's outputs to disk for later use by the
`verifier`.
- The `verifier` loads the proof, verifying key and public inputs and
verifies a single proof at a time identified by the opcode name.

#### Steps to profile a ZkVM Opcode

To generate the proofs go to `generator` directory and run these
commands.
```
% make
% ./generator
```
To verify the proof and profile all the opcodes go to `verifier` and
run these commands. You need to install `heaptrack` before running the
second command. 
```
% make
% ./run_heaptrack.sh
```
`run_heaptrack.sh` will generate `heaptrack` report for all the opcodes
in `output` directory. If you want to analyze a single opcode, run
the following.
```
% heaptrack ./verifer [OPCODE_NAME]
```
Once the `heaptrack` report is generated you can view it using `heaptrack_gui`. 

**Note:** This tool can also be used to generate and verify any ZkVM proof.

#### Analysis Results

| #  | Opcode                | RAM Usage | Verifying Key Size | Proof Size |
|----|-----------------------|-----------|--------------------|------------|
| 0  | sparse_merkle_root    | 17.1 MB   | 1.2 MB             | 7.7 kB     |
| 1  | merkle_root           | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 2  | posedion_hash         | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 3  | base_add              | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 4  | base_mul              | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 5  | base_sub              | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 6  | ec_add                | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 7  | ec_mul                | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 8  | ec_mul_base           | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 9  | ec_mul_short          | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 10 | ec_mul_var_base       | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 11 | ec_get_x              | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 12 | ec_get_y              | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 13 | constrain_instance    | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 14 | constrain_equal_base  | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 15 | constrain_equal_point | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 16 | bool_check            | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 17 | cond_select           | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 18 | zero_cond             | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 19 | less_than_strict      | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 20 | less_than_loose       | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 21 | range_check           | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 22 | witness_base          | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 23 | debug                 | 2.4 MB    | 145.6 kB           | 7.6 kB     |

#### Native Contracts' Proofs
| #  | Proof                            | RAM Usage | Verifying Key Size | Proof Size |
|----|----------------------------------|-----------|--------------------|------------|
| 0  | money_mint_v1                    | 2.5 MB    | 145.6 kB           | 7.6 kB     |
| 1  | money_burn_v1                    | 2.5 MB    | 145.6 kB           | 7.6 kB     |
| 2  | money_fee_v1                     | 2.5 MB    | 145.6 kB           | 7.6 kB     |
| 3  | money_token_mint_v1              | 2.5 MB    | 145.6 kB           | 7.6 kB     |
| 4  | money_auth_token_mint_v1         | 2.4 MB    | 145.6 kB           | 7.6 kB     |
| 5  | dao_mint                         | 2.5 MB    | 145.6 kB           | 7.6 kB     |
| 6  | dao_propose_input                | 17.2 MB   | 1.2 MB             | 7.7 kB     |
| 7  | dao_propose_main                 | 2.6 MB    | 145.6 kB           | 7.6 kB     |
| 8  | dao_vote_input                   | 17.2 MB   | 1.2 MB             | 7.7 kB     |
| 9  | dao_vote_main                    | 2.6 MB    | 145.6 kB           | 7.6 kB     |
| 10 | dao_exec                         | 2.6 MB    | 145.6 kB           | 7.6 kB     |
| 11 | dao_early_exec                   | 2.6 MB    | 145.6 kB           | 7.6 kB     |
| 12 | dao_auth_money_transfer          | 2.6 MB    | 145.6 kB           | 7.6 kB     |
| 13 | dao_auth_money_transfer_enc_coin | 2.5 MB    | 145.6 kB           | 7.6 kB     |


#### Circuit Complexity Metrics
Circuit metrics highlighting the relative computational and memory
demands of each operation.

| Proof                            | k  | max_deg | advice_columns | instance_queries | advice_queries | fixed_queries | lookups | permutation_cols | point_sets | max_rows | max_advice_rows | max_fixed_rows | num_fixed_columns | num_advice_columns | num_instance_columns | num_total_columns |
|----------------------------------|----|---------|----------------|------------------|----------------|---------------|---------|------------------|------------|----------|-----------------|----------------|-------------------|--------------------|----------------------|-------------------|
| sparse_merkle_root               | 14 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 10455    | 10455           | 9434           | 35                | 12                 | 1                    | 48                |
| merkle_root                      | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1088     | 1088            | 1024           | 35                | 12                 | 1                    | 48                |
| poseidon_hash                    | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 41              | 1024           | 35                | 12                 | 1                    | 48                |
| base_add                         | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 3               | 1024           | 35                | 12                 | 1                    | 48                |
| base_mul                         | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 3               | 1024           | 35                | 12                 | 1                    | 48                |
| base_sub                         | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 3               | 1024           | 35                | 12                 | 1                    | 48                |
| ec_add                           | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 5               | 1024           | 35                | 12                 | 1                    | 48                |
| ec_mul                           | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 88              | 1024           | 35                | 12                 | 1                    | 48                |
| ec_mul_base                      | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 90              | 1024           | 35                | 12                 | 1                    | 48                |
| ec_mul_short                     | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 27              | 1024           | 35                | 12                 | 1                    | 48                |
| ec_mul_var_base                  | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 151             | 1024           | 35                | 12                 | 1                    | 48                |
| ec_get_x                         | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 2               | 1024           | 35                | 12                 | 1                    | 48                |
| ec_get_y                         | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 2               | 1024           | 35                | 12                 | 1                    | 48                |
| constrain_instance               | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 2               | 1024           | 35                | 12                 | 1                    | 48                |
| constrain_equal_base             | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 3               | 1024           | 35                | 12                 | 1                    | 48                |
| constrain_equal_point            | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 3               | 1024           | 35                | 12                 | 1                    | 48                |
| bool_check                       | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 2               | 1024           | 35                | 12                 | 1                    | 48                |
| cond_select                      | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 4               | 1024           | 35                | 12                 | 1                    | 48                |
| zero_cond                        | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 3               | 1024           | 35                | 12                 | 1                    | 48                |
| less_than_strict                 | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 29              | 1024           | 35                | 12                 | 1                    | 48                |
| less_than_loose                  | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 29              | 1024           | 35                | 12                 | 1                    | 48                |
| range_check                      | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 10              | 1024           | 35                | 12                 | 1                    | 48                |
| witness_base                     | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 2               | 1024           | 35                | 12                 | 1                    | 48                |
| debug                            | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 2               | 1024           | 35                | 12                 | 1                    | 48                |


| Proof                            | k  | max_deg | advice_columns | instance_queries | advice_queries | fixed_queries | lookups | permutation_cols | point_sets | max_rows | max_advice_rows | max_fixed_rows | num_fixed_columns | num_advice_columns | num_instance_columns | num_total_columns |
|----------------------------------|----|---------|----------------|------------------|----------------|---------------|---------|------------------|------------|----------|-----------------|----------------|-------------------|--------------------|----------------------|-------------------|
| money_mint_v1                    | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 302             | 1024           | 35                | 12                 | 1                    | 48                |
| money_burn_v1                    | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1448     | 1448            | 1431           | 35                | 12                 | 1                    | 48                |
| money_fee_v1                     | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1713     | 1713            | 1696           | 35                | 12                 | 1                    | 48                |
| money_token_mint_v1              | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 242             | 1024           | 35                | 12                 | 1                    | 48                |
| money_auth_token_mint_v1         | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 202             | 1024           | 35                | 12                 | 1                    | 48                |
| dao_mint                         | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 901             | 1024           | 35                | 12                 | 1                    | 48                |
| dao_propose_input                | 14 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 11658    | 11658           | 11328          | 35                | 12                 | 1                    | 48                |
| dao_propose_main                 | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1619     | 1619            | 1602           | 35                | 12                 | 1                    | 48                |
| dao_vote_input                   | 14 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 11739    | 11739           | 11408          | 35                | 12                 | 1                    | 48                |
| dao_vote_main                    | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1298     | 1298            | 1297           | 35                | 12                 | 1                    | 48                |
| dao_exec                         | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 888             | 1024           | 35                | 12                 | 1                    | 48                |
| dao_early_exec                   | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 976             | 1024           | 35                | 12                 | 1                    | 48                |
| dao_auth_money_transfer          | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1182     | 1182            | 1181           | 35                | 12                 | 1                    | 48                |
| dao_auth_money_transfer_enc_coin | 11 | 9       | 12             | 1                | 31             | 35            | 11      | 17               | 5          | 1024     | 735             | 1024           | 35                | 12                 | 1                    | 48                |
