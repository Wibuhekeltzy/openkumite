//! Meeting by IP, with no signalling server: one hosts on a UDP port, the others join its address; they say hello
//! (the version, nickname, how many play there, a token). The guests talk to the host only: it passes their
//! rollback packets on to each other. Native only (a browser has no raw UDP; it meets in a room, `online.rs`).
//! Over the internet the host's port must reach it (forwarded on its router, or a VPN such as Tailscale /
//! ZeroTier); on a local network it just works.
//!
//! `--host [PORT]` (default 7000) / `--join HOST[:PORT]`, with `--name NICK`, `--locals 1|2`.

use crate::netlink::Link;
use crate::online::VERSION;
use anyhow::{Context, Result};
use std::net::{ToSocketAddrs, UdpSocket};

pub const DEFAULT_PORT: u16 = 7000;

fn bind(port: u16) -> Result<UdpSocket> {
    let socket = UdpSocket::bind(("0.0.0.0", port)).with_context(|| format!("UDP port {port}"))?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

/// Host on UDP `port`: the guests come to it.
pub fn host(port: u16, name: &str, locals: u8) -> Result<Link> {
    let socket = bind(port)?;
    log::info!("direct: hosting on UDP {port} ({}:{port})", local_ip().map_or("?".to_string(), |ip| ip.to_string()));
    Ok(Link::udp(socket, None, VERSION, name, locals))
}

/// Join the host at `address` (HOST or HOST:PORT).
pub fn join(address: &str, name: &str, locals: u8) -> Result<Link> {
    let address = if address.contains(':') { address.to_string() } else { format!("{address}:{DEFAULT_PORT}") };
    let host = address.to_socket_addrs().with_context(|| address.to_string())?.find(|a| a.is_ipv4()).with_context(|| format!("{address}: no IPv4 address"))?;
    let socket = bind(0)?;
    log::info!("direct: joining {host}");
    Ok(Link::udp(socket, Some(host), VERSION, name, locals))
}

/// This computer's address on its network (the one the default route leaves by; nothing is sent).
pub fn local_ip() -> Option<std::net::IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    s.local_addr().ok().map(|a| a.ip())
}
