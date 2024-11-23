use super::*;
use bolero_generator::driver::{exhaustive::Driver, object::Object};
use core::ops::ControlFlow;

#[derive(Debug, Default)]
pub struct Exhaustive {
    driver: Arc<Mutex<Object<Driver>>>,
}

impl Exhaustive {
    pub fn scope(&self) -> Scope {
        Scope {
            driver: self.driver.clone(),
            can_have_children: false,
        }
    }

    pub fn step(&self) -> ControlFlow<()> {
        self.driver.lock().unwrap().step()
    }

    pub fn estimate(&self) -> f64 {
        self.driver.lock().unwrap().estimate()
    }

    pub fn serialize(&self) -> Vec<u64> {
        self.driver.lock().unwrap().serialize()
    }

    pub fn deserialize(&mut self, state: &[u64]) {
        self.driver.lock().unwrap().deserialize(state)
    }
}
