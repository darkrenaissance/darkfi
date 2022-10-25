# Tables vs Key-Value

Every SQL style DB can be transformed to a key-value database.

Imagine you have a table with 2 indexes like so:

| row_index   | first_name   | last_name   | ...   |
|-------------|--------------|-------------|-------|
| 46          | john         | doe         | ...   |
| ...         |              |             |       |

This can be represented by two key-value databases.

```python
objects = {
    46: (...),
    ...
}

indexes = {
    "john": ("first_name", 46),
    "doe": ("last_name", 46)
}
```

This simple trick means we can translate any table layout easily
to a key-value format.

Indeed both key-value databases above can actually be combined together
but then that means the keys for object would allow string types. Having
just integer indexes that are sequential allows for fast optimization of
object lookups.

Also object types can be combined inside the same kv-db since the value
field of the db is just bytes that are deserialized.

# Example 1

```rust
struct State {
    foos: Vec<Foo>
}

struct Foo {
    name: String,
    bars: Vec<Bar>
}

struct Bar {
    x: u32
}

...

let state = State {
    foos: vec![
        Foo {
            name: "john doe",
            bars: vec![
                Bar { x: 110 },
                Bar { x: 4 },
            ]
        },
        Foo {
            name: "alison bob",
            bars: vec![
            ]
        },
    ]
};
```

Table containing Foo objects:

| foo_index   | foo_name   |
|-------------|------------|
| 1           | john doe   |
| 2           | alison bob |
| ...         |            |

This table links Bar objects to Foo:

| foo_index   | bar_index   |
|-------------|-------------|
| 1           | 73          |
| 1           | 74          |
| ...         |             |

Table containing Bar objects:

| bar_index   | bar_x   |
|-------------|---------|
| 73          | 110     |
| 74          | 4       |
| ...         |         |

# Example 2: DAO State

```rust
pub struct ProposalVotes {
    // TODO: might be more logical to have 'yes_votes_commit' and 'no_votes_commit'
    /// Weighted vote commit
    pub yes_votes_commit: pallas::Point,
    /// All value staked in the vote
    pub all_votes_commit: pallas::Point,
    /// Vote nullifiers
    pub vote_nulls: Vec<Nullifier>,
}

/// This DAO state is for all DAOs on the network. There should only be a single instance.
pub struct State {
    dao_bullas: Vec<DaoBulla>,
    pub dao_tree: MerkleTree,
    pub dao_roots: Vec<MerkleNode>,

    //proposal_bullas: Vec<pallas::Base>,
    pub proposal_tree: MerkleTree,
    pub proposal_roots: Vec<MerkleNode>,
    pub proposal_votes: HashMap<HashableBase, ProposalVotes>,
}
```

The important thing to note is that we are not restricted to using simple key-value databases.

We can also combine them with optimized databases such as merkle-tree dbs.

The main DAO state is represented by a table with a single row.

|   dao_tree_index |   proposal_tree_index |
|------------------|-----------------------|
|              301 |                   406 |

301 and 406 here refer to instances of separate merkle tree dbs.

| dao_bulla                                  |
|--------------------------------------------|
| 0xabea9132b05a70803a4e85094fd0e1800777fbef |
| 0x7c4de4aa5068376033aef8e3df766aff3080e045 |

| dao_roots                                  |
|--------------------------------------------|
| 0xd6dfd811e06267b25472753c4e57c0b28652bfb8 |
| 0x5f78fbab81f9892bbe379d88c8a224774411b0a9 |

| proposal_roots                             |
|--------------------------------------------|
| 0x1430118732f564ec474c4998d94521661143df23 |
| 0x87611ca3403a3878dfef0da2a786e209abfc1eff |

These tables keeping track of the roots, could even be part of the merkle databases themselves.

|   proposal_votes_index | yes_votes_commit   | all_votes_commit   |
|------------------------|--------------------|--------------------|
|                     72 | xxx                | yyy                |

|   proposal_votes_index | nullifier   |
|------------------------|-------------|
|                     72 | aaa         |
|                     72 | bbb         |
|                     72 | ccc         |

And lastly we create the Base -> ProposalVotes index.

| base                                                               |   proposal_votes_index |
|--------------------------------------------------------------------|------------------------|
| 0xa20bfb25ab13a77cc9b50aec28a0b826cee20f88892d087ec1cbc1cbda635d6e |                     72 |

