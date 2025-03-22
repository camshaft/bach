//! Test suite for the Web and headless browsers.

#![cfg(target_arch = "wasm32")]

extern crate wasm_bindgen_test;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn log_snapshot() {
    let v = bach_wasm_tests::sim();
    let expected = JsValue::from_str(include_str!("./expected.txt"));
    assert_eq!(v, expected);
}
