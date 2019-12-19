pub mod errors;
pub mod pkt;
pub mod util;

use async_trait::async_trait;
use futures::{prelude::*, stream::FuturesUnordered};
use std::convert::TryFrom;
use std::future::Future;
use std::io;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio_util::{codec::*, udp::*};

#[derive(Clone, Debug, PartialEq)]
pub struct RadiusMessage {
    pub addr: std::net::SocketAddr,
    pub data: pkt::RadiusData,
}

#[async_trait]
pub trait RadiusHandler {
    async fn handle(
        &self,
        pkt: RadiusMessage,
    ) -> Result<Vec<RadiusMessage>, Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
impl<F, Fut> RadiusHandler for F
where
    F: Fn(RadiusMessage) -> Fut + Send + Sync,
    Fut: Future<Output = Result<Vec<RadiusMessage>, Box<dyn std::error::Error + Send + Sync>>>
        + Send,
{
    async fn handle(
        &self,
        pkt: RadiusMessage,
    ) -> Result<Vec<RadiusMessage>, Box<dyn std::error::Error + Send + Sync>> {
        (self)(pkt).await
    }
}

pub enum SendErrorOutcome {
    Drop,
    Stop,
}

pub trait ErrorHandler: Send + Sync {
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
    handler: Arc<dyn RadiusHandler>,
}

impl ServerBuilder {
    pub fn new() -> Self {
        Self {
            handler: Arc::new(|_| async { Ok(vec![]) }),
        }
    }

    pub fn with_handler<F>(mut self, f: F) -> Self
    where
        F: RadiusHandler,
    {
        self.handler = Box::pin(f);
        self
    }

    pub async fn build(self, socket: UdpSocket) {
        let Self { handler } = self;

        let (output, mut input) = UdpFramed::new(socket, BytesCodec::new()).split();

        let (sender_tx, sender_rx) = tokio::sync::mpsc::channel(1000);

        let tasks = FuturesUnordered::new();

        tasks.push(tokio::spawn(async move {
            while let Some((p, addr)) = input.next().await.unwrap() {
                if let Ok(data) = radius_parser::parse_radius_data(p)
                    .and_then(|(_, data)| pkt::RadiusData::try_from(data))
                {
                    tokio::spawn({
                        let sender_tx = sender_tx.clone();
                        async move {
                            for pkt in handler.handle(RadiusMessage { data, addr }).await.unwrap() {
                                sender_tx.send(pkt).await;
                            }
                        }
                    });
                }
            }
        }));

        tasks.push(tokio::spawn(async move {
            while let Some(RadiusMessage { data, addr }) = sender_rx.next().await {
                output.send((Vec::from(data).into(), addr)).await.unwrap();
            }
        }));

        tasks.collect().await
    }
}
