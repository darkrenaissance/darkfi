/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use tfhe::{
    boolean::prelude::{gen_keys as boolean_gen_keys, *},
    integer::gen_keys_radix,
    shortint::prelude::{gen_keys as shortint_gen_keys, *},
};

fn main() {
    // ===============
    // Boolean circuit
    // ===============
    // Generate a set of client/server keys, using the default parameters.
    // The client generates both keys. The server key is meant to be published
    // so that homomorphic circuits can be computed.
    let (client_key, server_key) = boolean_gen_keys();

    // Encrypt two messages using the (private) client key:
    let msg1 = true;
    let msg2 = false;
    let ct_1 = client_key.encrypt(msg1);
    let ct_2 = client_key.encrypt(msg2);

    // We use the server public key to execute a boolean circuit:
    // if ((NOT ct_2) NAND (ct_1 AND ct_2)) then (NOT ct_2) else (ct_1 AND ct_2)
    let ct_3 = server_key.not(&ct_2);
    let ct_4 = server_key.and(&ct_1, &ct_2);
    let ct_5 = server_key.nand(&ct_3, &ct_4);
    let ct_6 = server_key.mux(&ct_5, &ct_3, &ct_4);

    // We use the client key to decrypt the output of the circuit
    let output = client_key.decrypt(&ct_6);
    assert!(output);

    // ================
    // Shortint circuit
    // ================
    // Generate a set of client/server keys
    // with 2 bits of message and 2 bits of carry
    let (client_key, server_key) = shortint_gen_keys(PARAM_MESSAGE_2_CARRY_2);

    let msg1 = 3;
    let msg2 = 2;

    // Encrypt two messages using the (private) client key:
    let ct_1 = client_key.encrypt(msg1);
    let ct_2 = client_key.encrypt(msg2);

    // Homomorphically compute an addition
    let ct_add = server_key.unchecked_add(&ct_1, &ct_2);

    // Define the Hamming weight function
    // f: x -> sum of the bits of x
    let f = |x: u64| x.count_ones() as u64;

    // Generate the accumulator for the function
    let acc = server_key.generate_accumulator(f);

    // Compute the function over the ciphertext using the PBS
    let ct_res = server_key.apply_lookup_table(&ct_add, &acc);

    // Decrypt the ciphertext using the (private) client key
    let output = client_key.decrypt(&ct_res);
    assert_eq!(output, f(msg1 + msg2));

    // ===============
    // Integer circuit
    // ===============
    // We create keys to create 16 bits integers
    // using 8 blocks of 2 bits
    let (cks, sks) = gen_keys_radix(&PARAM_MESSAGE_2_CARRY_2, 8);

    let clear_a = 2382u16;
    let clear_b = 29374u16;

    let mut a = cks.encrypt(clear_a as u64);
    let mut b = cks.encrypt(clear_b as u64);

    let encrypted_max = sks.smart_max_parallelized(&mut a, &mut b);
    let decrypted_max: u64 = cks.decrypt(&encrypted_max);

    assert_eq!(decrypted_max as u16, clear_a.max(clear_b))
}
