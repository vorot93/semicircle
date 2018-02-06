#![feature(conservative_impl_trait)]
#![feature(generators)]
#![feature(proc_macro)]

extern crate failure;
extern crate futures_await as futures;
extern crate futures_pool;
extern crate radius_parser as rp;
extern crate semicircle;
extern crate tokio_core;
extern crate tokio_timer;

use futures::prelude::*;
use futures_pool::Pool;
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;
use tokio_timer::Timer;
use semicircle::{RadiusHandlerResult, RadiusMessage};
use std::sync::Arc;
use std::time::Duration;

fn server_handler(timer: Arc<Timer>, pkt: RadiusMessage) -> RadiusHandlerResult {
    Box::new(async_block! {
        println!("Received message from {}:\n{:?}", pkt.addr, pkt.data);

        // We will just sleep here for now. All external I/O and decision making code is up to you.
        await!(timer.sleep(Duration::from_millis(1000))).unwrap();

        println!("Slept and now forming response");

        let response = vec![
            RadiusMessage {
                addr: pkt.addr,
                data: semicircle::pkt::RadiusData {
                    code: rp::RadiusCode::AccessAccept,
                    identifier: pkt.data.identifier,
                    authenticator: pkt.data.authenticator,
                    attributes: vec![],
                },
            },
        ];

        // And here we just return packets that will be sent in return
        Ok(response)
    })
}

fn main() {
    let mut core = Core::new().unwrap();
    let socket = UdpSocket::bind(&"127.0.0.1:1812".parse().unwrap(), &core.handle())
        .expect("Failed to bind to a socket");

    let timer = Arc::new(Timer::default());
    let handler = move |pkt| server_handler(Arc::clone(&timer), pkt);

    let srv = semicircle::ServerBuilder::new()
        .with_cpu_pool(Pool::builder().pool_size(8))
        .with_handler(handler)
        .acquire_socket(socket)
        .build();

    core.run(srv).unwrap();
}
