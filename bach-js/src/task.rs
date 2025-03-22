use crate::utils;
use bach::ext::PrimaryExt;
use pin_project_lite::pin_project;
use std::future::Future;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{
    js_sys::{self, Promise},
    JsFuture,
};

// TODO
type JoinHandle = JsValue;

macro_rules! wrap {
    ($f:expr) => {
        FakeSend::new(async move {
            let inner = async move {
                let f = $f.call0(&JsValue::null())?;
                let f = f.dyn_into::<Promise>()?;
                let f = JsFuture::from(f);
                let v = f.await?;
                <Result<_, JsValue>>::Ok(v)
            };
            FakeSend::new(inner.await)
        })
    };
}

#[wasm_bindgen]
pub fn spawn(f: js_sys::Function) -> Result<JoinHandle, JsValue> {
    utils::set_panic_hook();

    bach::spawn(wrap!(f));

    Ok(JsValue::null())
}

#[wasm_bindgen]
pub fn spawn_primary(f: js_sys::Function) -> Result<JoinHandle, JsValue> {
    utils::set_panic_hook();

    bach::spawn(wrap!(f).primary());

    Ok(JsValue::null())
}

pin_project! {
    struct FakeSend<T> {
        #[pin]
        inner: T,
    }
}

impl<T> FakeSend<T> {
    fn new(inner: T) -> Self {
        Self { inner }
    }
}

unsafe impl<T> Send for FakeSend<T> {}

impl<T> Future for FakeSend<T>
where
    T: Future,
{
    type Output = T::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.project();
        T::poll(this.inner, cx)
    }
}
