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
in `output` directory. If you prover to analyze a single opcode run 
the following.
```
% heaptrack ./verifer [OPCODE_NAME]
```
Once the `heaptrack` report is generated you can view it using `heaptrack_gui`. 

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
