use bach::time::{self, Instant as Inner};
use core::fmt;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, js_sys::Promise};

pub fn sleep(seconds: f32) -> Promise {
    let t = Duration::from_secs_f32(seconds);
    let f = time::sleep(t);
    let f = async move {
        f.await;
        Ok(JsValue::NULL)
    };
    future_to_promise(f)
}

#[wasm_bindgen]
pub struct Instant(Inner);

impl Default for Instant {
    fn default() -> Self {
        Self::now()
    }
}

impl fmt::Display for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[wasm_bindgen]
impl Instant {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::now()
    }

    pub fn now() -> Self {
        Self(Inner::now())
    }

    pub fn elapsed(&self) -> f32 {
        self.0.elapsed().as_secs_f32()
    }

    pub fn elapsed_since_start(&self) -> f32 {
        self.0.elapsed_since_start().as_secs_f32()
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}
