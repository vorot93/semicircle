pub mod errors;
pub mod pkt;
pub mod util;

use {
    async_trait::async_trait,
    futures::{sink::SinkExt, stream::FuturesUnordered},
    std::{convert::TryFrom, future::Future, io, sync::Arc},
    tokio::{net::UdpSocket, stream::*},
    tokio_util::{codec::*, udp::*},
};

#[derive(Clone, Debug, PartialEq)]
pub struct RadiusMessage {
    pub addr: std::net::SocketAddr,
    pub data: pkt::RadiusData,
}

#[async_trait]
pub trait RadiusHandler: Send + Sync + 'static {
    async fn handle(
        &self,
        pkt: RadiusMessage,
    ) -> Result<Vec<RadiusMessage>, Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
impl<F, Fut> RadiusHandler for F
where
    F: Fn(RadiusMessage) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Vec<RadiusMessage>, Box<dyn std::error::Error + Send + Sync>>>
        + Send
        + 'static,
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

impl Default for ServerBuilder {
    fn default() -> Self {
        Self {
            handler: Arc::new(|_| async { Ok(vec![]) }),
        }
    }
}

impl ServerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_handler<F>(mut self, f: F) -> Self
    where
        F: RadiusHandler,
    {
        self.handler = Arc::new(f);
        self
    }

    pub async fn build(self, socket: UdpSocket) {
        let Self { handler } = self;

        let (mut output, mut input) =
            futures::stream::StreamExt::split(UdpFramed::new(socket, BytesCodec::new()));

        let (sender_tx, mut sender_rx) = tokio::sync::mpsc::channel(1000);

        let tasks = FuturesUnordered::new();

        tasks.push(tokio::spawn(async move {
            while let Some((p, addr)) = input.next().await.transpose().unwrap() {
                let _ = || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                    let data = pkt::RadiusData::try_from(
                        radius_parser::parse_radius_data(p.as_ref())
                            .map_err(|e| format!("{:?}", e))?
                            .1,
                    )?;

                    tokio::spawn({
                        let handler = handler.clone();
                        let mut sender_tx = sender_tx.clone();
                        async move {
                            for pkt in handler.handle(RadiusMessage { data, addr }).await.unwrap() {
                                sender_tx.send(pkt).await.unwrap();
                            }
                        }
                    });

                    Ok(())
                }();
            }
        }));

        tasks.push(tokio::spawn(async move {
            while let Some(RadiusMessage { data, addr }) = sender_rx.next().await {
                output.send((Vec::from(data).into(), addr)).await.unwrap();
            }
        }));

        futures::stream::StreamExt::collect::<Vec<_>>(tasks).await;
    }
}
