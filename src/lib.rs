#![feature(try_from)]
#![feature(try_trait)]
#![feature(trait_alias)]
#![feature(proc_macro, conservative_impl_trait, generators)]

extern crate bytes;
extern crate cancellation;
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

use std::sync::{Arc, RwLock};
use std::io;
use std::net::SocketAddr;
use std::convert::TryFrom;
use cancellation::{CancellationToken, CancellationTokenSource};
use tokio_core::net::{UdpCodec, UdpFramed, UdpSocket};
use futures::prelude::*;
use futures::future::ok;

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

pub trait RadiusIO {
    fn framed(self, codec: RadiusCodec) -> UdpFramed<RadiusCodec>;
}

impl RadiusIO for UdpSocket {
    fn framed(self, codec: RadiusCodec) -> UdpFramed<RadiusCodec> {
        self.framed(codec)
    }
}

pub type RadiusHandlerResult = Box<Future<Item = Vec<RadiusMessage>, Error = io::Error>>;

pub struct ServerBuilder {
    cancellation_token: Arc<CancellationToken>,
    handler: Arc<RwLock<Fn(RadiusMessage) -> RadiusHandlerResult>>,
}

impl ServerBuilder {
    pub fn new() -> Self {
        Self {
            cancellation_token: CancellationTokenSource::new().token().clone(),
            handler: Arc::new(RwLock::new(|_| {
                RadiusHandlerResult::from(Box::new(ok(vec![])))
            })),
        }
    }

    #[async]
    fn main_handler(
        framed: tokio_core::net::UdpFramed<RadiusCodec>,
        handler: Arc<RwLock<Fn(RadiusMessage) -> RadiusHandlerResult>>,
        cancellation_token: Arc<CancellationToken>,
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

            if let Err(_) = cancellation_token.result() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Time to stop",
                ));
            }
        }

        Ok(())
    }

    pub fn with_handler<F>(mut self, f: F) -> Self
    where
        F: Fn(RadiusMessage) -> RadiusHandlerResult + Send + Sync + 'static,
    {
        self.handler = Arc::new(RwLock::new(f));
        self
    }

    pub fn with_cancellation(mut self, token: Arc<CancellationToken>) -> Self {
        self.cancellation_token = token;
        self
    }

    pub fn with_socket<T: RadiusIO>(self, socket: T) -> Server<T> {
        Server {
            inner: self,
            socket,
        }
    }
}

pub struct Server<T: RadiusIO> {
    inner: ServerBuilder,
    socket: T,
}

impl<T: RadiusIO + 'static> Server<T> {
    #[async]
    pub fn build(self) -> io::Result<()> {
        let framed = self.socket.framed(RadiusCodec);

        await!(ServerBuilder::main_handler(
            framed,
            self.inner.handler,
            self.inner.cancellation_token
        ))
    }
}
