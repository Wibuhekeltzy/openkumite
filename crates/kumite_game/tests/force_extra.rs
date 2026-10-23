//! Force's extra stages (OpenKumite's own, made by kumite_converter from the extra levels): two levels after the
//! fourth stage, their routes, chickens and the two bosses that come in together.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // tests and tools: a failure stops them

use glam::Vec3;
use kumite_assets::Assets;
use kumite_game::bout::Bout;
use kumite_game::force::{ForceData, ForceEvent, ForceStage};
use kumite_game::{Actor, Game};
use std::path::PathBuf;
use std::sync::Arc;

fn assets() -> Option<Assets> {
    let root = std::env::var_os("OPENKUMITE_ASSETS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
    Assets::new(&root).map_err(|e| eprintln!("skipping: {e:#}")).ok()
}

const MODEL_131: usize = 131;
const MODEL_71: usize = 71;
const MODEL_79: usize = 79;

fn data(a: &Assets) -> Option<Arc<ForceData>> {
    let d: ForceData = kumite_sim::data::load(&a.root.join("gamedata/force.json")).ok()?;
    (!d.extra_stages.is_empty()).then(|| Arc::new(d))
}

#[test]
fn the_extra_stages_lie_along_their_levels_with_chickens() {
    let Some(a) = assets() else { return };
    let Some(d) = data(&a) else { return };
    let names: Vec<&str> = d.extra_stages.iter().map(|s| s.name[0].as_str()).collect();
    assert!(names.len() == 2 && names.iter().all(|n| !n.is_empty()), "{names:?}");
    for s in &d.extra_stages {
        assert!(s.route.half_width >= 1.5, "{}: room beside the line", s.name[0]);
        assert!(s.script.iter().filter(|e| e.kind == 4).count() >= 7, "{}: chickens along the way", s.name[0]);
        assert_eq!(s.script.iter().rev().find(|e| e.kind != 7).map(|e| e.kind), Some(6), "{}: the clear before the end", s.name[0]);
        assert_eq!(s.bosses, 2);
        // the route's frame: its origin at 0, its axis +x, a quarter left +z
        let r = &s.route;
        let o = Vec3::from(r.origin);
        let [ax, az] = r.axis;
        assert!(r.local(o).length() < 1e-4);
        assert!((r.local(o + Vec3::new(ax, 0.0, az) * 10.0) - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-4);
        assert!((r.local(o + Vec3::new(-az, 0.0, ax) * 2.0) - Vec3::new(0.0, 0.0, 2.0)).length() < 1e-4);
    }
}

#[test]
fn two_bosses_come_in_together_and_both_must_fall() {
    let Some(a) = assets() else { return };
    let Some(d) = data(&a) else { return };
    let costumes = d.soldier_costumes[3];
    let soldiers = [0, 1].map(|k| Actor::load(&a, 259 + 4 * costumes[k].min(3)).unwrap());
    let mut game = Game::load(&a, [MODEL_131, 259 + 4 * costumes[0].min(3)]).unwrap();
    game.round_frames = kumite_game::force::CLOCK;
    let mut bout = Bout::new(game, kumite_game::ai_data(&a).unwrap(), 1, false);
    bout.force = true;
    let mut st = ForceStage::new(d.clone(), 3, soldiers, 7 * 4, 7, 0, 0, 0, 0).extra_stage(0);
    assert_eq!((st.index, st.extra), (3, Some(0)));
    st.setup(&mut bout.game);
    st.skip_to_boss(&mut bout.game);
    let mut asked = None;
    for _ in 0..200 {
        bout.step([kumite_sim::input::Pad::default(); 2]);
        st.step(&mut bout.game, true, false);
        if let Some(ForceEvent::LoadBosses { pool, n }) = st.events.iter().find(|e| matches!(e, ForceEvent::LoadBosses { .. })) {
            asked = Some((pool.clone(), *n));
            break;
        }
    }
    let (pool, n) = asked.expect("the extra stage asks for its bosses");
    assert_eq!((pool, n), (vec![13, 14, 20, 23, 33], 2));
    let bosses = [Actor::load(&a, MODEL_71).unwrap(), Actor::load(&a, MODEL_79).unwrap()];
    st.set_bosses(&mut bout.game, &bosses, &[(0, 0), (1, 0)]);
    assert_eq!(st.bosses_in, 2);
    let g = &bout.game.fight;
    let extra = g.extra.as_deref().expect("the second boss");
    assert!(!g.fighters[1].inactive && !extra.inactive, "both in play");
    assert_eq!((g.fighters[1].max_health, extra.max_health), (d.health[3][4], d.health[3][4]));
    // the first down: the round goes on; then the second: it is won
    bout.game.fight.fighters[1].health = 0;
    st.step(&mut bout.game, true, false);
    assert_eq!(bout.game.fight.state, kumite_sim::RoundState::Fighting);
    bout.game.fight.extra.as_deref_mut().unwrap().health = 0;
    st.step(&mut bout.game, true, false);
    assert_eq!(bout.game.fight.state, kumite_sim::RoundState::Ko { winner: 0 });
}
