use semicircle::RadiusMessage;
use std::time::Duration;
use tokio::net::UdpSocket;

async fn server_handler(
    pkt: RadiusMessage,
) -> Result<Vec<RadiusMessage>, Box<dyn std::error::Error + Send + Sync>> {
    println!("Received message from {}:\n{:?}", pkt.addr, pkt.data);

    // We will just sleep here for now. All external I/O and decision making code is up to you.
    tokio::time::delay_for(Duration::from_millis(1000)).await;

    println!("Slept and now forming response");

    let response = vec![RadiusMessage {
        addr: pkt.addr,
        data: semicircle::pkt::RadiusData {
            code: radius_parser::RadiusCode::AccessAccept,
            identifier: pkt.data.identifier,
            authenticator: pkt.data.authenticator,
            attributes: vec![],
        },
    }];

    // And here we just return packets that will be sent in return
    Ok(response)
}

#[tokio::main]
async fn main() {
    let socket = UdpSocket::bind("127.0.0.1:1812")
        .await
        .expect("Failed to bind to a socket");

    let srv = semicircle::ServerBuilder::new()
        .with_handler(server_handler)
        .build(socket);

    srv.await;
}
