use std::{
    cmp::min,
    io::{BufRead, Cursor},
    process::Command,
};

use darkfi_sdk::num_traits::{Num, Zero};
use num_bigint::BigUint;

use crate::{next_difficulty, DIFFICULTY_LAG, DIFFICULTY_WINDOW};

const DEFAULT_TEST_DIFFICULTY_TARGET: usize = 120;

#[test]
fn test_wide_difficulty() {
    let mut timestamps: Vec<u64> = vec![];
    let mut cummulative_difficulties: Vec<BigUint> = vec![];
    let mut cummulative_difficulty = BigUint::zero();

    let output = Command::new("./gen_wide_data.py").output().unwrap();
    let reader = Cursor::new(output.stdout);

    for (n, line) in reader.lines().enumerate() {
        let line = line.unwrap();
        let parts: Vec<String> = line.split(' ').map(|x| x.to_string()).collect();
        assert!(parts.len() == 2);

        let timestamp = parts[0].parse::<u64>().unwrap();
        let difficulty = BigUint::from_str_radix(&parts[1], 10).unwrap();

        let begin: usize;
        let end: usize;
        if n < DIFFICULTY_WINDOW + DIFFICULTY_LAG {
            begin = 0;
            end = min(n, DIFFICULTY_WINDOW);
        } else {
            end = n - DIFFICULTY_LAG;
            begin = end - DIFFICULTY_WINDOW;
        }

        let timestamps_cut = timestamps[begin..end].to_vec();
        let difficulty_cut = cummulative_difficulties[begin..end].to_vec();
        let res = next_difficulty(timestamps_cut, difficulty_cut, DEFAULT_TEST_DIFFICULTY_TARGET);

        if res != difficulty {
            eprintln!("Wrong wide difficulty for block {}", n);
            eprintln!("Expected: {}", difficulty);
            eprintln!("Found: {}", res);
            assert!(res == difficulty);
        }

        timestamps.push(timestamp);
        cummulative_difficulty += difficulty;
        cummulative_difficulties.push(cummulative_difficulty.clone());
    }
}
