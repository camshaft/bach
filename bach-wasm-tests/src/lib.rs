use bach::{environment::default::Runtime, ext::*, net::UdpSocket};
use std::sync::Mutex;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_namespace = console)]
extern "C" {
    fn log(s: &str);
}

macro_rules! log {
    ($($tt:tt)*) => {{
        let out = format!("[{}]: {}\n", bach::time::Instant::now(), format_args!($($tt)*));
        log(&out[..out.len() - 1]);
        LOG.lock().unwrap().push_str(&out);
    }};
}

mod utils;

#[wasm_bindgen]
pub fn sim() -> JsValue {
    utils::set_panic_hook();

    static LOG: Mutex<String> = Mutex::new(String::new());

    let mut rt = Runtime::new();
    rt.run(|| {
        async {
            let socket = UdpSocket::bind("localhost:0").await.unwrap();

            log!("client socket: {}", socket.local_addr().unwrap());

            socket.send_to(b"hello", "server:8080").await.unwrap();

            log!("client sent request");

            let mut data = [0; 5];
            let (len, _addr) = socket.recv_from(&mut data).await.unwrap();
            assert_eq!(&data[..len], b"hello");

            log!("client got response");
        }
        .group("client")
        .primary()
        .spawn();

        async {
            let socket = UdpSocket::bind("localhost:8080").await.unwrap();

            log!("server socket: {}", socket.local_addr().unwrap());

            let mut data = [0; 5];
            let (len, addr) = socket.recv_from(&mut data).await.unwrap();

            log!("server got request");

            assert_eq!(&data[..len], b"hello");
            socket.send_to(b"hello", addr).await.unwrap();

            log!("client sent response");
        }
        .group("server")
        .spawn();
    });

    let log = core::mem::take(&mut *LOG.lock().unwrap());
    JsValue::from_str(&log)
}
