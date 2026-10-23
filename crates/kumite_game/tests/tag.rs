//! The tag brawl (four on the stage at once: two teams of two, or each for themselves) on the real converted
//! assets.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // tests and tools: a failure stops them

use kumite_assets::Assets;
use kumite_game::tag::{Slot, TagGame, TagPhase};
use kumite_sim::input::Pad;
use std::path::PathBuf;

fn assets() -> Option<Assets> {
    let root = std::env::var_os("OPENKUMITE_ASSETS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
    Assets::new(&root).map_err(|e| eprintln!("skipping: {e:#}")).ok()
}

const MODEL_71: usize = 71;
const MODEL_79: usize = 79;
const MODEL_103: usize = 103;
const MODEL_87: usize = 87;

#[test]
fn four_cpus_play_a_match() {
    let Some(a) = assets() else { return };
    let cpu = Some((2, 9));
    let slots = [MODEL_71, MODEL_79, MODEL_103, MODEL_87].map(|record| Slot { record, cpu });
    let mut g = TagGame::load(&a, slots, vec![0, 0, 1, 1], kumite_game::ai_data(&a).unwrap(), 2).expect("load");
    let mut struck = [false; 4];
    let mut hit = [false; 4];
    let mut frames = 0;
    while !g.finished() && frames < 60 * 60 * 6 {
        g.step([Pad::default(); 4]);
        for h in &g.melee.hits {
            assert_ne!(g.melee.team[h.attacker], g.melee.team[h.victim], "no friendly hits");
            struck[h.victim] = true;
            hit[h.attacker] = true;
        }
        for f in &g.melee.actors {
            assert!(f.position.is_finite());
        }
        frames += 1;
    }
    println!("{frames} frames, wins {:?}, rounds {}, struck {struck:?}, hit {hit:?}", g.wins, g.round);
    assert!(matches!(g.phase, TagPhase::MatchOver { .. }), "the match ends");
    assert!(g.wins.iter().any(|&w| w >= 2));
    assert!(struck.iter().filter(|&&s| s).count() >= 3, "most fighters get hit: {struck:?}");
    assert!(hit.iter().filter(|&&s| s).count() >= 3, "most fighters land hits: {hit:?}");
}

#[test]
fn targets_are_the_nearest_foes() {
    let Some(a) = assets() else { return };
    let slots = [MODEL_71, MODEL_79, MODEL_103, MODEL_87].map(|record| Slot { record, cpu: None });
    let mut g = TagGame::load(&a, slots, vec![0, 0, 1, 1], kumite_game::ai_data(&a).unwrap(), 1).expect("load");
    for _ in 0..400 {
        g.step([Pad::default(); 4]);
    }
    assert_eq!(g.phase, TagPhase::Fight);
    // each fights a foe no more than the retarget margin further than the nearest
    let m = &g.melee;
    for i in 0..4 {
        let d = |j: usize| (m.actors[j].root() - m.actors[i].root()).length();
        let nearest = (0..4).filter(|&j| m.team[j] != m.team[i]).map(d).fold(f32::MAX, f32::min);
        assert_ne!(m.team[i], m.team[m.target[i]]);
        assert!(d(m.target[i]) <= nearest + 300.0, "actor {i} targets {} at {} (nearest {nearest})", m.target[i], d(m.target[i]));
    }
    // the leash keeps everyone near the group's centre
    let centre = m.actors.iter().map(|f| f.root()).sum::<glam::Vec3>() / 4.0;
    for f in &m.actors {
        let d = f.root() - centre;
        assert!(glam::Vec2::new(d.x, d.z).length() <= kumite_sim::melee::LEASH + 50.0);
    }
}

#[test]
fn each_for_themselves_the_last_one_standing_wins() {
    let Some(a) = assets() else { return };
    let cpu = Some((2, 9));
    let slots = [MODEL_71, MODEL_79, MODEL_103, MODEL_87].map(|record| Slot { record, cpu });
    let mut g = TagGame::load(&a, slots, vec![0, 1, 2, 3], kumite_game::ai_data(&a).unwrap(), 2).expect("load");
    assert_eq!(g.wins.len(), 4);
    let mut hit_by = [[false; 4]; 4];
    let mut frames = 0;
    while !g.finished() && frames < 60 * 60 * 8 {
        g.step([Pad::default(); 4]);
        for h in &g.melee.hits {
            hit_by[h.victim][h.attacker] = true;
        }
        if let TagPhase::RoundOver { winner: Some(w) } = g.phase {
            if g.phase_frame == 1 && !matches!(g.melee.state, kumite_sim::melee::MeleeState::TimeUp { .. }) {
                let standing: Vec<usize> = (0..4).filter(|&i| g.melee.standing(i)).collect();
                assert_eq!(standing, vec![w], "only the winner stands");
            }
        }
        frames += 1;
    }
    println!("{frames} frames, wins {:?}, rounds {}, hit by {hit_by:?}", g.wins, g.round);
    assert!(matches!(g.phase, TagPhase::MatchOver { winner: Some(_) } | TagPhase::MatchOver { winner: None }));
    assert!(g.wins.iter().any(|&w| w >= 2));
    // anyone may hit anyone: someone gets hit by two or more others
    assert!(hit_by.iter().any(|row| row.iter().filter(|&&h| h).count() >= 2), "{hit_by:?}");
}
