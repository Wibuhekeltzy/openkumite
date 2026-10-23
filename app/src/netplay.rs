//! Online play: rollback netcode (`ggrs`, GGPO-style) over a match of 2 to 4 players (VS, Ball, the tag
//! brawl, Force). Each computer runs the match at once on its own pads and predicted pads for the others
//! (the last ones repeated); when a real pad arrives and differs, the match goes back to the state saved at that
//! frame and runs again to now. The state is [`NetState`]: the fight's whole [`Bout`], the tag match or the Force
//! run (they clone cheaply: the bodies and data are shared, read-only); its checksum is compared to catch a desync.
//! When a match is over: [`kumite_net::Watch`].
//!
//! - `--net-synctest N`: no network; every frame goes back N frames and runs them again, checking the checksums
//!   match (the rollback's own test: everything the fight depends on is in the saved state).
//! - `--net-local PORT --net-peer HOST:PORT --net-player 0|1`: two players over UDP (two windows on one machine, or a
//!   local network), no room.
//! - `--net-delay MS --net-jitter MS --net-loss PCT`: a bad connection, simulated on the packets received (both
//!   sides: the round trip is twice the delay).
//! - The ONLINE menu (or `--online ROOM`, `--host`, `--join`): a link (`netlink.rs`), then a session per match.

#[cfg(not(target_arch = "wasm32"))]
use anyhow::bail;
use anyhow::{Context, Result};
use ggrs::{GgrsError, GgrsEvent, GgrsRequest, P2PSession, PlayerType, SessionBuilder, SessionState, SyncTestSession};
use kumite_game::bout::Bout;
use kumite_sim::input::Pad;
use std::net::SocketAddr;
#[cfg(not(target_arch = "wasm32"))]
use web_time::{Duration, Instant};

/// The most frames waited at once when ggrs says this computer runs ahead (it reckons from what the peer reports:
/// a lying report must not freeze the match for minutes).
const MAX_SKIP: u32 = 8;

pub use kumite_net::NetInput;

#[derive(Debug)]
pub struct NetConfig;

/// Who another computer is: a UDP address (`--net-local` testing), or its seat in the room (`netlink.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NetAddr {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Udp(SocketAddr),
    Seat(u8),
}

impl ggrs::Config for NetConfig {
    type Input = NetInput;
    type InputPredictor = ggrs::PredictRepeatLast;
    type State = NetState;
    type Address = NetAddr;
}

/// What a rollback session saves: the fight (VS, Ball) or the tag match.
#[derive(Clone)]
pub enum NetState {
    Bout(Box<Bout>),
    Tag(Box<kumite_game::tag::TagGame>),
    /// Force: the fight and the stage.
    Force(Box<ForceState>),
}

/// Force's rollback state: the fight and its stage logic.
#[derive(Clone)]
pub struct ForceState(pub Bout, pub kumite_game::force::ForceStage);

impl NetGame for ForceState {
    fn wrap(self) -> NetState {
        NetState::Force(Box::new(self))
    }
    fn unwrap(s: NetState) -> Option<Self> {
        if let NetState::Force(f) = s {
            Some(*f)
        } else {
            None
        }
    }
    fn checksum(&self) -> u64 {
        // (the stage's progress with the fight's)
        self.0.game.state_hash() ^ (self.1.cam_x as u64).rotate_left(17) ^ (self.1.score as u64).rotate_left(41) ^ (self.1.state as u64) << 56
    }
}

/// A match a rollback session runs: saved whole, checked by its hash.
pub trait NetGame: Clone {
    fn wrap(self) -> NetState;
    fn unwrap(s: NetState) -> Option<Self>;
    fn checksum(&self) -> u64;
}

impl NetGame for Bout {
    fn wrap(self) -> NetState {
        NetState::Bout(Box::new(self))
    }
    fn unwrap(s: NetState) -> Option<Self> {
        if let NetState::Bout(b) = s {
            Some(*b)
        } else {
            None
        }
    }
    fn checksum(&self) -> u64 {
        self.game.state_hash()
    }
}

impl NetGame for kumite_game::tag::TagGame {
    fn wrap(self) -> NetState {
        NetState::Tag(Box::new(self))
    }
    fn unwrap(s: NetState) -> Option<Self> {
        if let NetState::Tag(t) = s {
            Some(*t)
        } else {
            None
        }
    }
    fn checksum(&self) -> u64 {
        self.state_hash()
    }
}

/// A bad connection for testing (native: UDP): each packet received waits `delay` ± `jitter`, or is lost (`loss` 0..1).
#[cfg(not(target_arch = "wasm32"))]
pub struct LossySocket {
    inner: ggrs::UdpNonBlockingSocket,
    delay: Duration,
    jitter: Duration,
    loss: f32,
    queue: Vec<(Instant, SocketAddr, ggrs::Message)>,
    rng: u32,
}

#[cfg(not(target_arch = "wasm32"))]
impl LossySocket {
    fn random(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ggrs::NonBlockingSocket<NetAddr> for LossySocket {
    fn send_to(&mut self, msg: &ggrs::Message, addr: &NetAddr) {
        if let NetAddr::Udp(a) = addr {
            ggrs::NonBlockingSocket::send_to(&mut self.inner, msg, a);
        }
    }

    fn receive_all_messages(&mut self) -> Vec<(NetAddr, ggrs::Message)> {
        let now = Instant::now();
        for (addr, msg) in ggrs::NonBlockingSocket::receive_all_messages(&mut self.inner) {
            if self.random() < self.loss {
                continue;
            }
            let wait = self.delay + self.jitter.mul_f32(self.random());
            self.queue.push((now + wait, addr, msg));
        }
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.queue).into_iter().partition(|(t, _, _)| *t <= now);
        self.queue = later;
        due.into_iter().map(|(_, a, m)| (NetAddr::Udp(a), m)).collect()
    }
}

enum Session {
    P2P(Box<P2PSession<NetConfig>>),
    Sync(Box<SyncTestSession<NetConfig>>),
}

/// What a tick asks of the fight, in order.
pub type Requests = Vec<GgrsRequest<NetConfig>>;

pub struct NetPlay {
    session: Session,
    /// The players this computer plays (the sync test plays both of its two).
    pub local: Vec<usize>,
    /// The players in the session.
    pub players: usize,
    /// Frames to sit out (the session's time sync: this side ran ahead).
    skip: u32,
    /// When the match is over ([`kumite_net::Watch`]).
    watch: kumite_net::Watch<NetAddr>,
    /// Online: this computer has said its match loaded; another computer has not loaded it yet (the room says).
    pub ready_said: bool,
    pub peers_loading: bool,
    /// What the HUD line shows: synchronizing, the ping, a desync.
    pub status: String,
    /// The session's frame the fight is at, and the newest one simulated (to tell a new frame from one run again
    /// after a rollback).
    frame: i32,
    newest: i32,
    /// Frames since the other player left (the fight stops; the app leaves it after a while).
    pub gone: Option<u32>,
}

pub use kumite_net::{INPUT_DELAY, MAX_PREDICTION};

impl NetPlay {
    /// The session polled, no frame run: a match over here still sends its last inputs again until the others have
    /// them (ggrs resends on a poll; without it a lost last packet left another computer short of the match's end).
    pub fn poll(&mut self) {
        if let Session::P2P(s) = &mut self.session {
            s.poll_remote_clients();
            let _ = s.events().count();
        }
    }

    /// The command line's session, if it asks for one.
    pub fn from_args(args: &[String]) -> Result<Option<Self>> {
        let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
        if let Some(d) = arg("--net-synctest") {
            let d: usize = d.parse().context("--net-synctest N (frames to go back)")?;
            return Ok(Some(Self::synctest(d)?));
        }
        #[cfg(not(target_arch = "wasm32"))]
        return Self::udp_from_args(args);
        #[cfg(target_arch = "wasm32")]
        Ok(None)
    }

    /// `--net-local PORT --net-peer HOST:PORT --net-player 0|1 [--net-delay MS --net-jitter MS --net-loss PCT]`.
    #[cfg(not(target_arch = "wasm32"))]
    fn udp_from_args(args: &[String]) -> Result<Option<Self>> {
        let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
        let (Some(port), Some(peer)) = (arg("--net-local"), arg("--net-peer")) else { return Ok(None) };
        let port: u16 = port.parse().context("--net-local PORT")?;
        let peer: SocketAddr = peer.parse().context("--net-peer HOST:PORT")?;
        let local: usize = arg("--net-player").map_or(Ok(0), |p| p.parse()).context("--net-player 0|1")?;
        if local > 1 {
            bail!("--net-player 0|1");
        }
        let ms = |name: &str| arg(name).and_then(|v| v.parse::<u64>().ok()).map_or(Duration::ZERO, Duration::from_millis);
        let loss = arg("--net-loss").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 100.0;
        Ok(Some(Self::udp(port, peer, local, (ms("--net-delay"), ms("--net-jitter"), loss))?))
    }

    pub fn synctest(check_distance: usize) -> Result<Self> {
        let s = SessionBuilder::<NetConfig>::new()
            .with_num_players(2)?
            .with_check_distance(check_distance)
            .with_input_delay(INPUT_DELAY)
            .add_player(PlayerType::Local, 0)?
            .add_player(PlayerType::Local, 1)?
            .start_synctest_session()?;
        Ok(Self {
            session: Session::Sync(Box::new(s)),
            local: vec![0, 1],
            players: 2,
            skip: 0,
            status: format!("SYNC TEST {check_distance}"),
            frame: 0,
            newest: 0,
            gone: None,
            watch: Default::default(),
            ready_said: false,
            // (until the room says otherwise; without a room, a window started by hand: the long wait)
            peers_loading: true,
        })
    }

    /// `bad`: the simulated delay, jitter and loss of the packets received.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn udp(port: u16, peer: SocketAddr, local: usize, bad: (Duration, Duration, f32)) -> Result<Self> {
        let inner = ggrs::UdpNonBlockingSocket::bind_to_port(port).with_context(|| format!("UDP port {port}"))?;
        let socket = LossySocket { inner, delay: bad.0, jitter: bad.1, loss: bad.2, queue: Vec::new(), rng: 0x1234_5678 ^ port as u32 };
        log::info!("netplay: player {} on UDP {port}, the other at {peer}; simulated delay {:?} jitter {:?} loss {}%", local + 1, bad.0, bad.1, bad.2 * 100.0);
        Self::p2p(socket, &[if local == 0 { None } else { Some(NetAddr::Udp(peer)) }, if local == 1 { None } else { Some(NetAddr::Udp(peer)) }])
    }

    /// A session over a socket: each player's computer (None: this one).
    pub fn p2p(socket: impl ggrs::NonBlockingSocket<NetAddr> + 'static, players: &[Option<NetAddr>]) -> Result<Self> {
        let mut b = SessionBuilder::<NetConfig>::new()
            .with_num_players(players.len())?
            .with_input_delay(INPUT_DELAY)
            .with_max_prediction_window(MAX_PREDICTION)
            .with_desync_detection_mode(ggrs::DesyncDetection::On { interval: 60 })
            .with_disconnect_notify_delay(kumite_net::QUIET_NOTICE)
            .with_disconnect_timeout(kumite_net::GGRS_TIMEOUT)
            .with_fps(60)?;
        for (k, at) in players.iter().enumerate() {
            b = b.add_player(at.map_or(PlayerType::Local, PlayerType::Remote), k)?;
        }
        let s = b.start_p2p_session(socket)?;
        let local = (0..players.len()).filter(|&k| players[k].is_none()).collect();
        Ok(Self {
            session: Session::P2P(Box::new(s)),
            local,
            players: players.len(),
            skip: 0,
            status: "CONNECTING".to_string(),
            frame: 0,
            newest: 0,
            gone: None,
            watch: Default::default(),
            ready_said: false,
            // (until the room says otherwise; without a room, a window started by hand: the long wait)
            peers_loading: true,
        })
    }

    /// One 60 Hz tick: the local pads in (this computer's players' in order: its first's, its second's), what the
    /// match must do out (nothing while connecting or waiting).
    pub fn tick(&mut self, pads: &[Pad]) -> Requests {
        match &mut self.session {
            Session::Sync(s) => {
                for k in 0..2 {
                    if let Err(e) = s.add_local_input(k, NetInput::of(pads.get(k).copied().unwrap_or_default())) {
                        log::error!("netplay: {e}");
                    }
                }
                match s.advance_frame() {
                    Ok(r) => r,
                    Err(e) => {
                        // (a checksum that differs after the rollback: the saved state misses something)
                        self.status = format!("SYNC TEST FAILED: {e}");
                        log::error!("netplay sync test: {e}");
                        Vec::new()
                    }
                }
            }
            Session::P2P(s) => {
                // over: the session is left alone (not even polled: the others then hear nothing, and end theirs)
                if let Some(g) = self.gone.as_mut() {
                    *g += 1;
                    return Vec::new();
                }
                s.poll_remote_clients();
                for e in s.events() {
                    self.watch.event(&e);
                    match e {
                        GgrsEvent::WaitRecommendation { skip_frames } => self.skip = skip_frames.min(MAX_SKIP),
                        GgrsEvent::DesyncDetected { frame, local_checksum, remote_checksum, .. } => {
                            self.status = format!("DESYNC AT {frame}");
                            log::error!("netplay: desync at frame {frame}: {local_checksum:x} here, {remote_checksum:x} there");
                        }
                        GgrsEvent::NetworkInterrupted { .. } => self.status = "CONNECTION LOST...".to_string(),
                        e => log::info!("netplay: {e:?}"),
                    }
                }
                // over (a peer disconnected, quiet, never acknowledging, or the match stalled): no frame runs after
                // (kumite_net::Watch)
                // (another computer still loading its match: said, not CONNECTING)
                if s.current_state() != SessionState::Running {
                    self.status = if self.peers_loading { "WAITING FOR THE OTHERS" } else { "CONNECTING" }.to_string();
                }
                let local = &self.local;
                if let Some(why) = self.watch.over(s, (0..self.players).filter(|h| !local.contains(h)), self.peers_loading) {
                    self.status = why.to_string();
                    self.gone = Some(1);
                    return Vec::new();
                }
                if s.current_state() != SessionState::Running {
                    return Vec::new();
                }
                if self.skip > 0 {
                    self.skip -= 1;
                    return Vec::new();
                }
                for (k, &h) in self.local.iter().enumerate() {
                    if let Err(e) = s.add_local_input(h, NetInput::of(pads.get(k).copied().unwrap_or_default())) {
                        log::error!("netplay: {e}");
                        return Vec::new();
                    }
                }
                // the worst ping of the others
                let ping = (0..self.players).filter(|k| !self.local.contains(k)).filter_map(|k| s.network_stats(k).ok()).map(|st| st.ping).max();
                if let Some(ping) = ping {
                    if !self.status.starts_with("DESYNC") && !self.status.starts_with("DISCONNECTED") {
                        self.status = format!("PING {ping} MS");
                    }
                }
                match s.advance_frame() {
                    Ok(r) => r,
                    // (too far ahead of the other's pads: this frame waits)
                    Err(GgrsError::PredictionThreshold) => Vec::new(),
                    Err(e) => {
                        log::error!("netplay: {e}");
                        Vec::new()
                    }
                }
            }
        }
    }
}

/// Carry out a tick's requests on the fight: save, load, advance (`step`: one frame with every player's pad).
/// Returns how many frames advanced and whether the last one is a frame never simulated before (its sounds and
/// effects play; frames run again after a rollback stay silent).
pub fn apply<G: NetGame>(net: &mut NetPlay, game: &mut G, requests: Requests, step: &mut dyn FnMut(&mut G, &[Pad])) -> (usize, bool) {
    let (mut advanced, mut fresh) = (0, false);
    for r in requests {
        match r {
            GgrsRequest::SaveGameState { cell, frame } => cell.save(frame, Some(game.clone().wrap()), Some(game.checksum() as u128)),
            GgrsRequest::LoadGameState { cell, frame } => {
                if let Some(g) = cell.load().and_then(G::unwrap) {
                    *game = g;
                    net.frame = frame;
                }
            }
            GgrsRequest::AdvanceFrame { inputs } => {
                let pads: Vec<Pad> = inputs.iter().map(|i| i.0.pad()).collect();
                step(game, &pads);
                advanced += 1;
                net.frame += 1;
                fresh = net.frame > net.newest;
                net.newest = net.newest.max(net.frame);
            }
        }
    }
    (advanced, fresh)
}
