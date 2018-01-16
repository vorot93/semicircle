#![feature(try_from)]
#![feature(try_trait)]
#![feature(trait_alias)]
#![feature(conservative_impl_trait)]

extern crate bytes;
extern crate cancellation;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate futures_cpupool;
extern crate nom;
extern crate radius_parser;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_proto;
extern crate tokio_service;

pub mod errors;
pub mod pkt;
pub mod util;

use std::sync::{Arc, Mutex};
use std::io;
use std::net::SocketAddr;
use std::convert::TryFrom;
use cancellation::{CancellationToken, CancellationTokenSource};
use tokio_core::net::{UdpCodec, UdpFramed, UdpSocket};
use futures::prelude::*;
use futures::future::ok;
use futures_cpupool::CpuPool;

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

pub type RadiusHandlerResult = Box<Future<Item = Vec<RadiusMessage>, Error = io::Error> + Send>;

pub enum SendErrorOutcome {
    Drop,
    Stop,
}

pub trait ErrorHandler {
    fn on_handler_error(&self, m: &str);
    fn on_send_error(&self, msg: RadiusMessage, e: Option<io::Error>) -> SendErrorOutcome;
}

struct DummyErrorHandler;
impl ErrorHandler for DummyErrorHandler {
    fn on_handler_error(&self, _: &str) {}
    fn on_send_error(&self, _: RadiusMessage, _: Option<io::Error>) -> SendErrorOutcome {
        SendErrorOutcome::Drop
    }
}

pub struct ServerBuilder {
    cpu_pool: CpuPool,
    error_handler: Arc<ErrorHandler + Send + Sync + 'static>,
    cancellation_token: Arc<CancellationToken>,
    handler: Box<Fn(RadiusMessage) -> RadiusHandlerResult + Send + Sync + 'static>,
}

struct HandleState {
    pub error_handler: Arc<ErrorHandler + Send + Sync + 'static>,
    pub cancellation_token: Arc<CancellationToken>,
    pub output: Arc<Mutex<Sink<SinkItem = RadiusMessage, SinkError = io::Error> + Send>>,
}

impl ServerBuilder {
    pub fn new() -> Self {
        Self {
            cpu_pool: CpuPool::new(1),
            error_handler: Arc::new(DummyErrorHandler),
            cancellation_token: CancellationTokenSource::new().token().clone(),
            handler: Box::new(|_| RadiusHandlerResult::from(Box::new(ok(vec![])))),
        }
    }

    fn main_handler(
        cpu_pool: CpuPool,
        framed: tokio_core::net::UdpFramed<RadiusCodec>,
        error_handler: Arc<ErrorHandler + Send + Sync + 'static>,
        handler: Box<Fn(RadiusMessage) -> RadiusHandlerResult + Send + Sync + 'static>,
        cancellation_token: Arc<CancellationToken>,
    ) -> impl Future<Item = (), Error = io::Error> {
        let (output, input) = framed.split();

        let output_ref: Arc<
            Mutex<Sink<SinkItem = RadiusMessage, SinkError = std::io::Error> + Send + 'static>,
        > = Arc::new(Mutex::new(output));

        input
            .map(move |v| {
                (
                    HandleState {
                        error_handler: Arc::clone(&error_handler),
                        cancellation_token: Arc::clone(&cancellation_token),
                        output: Arc::clone(&output_ref),
                    },
                    v,
                )
            })
            .and_then({
                let pool = cpu_pool.clone();
                move |(state, pkt)| {
                    pool.spawn(
                        handler(pkt)
                            .or_else(|e| {
                                println!("{}", e);
                                Ok(vec![])
                            })
                            .map(move |replies| (state, replies)),
                    ).and_then(|(state, replies)| {
                            for reply in replies {
                                let send_result =
                                    state.output.lock().unwrap().start_send(reply.clone());
                                if let Err(e) = send_result {
                                    match state.error_handler.on_send_error(reply, Some(e)) {
                                        SendErrorOutcome::Drop => {
                                            continue;
                                        }
                                        SendErrorOutcome::Stop => {
                                            return Err(io::Error::new(
                                                io::ErrorKind::Other,
                                                "Stopping",
                                            ));
                                        }
                                    }
                                }
                            }
                            Ok(state)
                        })
                        .and_then(|state| {
                            state
                                .cancellation_token
                                .result()
                                .map(move |_| state)
                                .map_err(|_| io::Error::new(io::ErrorKind::Other, "Time to stop"))
                        })
                }
            })
            .for_each(|_| Ok(()))
    }

    pub fn with_error_handler<T>(mut self, error_handler: T) -> Self
    where
        T: ErrorHandler + Send + Sync + 'static,
    {
        self.error_handler = Arc::new(error_handler);
        self
    }

    pub fn with_handler<F>(mut self, f: F) -> Self
    where
        F: Fn(RadiusMessage) -> RadiusHandlerResult + Send + Sync + 'static,
    {
        self.handler = Box::new(f);
        self
    }

    pub fn with_cpu_pool(mut self, builder: &mut futures_cpupool::Builder) -> Self {
        self.cpu_pool = builder.create();
        self
    }

    pub fn with_cancellation(mut self, token: Arc<CancellationToken>) -> Self {
        self.cancellation_token = token;
        self
    }

    pub fn acquire_socket<T: RadiusIO>(self, socket: T) -> Server<T> {
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
    pub fn release_socket(self) -> ServerBuilder {
        self.inner
    }

    pub fn build(self) -> impl Future<Item = (), Error = io::Error> {
        let framed = self.socket.framed(RadiusCodec);

        ServerBuilder::main_handler(
            self.inner.cpu_pool,
            framed,
            self.inner.error_handler,
            self.inner.handler,
            self.inner.cancellation_token,
        )
    }
}
