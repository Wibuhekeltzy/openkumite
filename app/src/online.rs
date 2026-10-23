//! Meeting in a room: every computer joins the room's code on a signalling server (matchbox), WebRTC connects each
//! to each, and they say hello (the version, nickname, how many play there, a token) on the reliable channel. The
//! one that created the room hosts it (`room.rs`): it seats the others as they come (up to four players in all),
//! chooses the mode and starts the matches.
//!
//! `--online ROOM [--signal ws://HOST:PORT] [--name NICK] [--locals 1|2] [--host-room]`

use crate::netlink::Link;

/// The signalling server when none is given (a local one: `matchbox_server`, port 3536).
pub const DEFAULT_SIGNAL: &str = "ws://127.0.0.1:3536";
/// Bumped when what the computers exchange (or the fight) changes: players of different versions are told so.
pub const VERSION: u32 = kumite_net::PROTOCOL;

/// The room `room` on `signal`: the link from now on (the peers are met as they come).
pub fn room_link(signal: &str, room: &str, name: &str, locals: u8, host: bool) -> Link {
    let url = format!("{}/{}", signal.trim_end_matches('/'), room);
    let (socket, message_loop) = matchbox_socket::WebRtcSocket::builder(url.clone()).add_unreliable_channel().add_reliable_channel().build();
    // the socket's own loop (signalling, ICE, the data channels) runs beside the game
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        // (inside a tokio context: the signalling's websocket and TLS are tokio's)
        if let Err(e) = futures::executor::block_on(async_compat::Compat::new(message_loop)) {
            log::error!("online: {e}");
        }
    });
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = message_loop.await {
            log::error!("online: {e}");
        }
    });
    log::info!("online: room {room} at {url}{}", if host { " (hosting)" } else { "" });
    Link::rtc(socket, VERSION, name, locals, host)
}

/// Two rooms meeting over WebRTC through a local signalling server (`matchbox_server` on port 3536; skipped when
/// none answers): the signalling's tokio websocket under the app's loop, the hello, the host's room list.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use crate::room::{Room, RoomEvent};

    #[test]
    fn two_rooms_meet_through_a_local_signalling_server() {
        if std::net::TcpStream::connect(("127.0.0.1", 3536)).is_err() {
            return;
        }
        let code = format!("TEST{}", std::process::id());
        let mut host = Room::new(super::room_link(super::DEFAULT_SIGNAL, &code, "HOST", 1, true), 1);
        let mut guest = Room::new(super::room_link(super::DEFAULT_SIGNAL, &code, "GUEST", 1, false), 0);
        let start = std::time::Instant::now();
        while !(host.members.len() == 2 && guest.seat().is_some()) {
            assert!(start.elapsed().as_secs() < 20, "the rooms never met");
            for e in host.poll().into_iter().chain(guest.poll()) {
                assert!(!matches!(e, RoomEvent::Left(_)), "{e:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// A peer in the room on a bare websocket: the signalling server's messages to it, and its own.
    struct Bare(tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>);

    impl Bare {
        fn join(code: &str) -> Self {
            let (ws, _) = tungstenite::connect(format!("{}/{code}", super::DEFAULT_SIGNAL)).unwrap();
            if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_ref() {
                s.set_read_timeout(Some(std::time::Duration::from_secs(20))).unwrap();
            }
            Self(ws)
        }

        /// The next message from the server that `pick` takes.
        fn wait<T>(&mut self, pick: impl Fn(&serde_json::Value) -> Option<T>) -> T {
            loop {
                if let tungstenite::Message::Text(t) = self.0.read().unwrap() {
                    if let Some(v) = serde_json::from_str(t.as_str()).ok().as_ref().and_then(&pick) {
                        return v;
                    }
                }
            }
        }

        fn signal(&mut self, to: &serde_json::Value, data: serde_json::Value) {
            let m = serde_json::json!({ "Signal": { "receiver": to, "data": data } });
            self.0.send(tungstenite::Message::text(m.to_string())).unwrap();
        }
    }

    /// A peer offering or answering what is not SDP leaves its handshake pending; the host's socket goes on (a guest
    /// still meets it).
    #[test]
    fn a_bad_offer_or_answer_does_not_end_the_hosts_socket() {
        if std::net::TcpStream::connect(("127.0.0.1", 3536)).is_err() {
            return;
        }
        let code = format!("BAD{}", std::process::id());
        // one here before the host: offered to by it (the host accepts the handshake)
        let mut offers = Bare::join(&code);
        let mut host = Room::new(super::room_link(super::DEFAULT_SIGNAL, &code, "HOST", 1, true), 1);
        let to = offers.wait(|v| v.get("NewPeer").cloned());
        offers.signal(&to, serde_json::json!({ "Offer": "NOT SDP" }));
        // one after it: answers its offer
        let mut answers = Bare::join(&code);
        let to = answers.wait(|v| v.get("Signal").filter(|s| s["data"].get("Offer").is_some()).map(|s| s["sender"].clone()));
        answers.signal(&to, serde_json::json!({ "Answer": "NOT SDP" }));
        let mut guest = Room::new(super::room_link(super::DEFAULT_SIGNAL, &code, "GUEST", 1, false), 0);
        let start = std::time::Instant::now();
        while !(host.members.len() == 2 && guest.seat().is_some()) {
            assert!(start.elapsed().as_secs() < 20, "the rooms never met");
            for e in host.poll().into_iter().chain(guest.poll()) {
                assert!(!matches!(e, RoomEvent::Left(_)), "{e:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
