//! What the KO'd fighter does after the KO: `cargo run --release --example ko_trace`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // tests and tools: a failure stops them
use kumite_game::bout::{Bout, Phase};
use kumite_game::Game;
use kumite_sim::input::Pad;
fn main() -> anyhow::Result<()> {
    let assets = kumite_assets::Assets::new(std::env::var_os("OPENKUMITE_ASSETS").map_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"), std::path::PathBuf::from))?;
    let mut b = Bout::new(Game::load(&assets, [71, 79])?, kumite_game::ai_data(&assets)?, 2, false);
    b.set_cpu(1, Some((2, 9)));
    let mut last = String::new();
    for f in 0..60 * 60 * 3 {
        b.step([Pad::default(); 2]);
        if matches!(b.phase, Phase::RoundOver { .. } | Phase::MatchOver { .. }) {
            let p1 = &b.game.fight.fighters[0];
            let (a, fr) = p1.animation(&b.game.fight.data);
            let tag = format!("{a} posture {:#x}", p1.posture(&b.game.fight.data));
            if tag != last {
                println!("{f:5} {:?} {} P1 {tag} @{fr} hp {}", b.phase, b.phase_frame, p1.health >> 16);
                last = tag;
            }
        }
        if b.finished() {
            break;
        }
    }
    Ok(())
}
