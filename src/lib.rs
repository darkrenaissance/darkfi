#[macro_use]
extern crate clap;
use bellman::groth16;
use bls12_381::{Bls12, Scalar};
use std::collections::{HashMap, HashSet};

pub mod async_serial;
pub mod blockchain;
pub mod bls_extensions;
pub mod circuit;
pub mod cli;
pub mod client;
pub mod crypto;
pub mod endian;
pub mod error;
pub mod net;
pub mod rpc;
pub mod serial;
pub mod service;
pub mod state;
pub mod system;
pub mod tx;
pub mod util;
pub mod vm;
pub mod vm_serial;
pub mod wallet;

pub use crate::bls_extensions::BlsStringConversion;
pub use crate::error::{Error, Result};
pub use crate::net::p2p::P2p;
pub use crate::serial::{Decodable, Encodable};
pub use crate::vm::{
    AllocType, ConstraintInstruction, CryptoOperation, VariableIndex, VariableRef,
    ZkVirtualMachine, ZkVmCircuit,
};

pub type Bytes = Vec<u8>;

pub struct ZkContract {
    pub name: String,
    pub vm: ZkVirtualMachine,
    params_map: HashMap<String, VariableIndex>,
    pub params: HashMap<VariableIndex, Scalar>,
    public_map: bimap::BiMap<String, VariableIndex>,
}

pub struct ZkProof {
    pub public: HashMap<String, Scalar>,
    pub proof: groth16::Proof<Bls12>,
}

impl ZkContract {
    // Just have a load() and save()
    // Load the contract, do the setup, save it...

    pub fn setup(&mut self, filename: &str) -> Result<()> {
        self.vm.setup()?;

        let buffer = std::fs::File::create(filename)?;
        self.vm.params.as_ref().unwrap().write(buffer)?;
        Ok(())
    }

    pub fn load_setup(&mut self, filename: &str) -> Result<()> {
        let buffer = std::fs::File::open(filename)?;
        let setup = groth16::Parameters::<Bls12>::read(buffer, false)?;
        let vk = groth16::prepare_verifying_key(&setup.vk);
        self.vm.params = Some(setup);
        self.vm.verifying_key = Some(vk);
        Ok(())
    }

    pub fn param_names(&self) -> Vec<String> {
        self.params_map.keys().cloned().collect()
    }
    pub fn set_param(&mut self, name: &str, value: Scalar) -> Result<()> {
        match self.params_map.get(name) {
            Some(index) => {
                self.params.insert(*index, value);
                Ok(())
            }
            None => Err(Error::InvalidParamName),
        }
    }

    pub fn prove(&mut self) -> Result<ZkProof> {
        // Error if params not all set
        let user_params: HashSet<_> = self.params.keys().collect();
        let req_params: HashSet<_> = self.params_map.values().collect();
        if user_params != req_params {
            return Err(Error::MissingParams);
        }

        // execute
        let params = std::mem::replace(&mut self.params, HashMap::default());
        self.vm.initialize(&params.into_iter().collect())?;

        // prove
        let proof = self.vm.prove();

        let mut public = HashMap::new();
        for (index, value) in self.vm.public() {
            match self.public_map.get_by_right(&index) {
                Some(name) => {
                    public.insert(name.clone(), value);
                }
                None => return Err(Error::BadContract),
            }
        }

        // return proof and public values (Hashmap string -> scalars)
        Ok(ZkProof { public, proof })
    }
    pub fn verify(&self, proof: &ZkProof) -> bool {
        let mut public = vec![];
        for (name, value) in &proof.public {
            match self.public_map.get_by_left(name) {
                Some(index) => {
                    public.push((index, value.clone()));
                }
                None => return false,
            }
        }
        public.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap());
        let (_, public): (Vec<VariableIndex>, Vec<Scalar>) = public.into_iter().unzip();

        // Takes proof and public values
        self.vm.verify(&proof.proof, &public)
    }
}
