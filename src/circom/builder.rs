use ark_ff::PrimeField;
use num_bigint::BigInt;
use std::{collections::HashMap, fs::File, io::{BufReader, Cursor}, path::Path};
use wasmer::Store;

use super::{CircomCircuit, R1CS};

use crate::{
    circom::R1CSFile,
    witness::{Wasm, WitnessCalculator},
};
use color_eyre::Result;

#[derive(Debug)]
pub struct CircomBuilder<F: PrimeField> {
    pub cfg: CircomConfig<F>,
    pub inputs: HashMap<String, Vec<BigInt>>,
}

// Add utils for creating this from files / directly from bytes
#[derive(Debug)]
pub struct CircomConfig<F: PrimeField> {
    pub r1cs: R1CS<F>,
    pub wtns: WitnessCalculator,
    pub store: Store,
    pub sanity_check: bool,
}

impl<F: PrimeField> CircomConfig<F> {
    pub fn new(wtns: impl AsRef<Path>, r1cs: impl AsRef<Path>) -> Result<Self> {
        let mut store = Store::default();
        let wtns = WitnessCalculator::new(&mut store, wtns).unwrap();
        let reader = BufReader::new(File::open(r1cs)?);
        let r1cs = R1CSFile::new(reader)?.into();
        Ok(Self {
            wtns,
            r1cs,
            store,
            sanity_check: false,
        })
    }

    pub fn new_from_wasm(wasm: Wasm, r1cs: impl AsRef<Path>) -> Result<Self> {
        let mut store = Store::default();
        let wtns = WitnessCalculator::new_from_wasm(&mut store, wasm).unwrap();
        let reader = File::open(r1cs)?;
        let r1cs = R1CSFile::new(reader)?.into();
        Ok(Self {
            wtns,
            r1cs,
            store,
            sanity_check: false,
        })
    }

    pub fn new_from_bytes(wtns: &[u8], r1cs: &[u8]) -> Result<Self> {
        let mut store = Store::default();
        let wtns = WitnessCalculator::from_bytes(&mut store, wtns).unwrap();
        let r1cs = R1CSFile::new(Cursor::new(r1cs))?.into();
        Ok(Self {
            wtns,
            r1cs,
            store,
            sanity_check: false,
        })
    }
}

impl<F: PrimeField> CircomBuilder<F> {
    /// Instantiates a new builder using the provided WitnessGenerator and R1CS files
    /// for your circuit
    pub fn new(cfg: CircomConfig<F>) -> Self {
        Self {
            cfg,
            inputs: HashMap::new(),
        }
    }

    /// Pushes a Circom input at the specified name.
    pub fn push_input<T: Into<BigInt>>(&mut self, name: impl ToString, val: T) {
        let values = self.inputs.entry(name.to_string()).or_default();
        values.push(val.into());
    }

    /// Generates an empty circom circuit with no witness set, to be used for
    /// generation of the trusted setup parameters
    pub fn setup(&self) -> CircomCircuit<F> {
        let mut circom = CircomCircuit {
            r1cs: self.cfg.r1cs.clone(),
            witness: None,
        };

        // Disable the wire mapping
        circom.r1cs.wire_mapping = None;

        circom
    }

    /// Creates the circuit populated with the witness corresponding to the previously
    /// provided inputs
    pub fn build(mut self) -> Result<CircomCircuit<F>> {
        let mut circom = self.setup();

        // calculate the witness
        let witness = self.cfg.wtns.calculate_witness_element::<F, _>(
            &mut self.cfg.store,
            self.inputs,
            self.cfg.sanity_check,
        )?;
        circom.witness = Some(witness);

        // sanity check
        debug_assert!({
            use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystem};
            let cs = ConstraintSystem::<F>::new_ref();
            circom.clone().generate_constraints(cs.clone()).unwrap();
            let is_satisfied = cs.is_satisfied().unwrap();
            if !is_satisfied {
                println!(
                    "Unsatisfied constraint: {:?}",
                    cs.which_is_unsatisfied().unwrap()
                );
            }

            is_satisfied
        });

        Ok(circom)
    }
}
