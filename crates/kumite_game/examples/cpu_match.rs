//! Run a whole CPU-vs-CPU match headless and print what happens:
//! `cargo run --release --example cpu_match -- [P1 model] [P2 model] [difficulty] [level]`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // tests and tools: a failure stops them
use kumite_game::bout::{Bout, Phase};
use kumite_game::Game;
use kumite_sim::input::Pad;
use std::collections::BTreeMap;

fn main() -> anyhow::Result<()> {
    let args: Vec<usize> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let (m1, m2) = (*args.first().unwrap_or(&71), *args.get(1).unwrap_or(&79));
    let (diff, level) = (*args.get(2).unwrap_or(&1), *args.get(3).unwrap_or(&4));
    let assets = kumite_assets::Assets::new(std::env::var_os("OPENKUMITE_ASSETS").map_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"), std::path::PathBuf::from))?;
    let game = Game::load(&assets, [m1, m2])?;
    let mut bout = Bout::new(game, kumite_game::ai_data(&assets)?, 2, true);
    bout.set_cpu(0, Some((diff, level)));
    bout.set_cpu(1, Some((diff, level)));
    let (mut hits, mut blocks) = ([0; 2], [0; 2]);
    let mut moves: [BTreeMap<String, u32>; 2] = Default::default();
    let mut last = [String::new(), String::new()];
    let mut attacks = [0u32; 2];
    let mut dist_at_attack: [Vec<i32>; 2] = Default::default();
    let mut frames = 0u64;
    let (mut links, mut was_linked) = (0, false);
    while !bout.finished() && frames < 60 * 60 * 6 {
        bout.step([Pad::default(); 2]);
        frames += 1;
        for e in &bout.events {
            println!("{frames:6} round {} {:?} wins {:?}", bout.round, e, bout.wins);
        }
        for h in &bout.game.fight.hits {
            if h.blocked {
                blocks[h.attacker] += 1
            } else {
                hits[h.attacker] += 1
            }
            if std::env::var_os("HITS").is_some() {
                let v = &bout.game.fight.fighters[1 - h.attacker];
                println!(
                    "{frames:6} P{} {} dmg {} blocked {} counter {} -> victim {} juggle {} y {:.0}",
                    h.attacker + 1,
                    h.anim,
                    h.damage,
                    h.blocked,
                    h.counter,
                    v.animation(&bout.game.fight.data).0,
                    v.juggle,
                    v.position.y
                );
            }
        }
        let linked = bout.game.fight.link.is_some();
        if linked && !was_linked {
            links += 1;
            let f = &bout.game.fight.fighters;
            println!("{frames:6} link: P1 {} / P2 {}", f[0].animation(&bout.game.fight.data).0, f[1].animation(&bout.game.fight.data).0);
        }
        was_linked = linked;
        if bout.phase == Phase::Fight {
            for i in 0..2 {
                let (a, _) = bout.game.fight.fighters[i].animation(&bout.game.fight.data);
                if a != last[i] {
                    let f = &bout.game.fight.fighters;
                    if f[i].current_move(&bout.game.fight.data).active[0] > 0 && f[i].current_move(&bout.game.fight.data).damage > 0 {
                        attacks[i] += 1;
                        dist_at_attack[i].push(f[0].position.distance(f[1].position) as i32);
                    }
                    *moves[i].entry(a.to_string()).or_default() += 1;
                    last[i] = a.to_string();
                }
            }
        }
    }
    println!("frames {frames}, phase {:?}, throw links {links}", bout.phase);
    for i in 0..2 {
        let mut top: Vec<_> = moves[i].iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        let mut d = dist_at_attack[i].clone();
        d.sort();
        println!("P{} attacks {} (distance p25/p50/p75 {:?})", i + 1, attacks[i], [d.get(d.len() / 4), d.get(d.len() / 2), d.get(d.len() * 3 / 4)]);
        println!("P{}: {} hits, {} blocked; moves used {}: {:?}", i + 1, hits[i], blocks[i], moves[i].len(), &top[..top.len().min(12)]);
    }
    Ok(())
}
