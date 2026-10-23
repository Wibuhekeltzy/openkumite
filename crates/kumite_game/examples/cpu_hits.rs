#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // tests and tools: a failure stops them
use kumite_game::{bout::Bout, Game};
use kumite_sim::input::Pad;
fn main() -> anyhow::Result<()> {
    let assets = kumite_assets::Assets::new(std::env::var_os("OPENKUMITE_ASSETS").map_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"), std::path::PathBuf::from))?;
    for pair in [[71, 79], [79, 71], [71, 71], [79, 79], [75, 83], [83, 75]] {
        let mut b = Bout::new(Game::load(&assets, pair)?, kumite_game::ai_data(&assets)?, 2, false);
        b.set_cpu(0, Some((2, 9)));
        b.set_cpu(1, Some((2, 9)));
        let (mut hits, mut blocks, mut attacks) = ([0; 2], [0; 2], [0; 2]);
        let mut was = [false; 2];
        for _ in 0..60 * 60 * 2 {
            b.step([Pad::default(); 2]);
            for h in &b.game.fight.hits {
                if h.blocked {
                    blocks[h.attacker] += 1
                } else {
                    hits[h.attacker] += 1
                }
            }
            for k in 0..2 {
                let f = &b.game.fight.fighters[k];
                let a = f.current_move(&b.game.fight.data).active[0] != 0;
                if a && !was[k] {
                    attacks[k] += 1
                }
                was[k] = a;
            }
            if b.finished() {
                break;
            }
        }
        println!("{pair:?} attacks {attacks:?} hits {hits:?} blocked {blocks:?}");
    }
    Ok(())
}
