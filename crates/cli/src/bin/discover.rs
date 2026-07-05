use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::Duration;

use serde::Deserialize;

#[derive(Deserialize)]
struct Announce {
    app: String,
    proto: String,
    bind: String,
    ts: u64,
}

const MULTICAST_ADDR: &str = "239.255.0.1";
const MULTICAST_PORT: u16 = 55888;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Bind to the multicast port on all interfaces.
    let bind_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MULTICAST_PORT);
    let std_sock = UdpSocket::bind(bind_addr)?;

    // Join multicast group on all interfaces (OS chooses the default).
    let group: Ipv4Addr = MULTICAST_ADDR.parse()?;
    std_sock.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)?;

    // Set a read timeout so Ctrl-C is responsive (optional).
    std_sock.set_read_timeout(Some(Duration::from_secs(5)))?;

    let socket = tokio::net::UdpSocket::from_std(std_sock)?;

    println!("listening for Parcello servers on {MULTICAST_ADDR}:{MULTICAST_PORT}...");

    let mut buf = vec![0u8; 2048];
    loop {
        match socket.recv_from(&mut buf).await {
            Ok((n, addr)) => {
                if n == 0 {
                    continue;
                }
                if let Ok(a) = serde_json::from_slice::<Announce>(&buf[..n]) {
                    if a.proto == "parcello-discovery-v1" {
                        println!("found {} at {} (announced {}s)", a.app, a.bind, a.ts);
                    } else {
                        println!("other-proto {} from {}: {}", a.proto, a.bind, a.ts);
                    }
                } else {
                    println!("raw from {}: {}", addr, String::from_utf8_lossy(&buf[..n]));
                }
            }
            Err(e) => {
                // timeout or other; just continue so Ctrl-C can break.
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    eprintln!("recv error: {e}");
                }
            }
        }
    }
}
