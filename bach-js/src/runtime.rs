use crate::utils;
use bach::environment::default::Runtime as Inner;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::js_sys::{self, Promise};

#[wasm_bindgen(raw_module = "./runtime.js")]
extern "C" {
    fn run(rt: JsValue) -> Promise;
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Runtime {
    inner: Rc<RefCell<Inner>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl Runtime {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        utils::set_panic_hook();
        let inner = Rc::new(RefCell::new(Inner::default()));
        Self { inner }
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.inner.borrow_mut().set_seed(seed);
    }

    pub fn run(mut self, f: js_sys::Function) -> Result<Promise, JsValue> {
        let _ = self.enter(f);
        Ok(run(JsValue::from(self.clone())))
    }

    pub fn enter(&mut self, f: js_sys::Function) -> Result<JsValue, JsValue> {
        let this = JsValue::null();
        self.inner.borrow_mut().enter(|| f.call0(&this))
    }

    pub fn macrostep(&mut self) {
        self.inner.borrow_mut().macrostep();
    }

    #[wasm_bindgen(js_name = "hasPrimary")]
    pub fn has_primary(&self) -> bool {
        self.inner.borrow().primary_count() > 0
    }
}
