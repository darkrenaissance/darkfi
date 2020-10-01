use bellman::{
    gadgets::{
        boolean::{AllocatedBit, Boolean},
        multipack, num, Assignment,
    },
    groth16, Circuit, ConstraintSystem, SynthesisError,
};
use bls12_381::Bls12;
use bls12_381::Scalar;
use ff::{Field, PrimeField};
use group::Curve;
use rand::rngs::OsRng;
use std::ops::{MulAssign, Neg, SubAssign};
use std::time::Instant;

pub const CRH_IVK_PERSONALIZATION: &[u8; 8] = b"Zcashivk";

struct ZKVirtualMachine {
    ops: Vec<CryptoOperation>,
    aux: Vec<Scalar>,
    alloc: Vec<(AllocType, VariableIndex)>,
    constraints: Vec<ConstraintInstruction>,
    params: Option<groth16::Parameters<Bls12>>,
    verifying_key: Option<groth16::PreparedVerifyingKey<Bls12>>,
}

type VariableIndex = usize;

enum CryptoOperation {
    Set(VariableIndex, VariableIndex),
    Mul(VariableIndex, VariableIndex),
}

#[derive(Clone)]
enum AllocType {
    Private,
    Public,
}

impl ZKVirtualMachine {
    fn initialize(&mut self, params: &Vec<(VariableIndex, Scalar)>) {
        // Resize array
        self.aux = vec![Scalar::zero(); self.ops.len()];

        // Copy over the parameters
        for (index, value) in params {
            self.aux[*index] = *value;
        }

        for op in &self.ops {
            match op {
                CryptoOperation::Set(self_index, other_index) => {
                    self.aux[*self_index] = self.aux[*other_index];
                }
                CryptoOperation::Mul(self_index, other_index) => {
                    let other = self.aux[*other_index].clone();
                    self.aux[*self_index].mul_assign(other);
                }
            }
        }
    }

    fn public(&self) -> Vec<Scalar> {
        let mut publics = Vec::new();
        for (alloc_type, index) in &self.alloc {
            match alloc_type {
                AllocType::Private => {
                }
                AllocType::Public => {
                    let scalar = self.aux[*index].clone();
                    publics.push(scalar);
                }
            }
        }
        publics
    }

    fn setup(&mut self) {
        let start = Instant::now();
        // Create parameters for our circuit. In a production deployment these would
        // be generated securely using a multiparty computation.
        self.params = Some({
            let circuit = ZKVMCircuit {
                aux: vec![None; self.aux.len()],
                alloc: self.alloc.clone(),
                constraints: self.constraints.clone(),
            };
            groth16::generate_random_parameters::<Bls12, _, _>(circuit, &mut OsRng).unwrap()
        });

        println!("Setup: [{:?}]", start.elapsed());

        self.verifying_key = Some(groth16::prepare_verifying_key(
            &self.params.as_ref().unwrap().vk,
        ))
    }

    fn prove(&self) -> groth16::Proof<Bls12> {
        let aux = self.aux.iter().map(|scalar| Some(scalar.clone())).collect();
        // Create an instance of our circuit (with the preimage as a witness).
        let circuit = ZKVMCircuit {
            aux,
            alloc: self.alloc.clone(),
            constraints: self.constraints.clone(),
        };

        let start = Instant::now();
        // Create a Groth16 proof with our parameters.
        let proof =
            groth16::create_random_proof(circuit, self.params.as_ref().unwrap(), &mut OsRng)
                .unwrap();
        println!("Prove: [{:?}]", start.elapsed());
        proof
    }

    fn verify(&self, proof: &groth16::Proof<Bls12>, public_values: &Vec<Scalar>) -> bool {
        groth16::verify_proof(self.verifying_key.as_ref().unwrap(), proof, public_values).is_ok()
    }
}

struct ZKVMCircuit {
    aux: Vec<Option<bls12_381::Scalar>>,
    alloc: Vec<(AllocType, VariableIndex)>,
    constraints: Vec<ConstraintInstruction>,
}

impl Circuit<bls12_381::Scalar> for ZKVMCircuit {
    fn synthesize<CS: ConstraintSystem<bls12_381::Scalar>>(
        self,
        cs: &mut CS,
    ) -> Result<(), SynthesisError> {
        let mut variables = Vec::new();

        for (alloc_type, index) in &self.alloc {
            match alloc_type {
                AllocType::Private => {
                    let var = cs.alloc(|| "private alloc", || Ok(*self.aux[*index].get()?))?;
                    variables.push(var);
                }
                AllocType::Public => {
                    let var = cs.alloc_input(|| "public alloc", || Ok(*self.aux[*index].get()?))?;
                    variables.push(var);
                }
            }
        }

        let coeff_one = bls12_381::Scalar::one();
        let mut lc0 = bellman::LinearCombination::<Scalar>::zero();
        let mut lc1 = bellman::LinearCombination::<Scalar>::zero();
        let mut lc2 = bellman::LinearCombination::<Scalar>::zero();

        for constraint in self.constraints {
            match constraint {
                ConstraintInstruction::Lc0Add(index) => {
                    lc0 = lc0 + (coeff_one, variables[index]);
                }
                ConstraintInstruction::Lc1Add(index) => {
                    lc1 = lc1 + (coeff_one, variables[index]);
                }
                ConstraintInstruction::Lc2Add(index) => {
                    lc2 = lc2 + (coeff_one, variables[index]);
                }
                ConstraintInstruction::Lc1AddOne => {
                    lc1 = lc1 + CS::one();
                }
                ConstraintInstruction::Enforce => {
                    cs.enforce(
                        || "constraint",
                        |_| lc0.clone(),
                        |_| lc1.clone(),
                        |_| lc2.clone(),
                    );
                    lc0 = bellman::LinearCombination::<Scalar>::zero();
                    lc1 = bellman::LinearCombination::<Scalar>::zero();
                    lc2 = bellman::LinearCombination::<Scalar>::zero();
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
enum ConstraintInstruction {
    Lc0Add(VariableIndex),
    Lc1Add(VariableIndex),
    Lc2Add(VariableIndex),
    Lc1AddOne,
    Enforce,
}

fn main() {
    let mut vm = ZKVirtualMachine {
        ops: vec![
            // x2 = x
            CryptoOperation::Set(1, 0),
            // x2 *= x
            CryptoOperation::Mul(1, 0),
            // x3 = x2
            CryptoOperation::Set(2, 1),
            // x3 *= x
            CryptoOperation::Mul(2, 0),
        ],
        aux: vec![
        ],
        alloc: vec![
            (AllocType::Private, 0),
            (AllocType::Private, 1),
            (AllocType::Private, 2),
            (AllocType::Public, 3),
        ],
        constraints: vec![
            // x * x = x2
            ConstraintInstruction::Lc0Add(0),
            ConstraintInstruction::Lc1Add(0),
            ConstraintInstruction::Lc2Add(1),
            ConstraintInstruction::Enforce,
            // x2 * x = x3
            ConstraintInstruction::Lc0Add(1),
            ConstraintInstruction::Lc1Add(0),
            ConstraintInstruction::Lc2Add(2),
            ConstraintInstruction::Enforce,
            // x3 * 1 = public_x3
            ConstraintInstruction::Lc0Add(2),
            ConstraintInstruction::Lc1AddOne,
            ConstraintInstruction::Lc2Add(3),
            ConstraintInstruction::Enforce,
        ],
        params: None,
        verifying_key: None,
    };

    vm.setup();

    let params = vec![
        (0, Scalar::from(3))
    ];
    vm.initialize(&params);

    let proof = vm.prove();

    let public = vm.public();
    assert!(vm.verify(&proof, &public));
}
