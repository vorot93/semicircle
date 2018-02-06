#![feature(try_from)]
#![feature(try_trait)]
#![feature(trait_alias)]
#![feature(conservative_impl_trait)]
#![feature(generators)]
#![feature(proc_macro)]

extern crate bytes;
extern crate cancellation;
#[macro_use]
extern crate failure;
extern crate futures_await as futures;
extern crate futures_pool;
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
use futures_pool::{Builder as PoolBuilder, Pool, Sender as PoolSender};

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

pub type RadiusHandlerResult =
    Box<Future<Item = Vec<RadiusMessage>, Error = failure::Error> + Send>;

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
    cpu_pool: (PoolSender, Pool),
    error_handler: Arc<ErrorHandler + Send + Sync + 'static>,
    cancellation_token: Arc<CancellationToken>,
    handler: Box<Fn(RadiusMessage) -> RadiusHandlerResult + Send + Sync + 'static>,
}

impl ServerBuilder {
    pub fn new() -> Self {
        Self {
            cpu_pool: Pool::new(),
            error_handler: Arc::new(DummyErrorHandler),
            cancellation_token: CancellationTokenSource::new().token().clone(),
            handler: Box::new(|_| RadiusHandlerResult::from(Box::new(ok(vec![])))),
        }
    }

    #[async]
    fn main_handler(
        cpu_pool: (PoolSender, Pool),
        framed: tokio_core::net::UdpFramed<RadiusCodec>,
        error_handler: Arc<ErrorHandler + Send + Sync + 'static>,
        handler: Box<Fn(RadiusMessage) -> RadiusHandlerResult + Send + Sync + 'static>,
        cancellation_token: Arc<CancellationToken>,
    ) -> Result<(), failure::Error> {
        let (output, input) = framed.split();

        let output_ref: Arc<
            Mutex<Sink<SinkItem = RadiusMessage, SinkError = std::io::Error> + Send + 'static>,
        > = Arc::new(Mutex::new(output));

        let (sender, _pool) = cpu_pool;

        #[async]
        for request in input {
            let replies = match await!(futures::sync::oneshot::spawn(handler(request), &sender)) {
                Ok(v) => v,
                Err(e) => {
                    println!("{}", e);
                    vec![]
                }
            };

            for reply in replies {
                let send_result = output_ref.lock().unwrap().start_send(reply.clone());
                if let Err(e) = send_result {
                    match error_handler.on_send_error(reply, Some(e)) {
                        SendErrorOutcome::Drop => {
                            continue;
                        }
                        SendErrorOutcome::Stop => {
                            bail!("Stopped");
                        }
                    }
                }
            }

            if let Err(_) = cancellation_token.result() {
                bail!("Time to stop");
            }
        }

        Ok(())
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

    pub fn with_cpu_pool(mut self, builder: &mut PoolBuilder) -> Self {
        self.cpu_pool = builder.build();
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

    pub fn reconfigure<F>(self, f: F) -> Self
    where
        F: Fn(ServerBuilder) -> ServerBuilder,
    {
        Self {
            inner: f(self.inner),
            socket: self.socket,
        }
    }

    pub fn build(self) -> impl Future<Item = (), Error = failure::Error> {
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
