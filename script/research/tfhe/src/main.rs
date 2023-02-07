use tfhe::shortint::{gen_keys, Parameters};

fn main() {
    // Generate a set of client/server keys, using the default parameters.
    // The client generates both keys. The server key is meant to be published
    // so that homomorphic circuits can be computed.
    let (client_key, server_key) = gen_keys(Parameters::default());

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
    let ct_res = server_key.keyswitch_programmable_bootstrap(&ct_add, &acc);

    // Decrypt the ciphertext using the (private) client key
    let output = client_key.decrypt(&ct_res);
    assert_eq!(output, f(msg1 + msg2));
    println!("{:#b}", msg1 + msg2);
}
