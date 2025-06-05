/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 * Copyright (C) 2014 Thomas Voegtlin (MIT License)
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, Result};
use hmac::{Hmac, Mac};
use num_bigint::{BigUint, RandBigInt};
use num_traits::identities::{One, Zero};
use pasta_curves::{group::ff::FromUniformBytes, Fp};
use pbkdf2::pbkdf2_hmac;
use rand::thread_rng;
use sha2::Sha512;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

#[repr(u8)]
#[derive(Copy, Clone)]
enum SeedPrefix {
    Standard = 0x01,
}

impl FromStr for SeedPrefix {
    type Err = io::Error;

    fn from_str(seed_type: &str) -> io::Result<Self> {
        match seed_type {
            "standard" => Ok(Self::Standard),
            _ => Err(io::Error::other(format!("Unsupported seed type {}", seed_type))),
        }
    }
}

#[derive(Debug)]
struct Wordlist {
    index_from_word: HashMap<String, u64>,
    word_from_index: HashMap<u64, String>,
}

impl Wordlist {
    fn new(words: Vec<String>) -> Self {
        let mut index_from_word = HashMap::new();
        let mut word_from_index = HashMap::new();
        for (i, word) in words.iter().enumerate() {
            index_from_word.insert(word.clone(), i as u64);
            word_from_index.insert(i as u64, word.clone());
        }

        Self { index_from_word, word_from_index }
    }

    fn index_from_word(&self, word: &str) -> Option<u64> {
        self.index_from_word.get(word).copied()
    }

    fn len(&self) -> usize {
        self.index_from_word.len()
    }

    fn from_file(filename: &Path) -> Result<Self> {
        let s = fs::read_to_string(filename)?;
        let s = s.trim();
        let s: String = s.nfkd().collect();
        let lines = s.split('\n');
        let mut words = vec![];

        for line in lines {
            let line = line.split('#').next().unwrap_or("");
            let line = line.trim_matches(&[' ', '\r'][..]);
            assert!(!line.contains(' '));
            if !line.is_empty() {
                words.push(line.to_string());
            }
        }

        Ok(Self::new(words))
    }
}

impl std::ops::Index<u64> for Wordlist {
    type Output = String;

    fn index(&self, index: u64) -> &Self::Output {
        self.word_from_index.get(&index).unwrap()
    }
}

#[derive(Debug)]
struct Mnemonic {
    wordlist: Wordlist,
}

impl Mnemonic {
    fn new(lang: &str) -> Result<Self> {
        let path = match lang {
            "en" => PathBuf::from("wordlist/english.txt"),
            "es" => PathBuf::from("wordlist/spanish.txt"),
            "ja" => PathBuf::from("wordlist/japanese.txt"),
            "pt" => PathBuf::from("wordlist/portuguese.txt"),
            "zh" => PathBuf::from("wordlist/chinese_simplified.txt"),
            _ => unimplemented!(),
        };

        let wordlist = Wordlist::from_file(&path)?;
        Ok(Self { wordlist })
    }

    fn mnemonic_to_seed(mnemonic: &str, passphrase: Option<&str>) -> [u8; 64] {
        const PBKDF_ROUNDS: u32 = 2048;
        let mnemonic = normalize_text(mnemonic);
        let passphrase = normalize_text(passphrase.unwrap_or(""));

        let mut salt = String::from("darkfi");
        salt.push_str(&passphrase);

        let mut key = [0u8; 64];
        pbkdf2_hmac::<Sha512>(mnemonic.as_bytes(), salt.as_bytes(), PBKDF_ROUNDS, &mut key);
        key
    }

    fn mnemonic_encode(&self, i: &BigUint) -> String {
        let n = BigUint::from(self.wordlist.len());
        let mut words = vec![];
        let mut i = i.clone();
        while i > BigUint::zero() {
            let x = &i % &n;
            i /= &n;
            words.push(self.wordlist[x.try_into().unwrap()].clone())
        }

        words.join(" ")
    }

    fn mnemonic_decode(&self, seed: &str) -> Result<BigUint> {
        let n = BigUint::from(self.wordlist.len());
        let mut words: Vec<&str> = seed.split_whitespace().collect();
        let mut i = BigUint::zero();
        while let Some(w) = words.pop() {
            let Some(k) = self.wordlist.index_from_word(w) else {
                return Err(anyhow!("Invalid word in mnemonic: {}", w));
            };
            i = &i * &n + k;
        }

        Ok(i)
    }

    fn make_seed(&self, seed_type: Option<&str>, num_bits: Option<usize>) -> Result<String> {
        let num_bits = num_bits.unwrap_or(232);
        let prefix = SeedPrefix::from_str(seed_type.unwrap_or("standard"))?;

        // Increase num_bits in order to obtain a uniform distribution for the last word
        let bpw = (self.wordlist.len() as f64).log2();
        let adj_num_bits = ((num_bits as f64 / bpw).ceil() * bpw) as u32;
        println!("make_seed: prefix={}, entropy={adj_num_bits} bits", prefix as u8);

        let threshold_exp = (num_bits as f64 - bpw) as u32;
        let threshold = BigUint::from(2u32).pow(threshold_exp);
        let max_entropy = BigUint::from(2u32).pow(adj_num_bits);

        // Generate random
        let mut rng = thread_rng();
        let mut entropy = BigUint::one();
        while entropy < threshold {
            entropy = rng.gen_biguint_below(&max_entropy);
        }

        // Brute-force seed that has correct "version number"
        let mut nonce = BigUint::zero();
        let mut seed;
        loop {
            nonce += 1u32;
            let i = &entropy + &nonce;

            seed = self.mnemonic_encode(&i);
            if i != self.mnemonic_decode(&seed)? {
                return Err(anyhow!("Cannot extract same entropy from mnemonic!"))
            }
            // Make sure the mnemonic we generate is not also a valid BIP39 seed
            // by accident.
            /*
            if bip39_is_checksum_valid(seed, &self.wordlist) == (true, true) {
                continue
            }
            */
            if is_new_seed(&seed, prefix) {
                break
            }
        }

        let num_words = seed.split_whitespace().collect::<Vec<&str>>().len();
        println!("{num_words} words");
        Ok(seed)
    }
}

fn hmac_oneshot(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha512>::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

fn is_new_seed(seed: &str, prefix: SeedPrefix) -> bool {
    let seed = normalize_text(seed);
    let seed = hmac_oneshot("Seed version".as_bytes(), seed.as_bytes());
    seed[0] == prefix as u8
}

const CJK_INTERVALS: &[(u32, u32, &str)] = &[
    (0x4E00, 0x9FFF, "CJK Unified Ideographs"),
    (0x3400, 0x4DBF, "CJK Unified Ideographs Extension A"),
    (0x20000, 0x2A6DF, "CJK Unified Ideographs Extension B"),
    (0x2A700, 0x2B73F, "CJK Unified Ideographs Extension C"),
    (0x2B740, 0x2B81F, "CJK Unified Ideographs Extension D"),
    (0xF900, 0xFAFF, "CJK Compatibility Ideographs"),
    (0x2F800, 0x2FA1D, "CJK Compatibility Ideographs Supplement"),
    (0x3190, 0x319F, "Kanbun"),
    (0x2E80, 0x2EFF, "CJK Radicals Supplement"),
    (0x2F00, 0x2FDF, "CJK Radicals"),
    (0x31C0, 0x31EF, "CJK Strokes"),
    (0x2FF0, 0x2FFF, "Ideographic Description Characters"),
    (0xE0100, 0xE01EF, "Variation Selectors Supplement"),
    (0x3100, 0x312F, "Bopomofo"),
    (0x31A0, 0x31BF, "Bopomofo Extended"),
    (0xFF00, 0xFFEF, "Halfwidth and Fullwidth Forms"),
    (0x3040, 0x309F, "Hiragana"),
    (0x30A0, 0x30FF, "Katakana"),
    (0x31F0, 0x31FF, "Katakana Phonetic Extensions"),
    (0x1B000, 0x1B0FF, "Kana Supplement"),
    (0xAC00, 0xD7AF, "Hangul Syllables"),
    (0x1100, 0x11FF, "Hangul Jamo"),
    (0xA960, 0xA97F, "Hangul Jamo Extended A"),
    (0xD7B0, 0xD7FF, "Hangul Jamo Extended B"),
    (0x3130, 0x318F, "Hangul Compatibility Jamo"),
    (0xA4D0, 0xA4FF, "Lisu"),
    (0x16F00, 0x16F9F, "Miao"),
    (0xA000, 0xA48F, "Yi Syllables"),
    (0xA490, 0xA4CF, "Yi Radicals"),
];

fn is_cjk(c: char) -> bool {
    let n = c as u32;
    for (imin, imax, _name) in CJK_INTERVALS {
        if imin <= &n && &n <= imax {
            return true
        }
    }
    false
}

fn normalize_text(seed: &str) -> String {
    // Normalize
    let seed: String = seed.nfkd().collect();
    // Lower
    let seed = seed.to_lowercase();
    // Remove accents
    let seed: String = seed.chars().filter(|&c| !is_combining_mark(c)).collect();
    // Normalize whitespaces
    let seed: String = seed.split_whitespace().collect::<Vec<&str>>().join(" ");
    // Remove whitespaces between CJK
    let chars: Vec<char> = seed.chars().collect();
    let seed: String = chars
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| {
            if c.is_whitespace() && i > 0 && i < chars.len() - 1 {
                if is_cjk(chars[i - 1]) && is_cjk(chars[i + 1]) {
                    None
                } else {
                    Some(c)
                }
            } else {
                Some(c)
            }
        })
        .collect();

    seed
}

fn main() -> Result<()> {
    for lang in ["en", "es", "ja", "pt", "zh"] {
        let mnemonic = Mnemonic::new(lang)?;
        let seed = mnemonic.make_seed(None, None)?;
        let decoded = mnemonic.mnemonic_decode(&seed)?;
        println!("{seed}\n{decoded}");
        assert_eq!(mnemonic.mnemonic_encode(&decoded), seed);
        assert!(decoded > BigUint::from(u64::MAX));

        // Obtain secret key from seed
        let key = Mnemonic::mnemonic_to_seed(&seed, None);
        Fp::from_uniform_bytes(&key);
    }

    Ok(())
}
