#![feature(try_from)]
#![feature(try_trait)]
#![feature(trait_alias)]
#![feature(proc_macro, conservative_impl_trait, generators)]

extern crate bytes;
#[macro_use]
extern crate error_chain;
extern crate futures_await as futures;
extern crate nom;
extern crate radius_parser;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_proto;
extern crate tokio_service;

pub mod errors;
pub mod pkt;
pub mod util;

use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::io;
use std::net::SocketAddr;
use self::std::convert::TryFrom;
use tokio_core::net::{UdpCodec, UdpSocket};
use tokio_core::reactor::Core;
use futures::prelude::*;
use futures::future::ok;

#[derive(Clone, Debug, PartialEq)]
enum ServerStatus {
    Stopped,
    Running,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadiusMessage {
    pub addr: std::net::SocketAddr,
    pub data: pkt::RadiusData,
}

pub struct RadiusCodec;

impl UdpCodec for RadiusCodec {
    type In = RadiusMessage;
    type Out = RadiusMessage;

    fn decode(&mut self, addr: &SocketAddr, buf: &[u8]) -> io::Result<Self::In> {
        match radius_parser::parse_radius_data(buf) {
            nom::IResult::Done(_, unowned_pkt) => Ok(RadiusMessage {
                addr: *addr,
                data: TryFrom::try_from(unowned_pkt).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "Failed to parse packet")
                })?,
            }),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "")),
        }
    }

    fn encode(&mut self, v: Self::Out, into: &mut Vec<u8>) -> SocketAddr {
        into.append(&mut v.data.into());
        v.addr
    }
}

pub struct RequestContext {
    pub incoming: RadiusMessage,
    pub outgoing: Vec<RadiusMessage>,
}

pub type RadiusHandlerResult = Box<Future<Item = Vec<RadiusMessage>, Error = io::Error>>;

pub struct Server {
    core: Arc<Mutex<Core>>,
    addr: SocketAddr,
    time_to_stop: Arc<AtomicBool>,
    status: ServerStatus,
    handler: Arc<RwLock<Fn(RadiusMessage) -> RadiusHandlerResult>>,
}

impl Server {
    pub fn new(addr: std::net::SocketAddr) -> errors::Result<Self> {
        Ok(Self {
            core: Arc::new(Mutex::new(Core::new()?)),
            status: ServerStatus::Stopped,
            addr: addr,

            time_to_stop: Default::default(),
            handler: Arc::new(RwLock::new(|_| {
                RadiusHandlerResult::from(Box::new(ok(vec![])))
            })),
        })
    }

    #[async]
    fn main_handler(
        framed: tokio_core::net::UdpFramed<RadiusCodec>,
        handler: Arc<RwLock<Fn(RadiusMessage) -> RadiusHandlerResult>>,
        must_stop: Arc<AtomicBool>,
    ) -> std::io::Result<()> {
        let (mut output, input) = framed.split();
        #[async]
        for pkt in input {
            let handler_result = await!((handler.read().unwrap())(pkt));

            match handler_result {
                Ok(replies) => for reply in replies {
                    output.start_send(reply)?;
                },
                Err(e) => {
                    println!("{}", e);
                }
            }

            if must_stop.load(Ordering::Relaxed) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Time to stop",
                ));
            }
        }

        Ok(())
    }

    pub fn set_handler<F>(&mut self, f: F)
    where
        F: Fn(RadiusMessage) -> RadiusHandlerResult + Send + Sync + 'static,
    {
        self.handler = Arc::new(RwLock::new(f));
    }

    /// Starts the server and blocks the thread
    pub fn serve(&mut self) -> errors::Result<()> {
        if self.status == ServerStatus::Stopped {
            let handle = self.core.lock().unwrap().handle();
            let socket = UdpSocket::bind(&self.addr, &handle)?;

            let framed = socket.framed(RadiusCodec);

            self.status = ServerStatus::Running;

            let core = self.core.clone();
            let handler = self.handler.clone();
            let time_to_stop = self.time_to_stop.clone();
            core.lock()
                .unwrap()
                .run(Self::main_handler(framed, handler, time_to_stop))
                .unwrap();
        }
        Ok(())
    }
}
