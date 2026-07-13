use serde::Serialize;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket as StdUdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tracing::{error, info};

/// Discovery announce payload (ADR-0016). `bind` is the authoritative
/// WebSocket target; no internal state is exposed.
#[derive(Serialize)]
struct Announce<'a> {
    app: &'a str,
    proto: &'a str,
    bind: &'a str,
    ts: u64,
}

/// Spawn a best-effort background task that announces `bind_addr` to the
/// `maddr:port` multicast group every 2s.
///
/// With `broadcast_fallback` it also
/// sends the same payload to 255.255.255.255:port for networks that block
/// multicast. Detached; failures are logged, never fatal.
pub fn spawn_broadcaster(maddr: String, port: u16, broadcast_fallback: bool, bind_addr: String) {
    tokio::spawn(async move {
        let multicast: SocketAddr = match format!("{maddr}:{port}").parse() {
            Ok(a) => a,
            Err(e) => {
                error!(%e, %maddr, "invalid LAN multicast address; broadcaster disabled");
                return;
            }
        };

        // 0.0.0.0:0 lets the OS pick the source port/interface for sending.
        let sock = match StdUdpSocket::bind("0.0.0.0:0") {
            Ok(s) => s,
            Err(e) => {
                error!(%e, "failed to bind UDP socket for LAN broadcaster");
                return;
            }
        };
        if broadcast_fallback {
            match sock.set_broadcast(true) {
                Ok(()) => {}
                Err(e) => error!(%e, "failed to enable broadcast; multicast only"),
            }
        }
        let socket = match tokio::net::UdpSocket::from_std(sock) {
            Ok(s) => s,
            Err(e) => {
                error!(%e, "failed to create tokio UDP socket for LAN broadcaster");
                return;
            }
        };

        // Send to multicast (and optionally broadcast) each tick without
        // nesting: collect the targets once.
        let mut targets = vec![("multicast", multicast)];
        if broadcast_fallback {
            let bcast = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::BROADCAST, port));
            targets.push(("broadcast", bcast));
        }

        info!(addr = %multicast, fallback = broadcast_fallback, "LAN broadcaster started");

        loop {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs();
            let msg = Announce {
                app: "parcello",
                proto: "parcello-discovery-v1",
                bind: &bind_addr,
                ts,
            };
            match serde_json::to_vec(&msg) {
                Ok(buf) => {
                    for (kind, target) in &targets {
                        if let Err(e) = socket.send_to(&buf, target).await {
                            error!(%e, kind, "failed to send LAN announce");
                        }
                    }
                }
                Err(e) => error!(%e, "failed to serialize LAN announce"),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}
