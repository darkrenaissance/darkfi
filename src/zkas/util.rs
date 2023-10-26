use darkfi_sdk::pasta::pallas;
use std::{collections::HashMap, fs::File, io::Write, path::Path};
use tinyjson::JsonValue::{Array as JsonArray, Object as JsonObj, String as JsonStr};

use crate::zk::Witness;

/// Export witness.json which can be used by zkrunner for debugging circuits
pub fn export_witness_json<P: AsRef<Path>>(
    output_path: P,
    prover_witnesses: &Vec<Witness>,
    public_inputs: &Vec<pallas::Base>,
) {
    let mut witnesses = Vec::new();
    for witness in prover_witnesses {
        let mut value_json = HashMap::new();
        match witness {
            Witness::Base(value) => {
                value.map(|w1| {
                    value_json.insert("Base".to_string(), JsonStr(format!("{:?}", w1)));
                    w1
                });
            }
            Witness::Scalar(value) => {
                value.map(|w1| {
                    value_json.insert("Scalar".to_string(), JsonStr(format!("{:?}", w1)));
                    w1
                });
            }
            _ => unimplemented!(),
        }
        witnesses.push(JsonObj(value_json));
    }

    let mut instances = Vec::new();
    for instance in public_inputs {
        instances.push(JsonStr(format!("{:?}", instance)));
    }

    let witnesses_json = JsonArray(witnesses);
    let instances_json = JsonArray(instances);
    let witness_json = JsonObj(HashMap::from([
        ("witnesses".to_string(), witnesses_json),
        ("instances".to_string(), instances_json),
    ]));
    // This is a debugging method. We don't care about .expect() crashing.
    let json = witness_json.format().expect("cannot create debug json");
    let mut output = File::create(output_path).expect("cannot write file");
    output.write_all(json.as_bytes()).expect("write failed");
}
