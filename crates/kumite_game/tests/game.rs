//! Integration tests on the real converted assets (`assets/` from convert_all, or `$OPENKUMITE_ASSETS`).
//! They are skipped, with a note, when the assets are not present.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // tests and tools: a failure stops them

use kumite_assets::Assets;
use kumite_game::{parse_script, Game};
use kumite_sim::data::{Bank, Moveset, MovesetIndex};
use kumite_sim::input::Pad;
use kumite_sim::{RoundState, ONE};
use std::path::PathBuf;

fn assets() -> Option<Assets> {
    let root = std::env::var_os("OPENKUMITE_ASSETS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
    match Assets::new(&root) {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!("skipping: {e:#}");
            None
        }
    }
}

// (these tests check this one game's fighters: models 71 and 79 are its characters 0 and 1, the scripts their moves,
// throws, sparks, voices and win poses; 183 its arcade's final boss)
const MODEL_71: usize = 71;
const MODEL_79: usize = 79;
const START_X: f32 = -1000.0;

/// A fight whose P2 is a CPU-style dummy: without the guard input flag (+0xCA) its neutral stance does not
/// block, so P1's attacks land.
fn load_vs_dummy(a: &Assets, models: [usize; 2]) -> Game {
    let mut g = Game::load(a, models).expect("load fight");
    g.fight.fighters[1].guard_input = false;
    g
}

/// Run a P1 script (P2 an idle dummy) and return the game.
fn run(a: &Assets, models: [usize; 2], script: &str, extra_idle: usize) -> Game {
    let mut g = load_vs_dummy(a, models);
    for pad in parse_script(script).expect("script") {
        g.step([pad, Pad::default()]);
    }
    for _ in 0..extra_idle {
        g.step([Pad::default(); 2]);
    }
    g
}

fn hp(g: &Game, i: usize) -> i32 {
    g.fight.fighters[i].health / ONE
}

fn moveset(a: &Assets, file: &str) -> Moveset {
    kumite_sim::data::load(&a.root.join("gamedata/movesets").join(file)).expect("moveset")
}

/// Every fighter but the Force soldiers casts a shadow (shadows.json), its joints all in the skeleton.
#[test]
fn every_fighter_has_its_shadow_mesh() {
    let Some(a) = assets() else { return };
    let index: MovesetIndex = kumite_sim::data::load(&a.root.join("gamedata/movesets/index.json")).unwrap();
    let shadows: kumite_assets::Shadows = a.gamedata("shadows.json").unwrap();
    for m in &index.models {
        let mesh = shadows.of(m.model_record);
        if (259..=271).contains(&m.model_record) {
            assert!(mesh.is_none(), "soldier {} has a shadow", m.model_record);
            continue;
        }
        let c = a.character(m.model_record).unwrap();
        let (v, i) = mesh.and_then(|s| c.shadow(s)).unwrap_or_else(|| panic!("model {}: no shadow", m.model_record));
        assert!(v.len() >= 70 && i.len() >= 3 * 60 && i.iter().all(|&k| (k as usize) < v.len()), "model {}", m.model_record);
    }
}

#[test]
fn every_character_loads_and_every_move_has_its_animation() {
    let Some(a) = assets() else { return };
    let index: MovesetIndex = kumite_sim::data::load(&a.root.join("gamedata/movesets/index.json")).unwrap();
    let common = moveset(&a, "common.json");
    // the original 52 models, then any extra fighters (model records 900..)
    assert_eq!(index.models.iter().filter(|m| m.model_record < 900).count(), 52);
    let mut checked = 0;
    for m in &index.models {
        let c = a.character(m.model_record).unwrap_or_else(|e| panic!("model {}: {e:#}", m.model_record));
        assert!(!c.vertices.is_empty() && !c.indices.is_empty() && !c.joints.is_empty(), "{} empty", c.name);
        let own = moveset(&a, &m.moveset_file);
        for mv in own.moves.iter().chain(&common.moves) {
            let clip = c.clip(&mv.anim).unwrap_or_else(|| panic!("{}: animation {} missing", c.name, mv.anim));
            let frames = c.clips[clip].frames as u32;
            assert!(frames == mv.frames || mv.frames == 0, "{}: {} has {frames} frames, move says {}", c.name, mv.anim, mv.frames);
            checked += 1;
        }
    }
    eprintln!("checked {checked} move animations");
}

#[test]
fn every_stage_loads() {
    let Some(a) = assets() else { return };
    for id in 0..20 {
        let s = a.stage(id).unwrap_or_else(|e| panic!("stage {id}: {e:#}"));
        assert!(!s.meshes.is_empty(), "stage {id}: no floor");
        assert!(!s.textures.is_empty(), "stage {id}: no textures");
        match id {
            0..=14 => assert!(!s.sky.is_empty(), "arena {id}: no backdrop"),
            15..=18 => {
                assert!(s.placements.iter().any(|p| p.extras["type"] == "enemy"), "force stage {id}: no enemies");
                assert!(s.item_mesh.is_some(), "force stage {id}: no pickup sprite");
            }
            _ => {}
        }
    }
}

#[test]
fn idle_fighters_stay_put_and_take_no_damage() {
    let Some(a) = assets() else { return };
    let g = run(&a, [MODEL_71, MODEL_71], "", 300);
    let x = g.fight.fighters[0].root().x;
    assert!((x - START_X).abs() < 50.0, "P1 drifted to {x}");
    assert_eq!((hp(&g, 0), hp(&g, 1)), (130, 130));
    assert_eq!(g.fight.state, RoundState::Fighting);
    // the neutral stance is move 0 of the character's own set (alias 3)
    assert_eq!(g.fight.fighters[0].current.bank, Bank::Own);
}

#[test]
fn walking_forward_and_back_moves_the_fighter() {
    let Some(a) = assets() else { return };
    let fwd = run(&a, [MODEL_71, MODEL_79], "f*60", 0).fight.fighters[0].root().x;
    assert!(fwd - START_X > 300.0, "walked forward only to {fwd}");
    let back = run(&a, [MODEL_71, MODEL_79], "b*60", 0).fight.fighters[0].root().x;
    assert!(START_X - back > 200.0, "walked back only to {back}");
}

#[test]
fn close_jab_hits_the_head_for_six_and_the_victim_reacts() {
    let Some(a) = assets() else { return };
    let ms = moveset(&a, "moveset_00.json");
    let reactions: kumite_sim::data::Reactions = kumite_sim::data::load(&a.root.join("gamedata/reactions.json")).unwrap();
    // expected victim move: the jab's reaction entry, front-standing slot, through the victim's alias table
    let jab = &ms.moves[104];
    let alias = reactions.entries[jab.reaction.index].victims[0];
    let want = ms.aliases.as_ref().unwrap()[alias];
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_71]);
    let mut reacted = false;
    for pad in parse_script("f*40,n*4,1,n*30").unwrap() {
        g.step([pad, Pad::default()]);
        let c = g.fight.fighters[1].current;
        reacted |= hp(&g, 1) < 130 && c.bank == want.bank && Some(c.index) == want.index;
    }
    assert_eq!(hp(&g, 0), 130);
    assert_eq!(hp(&g, 1), 124, "jab = 5 damage x 130% head zone");
    assert!(reacted, "victim should play reaction {:?}", want);
}

#[test]
fn jab_from_the_starting_distance_misses() {
    let Some(a) = assets() else { return };
    let g = run(&a, [MODEL_71, MODEL_71], "n*4,1,n*40", 0);
    assert_eq!(hp(&g, 1), 130);
}

#[test]
fn down_four_low_kick_hits_a_leg_zone() {
    let Some(a) = assets() else { return };
    let g = run(&a, [MODEL_71, MODEL_71], "f*40,n*4,d+4,n*60", 0);
    let dmg = 130 - hp(&g, 1);
    // d+4: base 15; leg zones scale 90-125 %
    assert!((13..=18).contains(&dmg), "d+4 did {dmg}");
}

#[test]
fn motion_input_f_n_f_1_2_starts_its_move() {
    let Some(a) = assets() else { return };
    let ms = moveset(&a, "moveset_00.json");
    let mut g = Game::load(&a, [MODEL_71, MODEL_79]).unwrap();
    let mut seen = false;
    for pad in parse_script("f*2,n*2,f+1+2,n*5").unwrap() {
        g.step([pad, Pad::default()]);
        let c = g.fight.fighters[0].current;
        seen |= c.bank == Bank::Own && c.index == 243;
    }
    assert!(seen, "f,n,f+1+2 should start move 243 ({})", ms.moves[243].anim);
}

#[test]
fn holding_down_crouches() {
    let Some(a) = assets() else { return };
    let ms = moveset(&a, "moveset_00.json");
    let g = run(&a, [MODEL_71, MODEL_79], "d*30", 0);
    let c = g.fight.fighters[0].current;
    let mv = if c.bank == Bank::Own { &ms.moves[c.index] } else { &moveset(&a, "common.json").moves[c.index] };
    // posture bit 0x01 = crouching base (kumite_converter's reference engine.md §7.1)
    assert!(mv.posture.base & 1 == 1, "holding d gave {} (posture base {})", mv.anim, mv.posture.base);
}

#[test]
fn repeated_hits_end_in_a_ko() {
    let Some(a) = assets() else { return };
    let mut script = String::from("f*40");
    for _ in 0..40 {
        // pushback moves the victim away after each hit: step back in before the next jab
        script.push_str(",f*40,n*4,1,n*30");
    }
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_71]);
    for pad in parse_script(&script).unwrap() {
        g.step([pad, Pad::default()]);
        if matches!(g.fight.state, RoundState::Ko { .. }) {
            break;
        }
    }
    assert_eq!(g.fight.state, RoundState::Ko { winner: 0 }, "P2 health {}", hp(&g, 1));
}

#[test]
fn the_clock_runs_out_after_forty_seconds() {
    let Some(a) = assets() else { return };
    let g = run(&a, [MODEL_71, MODEL_79], "", 40 * 60);
    assert_eq!(g.fight.state, RoundState::TimeUp { winner: None });
}

#[test]
fn identical_inputs_give_identical_fights() {
    let Some(a) = assets() else { return };
    let script = "f*30,n*3,1,n*10,d+4,n*20,b*15,f+2,n*30";
    let snap = |g: &Game| g.fight.fighters.iter().map(|f| (f.root().to_array().map(f32::to_bits), f.health, f.current, f.frame)).collect::<Vec<_>>();
    let (x, y) = (run(&a, [MODEL_71, MODEL_79], script, 60), run(&a, [MODEL_71, MODEL_79], script, 60));
    assert_eq!(snap(&x), snap(&y));
}

#[test]
fn walking_into_the_opponent_does_not_pass_through() {
    let Some(a) = assets() else { return };
    let g = run(&a, [MODEL_71, MODEL_79], "f*300", 0);
    let (p1, p2) = (g.fight.fighters[0].position, g.fight.fighters[1].position);
    assert!(p1.x < p2.x, "P1 walked through P2: {p1} vs {p2}");
    assert!(p1.distance(p2) >= kumite_sim::PUSH_DISTANCE - 1.0, "fighters overlap: {}", p1.distance(p2));
    assert!(g.fight.fighters[0].root().x < g.fight.fighters[1].root().x, "drawn bodies crossed");
}

#[test]
fn walk_up_then_jab_connects() {
    let Some(a) = assets() else { return };
    // the live-play sequence: hold forward for 2 s, then tap 1 three times
    let g = run(&a, [MODEL_71, MODEL_71], "f*120,n*18,1,n*42,1,n*42,1,n*30", 0);
    assert!(hp(&g, 1) < 130, "no jab connected after walking up (P2 health {})", hp(&g, 1));
}

/// Project a camera sample with the game view (pinhole; Y row scaled by 7122/4096): screen pixels from
/// the centre of the 368x480 picture, and depth.
fn project(v: &kumite_sim::camera::View, p: &kumite_sim::camera::Sample) -> (f64, f64, f64) {
    let a = |u: i64| u as f64 * std::f64::consts::TAU / 4096.0;
    let (yw, pt) = (a(v.yaw), a(v.pitch));
    let (dx, dy, dz) = ((p.x - v.eye[0]) as f64, (p.y - v.eye[1]) as f64, (p.z - v.eye[2]) as f64);
    let hd = -yw.sin() * dx + yw.cos() * dz;
    let x = yw.cos() * dx + yw.sin() * dz;
    let z = pt.cos() * hd + pt.sin() * dy;
    let y = -pt.sin() * hd + pt.cos() * dy;
    let h = v.h as f64;
    (x * h / z, y * h / z * 7122.0 / 4096.0, z)
}

#[test]
fn the_fight_camera_keeps_both_fighters_on_screen() {
    let Some(a) = assets() else { return };
    for script in ["f*300", "b*300", "f*60,n*4,1,n*20,d+4,n*30,b*120,f*60", "u*3,n*60,d*40,f*100"] {
        let mut g = Game::load(&a, [MODEL_71, MODEL_79]).unwrap();
        for (f, pad) in parse_script(script).unwrap().into_iter().enumerate() {
            g.step([pad, Pad::default()]);
            for (i, s) in g.camera_samples().iter().enumerate() {
                let (x, y, z) = project(&g.camera.view, s);
                assert!(z > 1000.0, "{script} frame {f}: P{} behind or at the camera (depth {z})", i + 1);
                assert!(x.abs() < 184.0 && y.abs() < 240.0, "{script} frame {f}: P{} off screen at ({x:.0}, {y:.0})", i + 1);
            }
        }
    }
}

#[test]
fn the_camera_frames_the_start_position_like_the_game() {
    let Some(a) = assets() else { return };
    let g = run(&a, [MODEL_71, MODEL_71], "", 30);
    let v = g.camera.view;
    // fighters 2500 apart: the centre point sits at the framing distance 5517 straight ahead
    assert_eq!((v.pitch, v.yaw, v.h), (0, 0, 500));
    assert!((v.eye[2] + 5517).abs() < 400 && v.eye[0].abs() < 100, "eye {:?}", v.eye);
    let [p1, p2] = g.camera_samples();
    let (x1, x2) = (project(&v, &p1).0, project(&v, &p2).0);
    assert!(x1 < -50.0 && x2 > 50.0 && (x1 + x2).abs() < 30.0, "P1 at {x1:.0}, P2 at {x2:.0}");
}

#[test]
fn the_round_intro_flies_in_and_holds_the_round() {
    let Some(a) = assets() else { return };
    let mut g = Game::load(&a, [MODEL_71, MODEL_79]).unwrap().with_intro(0, true);
    let start = g.camera.view;
    assert!(start.eye[2] < -12000, "intro starts far behind: {:?}", start.eye);
    let mut frames = 1; // the first intro frame is shown on load
    while g.camera.in_intro() {
        g.step([parse_script("f+1").unwrap()[0], Pad::default()]);
        frames += 1;
        assert!(frames <= 120, "intro too long");
    }
    assert_eq!(frames, 120);
    assert_eq!(g.fight.clock, kumite_sim::ROUND_FRAMES, "the clock waits for the intro");
    assert!((g.fight.fighters[0].root().x - START_X).abs() < 50.0, "pads are ignored during the intro");
    let end = g.camera.view;
    assert!((end.eye[2] + 5517).abs() < 400, "ends at the fight framing: {:?}", end.eye);
    g.step([Pad::default(); 2]);
    assert_eq!(g.fight.clock, kumite_sim::ROUND_FRAMES - 1);
}

/// Run with scripts for both players.
fn run2(a: &Assets, p1: &str, p2: &str) -> Game {
    let mut g = load_vs_dummy(a, [MODEL_71, MODEL_79]);
    let (s1, s2) = (parse_script(p1).unwrap(), parse_script(p2).unwrap());
    for f in 0..s1.len().max(s2.len()) {
        g.step([s1.get(f).copied().unwrap_or_default(), s2.get(f).copied().unwrap_or_default()]);
    }
    g
}

/// Played animations of fighter `i` over a run.
fn anims_seen(a: &Assets, p1: &str, p2: &str, i: usize) -> Vec<String> {
    let mut g = load_vs_dummy(a, [MODEL_71, MODEL_79]);
    let (s1, s2) = (parse_script(p1).unwrap(), parse_script(p2).unwrap());
    let mut seen: Vec<String> = Vec::new();
    for f in 0..s1.len().max(s2.len()) {
        g.step([s1.get(f).copied().unwrap_or_default(), s2.get(f).copied().unwrap_or_default()]);
        let anim = g.fight.fighters[i].animation(&g.fight.data).0.to_string();
        if seen.last() != Some(&anim) {
            seen.push(anim);
        }
    }
    seen
}

#[test]
fn tapping_up_or_down_sidesteps_into_or_out_of_the_screen() {
    let Some(a) = assets() else { return };
    // P1: u tap -> sidestep alias 283 (cond 0x3E, start side left), towards the background (−z)
    let ms = moveset(&a, "moveset_00.json");
    let al = ms.aliases.as_ref().unwrap();
    let anim = |alias: usize| ms.moves[al[alias].index.unwrap()].anim.clone();
    assert!(anims_seen(&a, "n*5,u*2,n*40", "", 0).contains(&anim(283)), "u tap should play {}", anim(283));
    assert!(anims_seen(&a, "n*5,d*2,n*40", "", 0).contains(&anim(285)), "d tap should play {}", anim(285));
    let up = run2(&a, "n*5,u*2,n*60", "").fight.fighters[0].root();
    let down = run2(&a, "n*5,d*2,n*60", "").fight.fighters[0].root();
    assert!(up.z < -500.0 && (up.x - START_X).abs() < 150.0, "u sidestep went to {up}");
    assert!(down.z > 500.0 && (down.x - START_X).abs() < 150.0, "d sidestep went to {down}");
    // P2 (start side right) uses the mirrored entries and also steps into the background on u
    let p2 = run2(&a, "", "n*5,u*2,n*60").fight.fighters[1].root();
    assert!(p2.z < -500.0, "P2 u sidestep went to {p2}");
}

#[test]
fn holding_up_still_jumps_instead_of_sidestepping() {
    let Some(a) = assets() else { return };
    let seen = anims_seen(&a, "n*5,u*20,n*60", "", 0);
    assert!(!seen.iter().any(|s| s == "motion_006" || s == "motion_008"), "held u sidestepped: {seen:?}");
}

#[test]
fn after_a_sidestep_both_fighters_face_each_other_and_the_camera_turns() {
    let Some(a) = assets() else { return };
    let facing = |yaw: f32| glam::Vec3::new(yaw.sin(), 0.0, yaw.cos());
    // the sidestepper keeps facing the opponent; a fighter standing still does not re-face (neutral keeps its
    // facing, facing_re §0) until it acts: a short walk turns it
    let g = run2(&a, "n*5,u*2,n*60,u*2,n*240", "");
    let [f1, f2] = &g.fight.fighters;
    let axis = f2.root() - f1.root();
    assert!(facing(f1.yaw).dot(axis.normalize()) > 0.99, "P1 yaw {} vs axis {axis}", f1.yaw.to_degrees());
    assert!(facing(f2.yaw).dot(-axis.normalize()) < 0.95, "idle P2 turned: yaw {} vs axis {axis}", f2.yaw.to_degrees());
    let g = run2(&a, "n*5,u*2,n*60,u*2,n*240", "n*200,f*12,n*97");
    let [f1, f2] = &g.fight.fighters;
    let axis = f2.root() - f1.root();
    assert!(facing(f1.yaw).dot(axis.normalize()) > 0.99, "P1 yaw {} vs axis {axis}", f1.yaw.to_degrees());
    assert!(facing(f2.yaw).dot(-axis.normalize()) > 0.99, "P2 yaw {} vs axis {axis}", f2.yaw.to_degrees());
    // camera yaw follows the P1->P2 axis (game space: z flipped), 4096 units per turn
    let [s1, s2] = g.camera_samples();
    let want = ((s2.z - s1.z) as f64).atan2((s2.x - s1.x) as f64).to_degrees();
    let got = g.camera.view.yaw as f64 * 360.0 / 4096.0;
    assert!(want.abs() > 20.0, "the sidesteps should turn the axis ({want:.1}°)");
    assert!((got - want).abs() < 6.0, "camera yaw {got:.1}° vs fighters' axis {want:.1}°");
    // and both are still on screen
    for s in [s1, s2] {
        let (x, _, z) = project(&g.camera.view, &s);
        assert!(z > 1000.0 && x.abs() < 184.0, "off screen at {x:.0}");
    }
}

#[test]
fn forward_still_walks_towards_the_opponent_after_the_camera_turned() {
    let Some(a) = assets() else { return };
    let before = run2(&a, "n*5,u*2,n*60,u*2,n*240", "");
    let after = run2(&a, "n*5,u*2,n*60,u*2,n*240,f*60", "");
    let d = |g: &Game| g.fight.fighters[0].position.distance(g.fight.fighters[1].position);
    assert!(d(&after) < d(&before) - 200.0, "walking forward: {:.0} -> {:.0}", d(&before), d(&after));
}

#[test]
fn a_jab_connects_on_a_turned_axis() {
    let Some(a) = assets() else { return };
    let g = run2(&a, "n*5,u*2,n*60,u*2,n*240,f*120,n*18,1,n*42,1,n*42,1,n*30", "");
    let p = g.fight.fighters[1].position - g.fight.fighters[0].position;
    assert!(p.z.abs() > 500.0, "the axis should be turned: {p}");
    assert!(hp(&g, 1) < 130, "no jab connected (P2 health {})", hp(&g, 1));
}

#[test]
fn recorded_scripts_read_back_to_the_same_pads() {
    let script = "n*5,f*30,d/f+1,u/b*3,1+2+3+4,b+4,d*2,n";
    let pads = parse_script(script).unwrap();
    assert_eq!(kumite_game::format_script(&pads), script);
    assert_eq!(parse_script(&kumite_game::format_script(&pads)).unwrap(), pads);
}

#[test]
fn reset_round_restores_the_start() {
    let Some(a) = assets() else { return };
    let mut g = run(&a, [MODEL_71, MODEL_71], "f*40,n*4,1,n*30,d*20", 0);
    assert!(hp(&g, 1) < 130);
    g.reset_round();
    let fresh = Game::load(&a, [MODEL_71, MODEL_71]).unwrap();
    for i in 0..2 {
        assert_eq!(g.fight.fighters[i].position, fresh.fight.fighters[i].position);
        assert_eq!(g.fight.fighters[i].current, fresh.fight.fighters[i].current);
        assert_eq!(hp(&g, i), 130);
    }
    assert_eq!((g.fight.clock, g.fight.frame), (kumite_sim::ROUND_FRAMES, 0));
    assert_eq!(g.camera.view, fresh.camera.view);
}

#[test]
fn any_move_can_be_played_on_demand() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_71]);
    for _ in 0..40 {
        g.step([parse_script("f").unwrap()[0], Pad::default()]);
    }
    // force the d+4 low kick (move 139) without its input; it hits like the real thing
    g.fight.fighters[0].play(kumite_sim::Current { bank: Bank::Own, index: 139 });
    let mut hits = Vec::new();
    for _ in 0..60 {
        g.step([Pad::default(); 2]);
        hits.extend(g.fight.hits.iter().cloned());
    }
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!((hits[0].attacker, hits[0].anim.as_str()), (0, moveset(&a, "moveset_00.json").moves[139].anim.as_str()));
    assert_eq!(hits[0].health, hp(&g, 1));
}

/// Run both scripts with P2 a human player (neutral guard on) or a CPU-style dummy; collect every hit event.
fn fight(a: &Assets, p1: &str, p2: &str, p2_human: bool) -> (Game, Vec<kumite_sim::HitEvent>) {
    let mut g = Game::load(a, [MODEL_71, MODEL_71]).expect("load fight");
    g.fight.fighters[1].guard_input = p2_human;
    let (s1, s2) = (parse_script(p1).unwrap(), parse_script(p2).unwrap());
    let mut hits = Vec::new();
    for f in 0..s1.len().max(s2.len()) {
        g.step([s1.get(f).copied().unwrap_or_default(), s2.get(f).copied().unwrap_or_default()]);
        hits.extend(g.fight.hits.iter().cloned());
    }
    (g, hits)
}

const JAB: &str = "f*40,n*4,1,n*40";
const LOW: &str = "f*40,n*4,d+4,n*60";
const MID: &str = "f*40,n*4,d/f+1,n*50";
// P2 starts on the right: "f" (right on the pad) is back for it, "d/f" is down-back
const P2_BACK: &str = "n*44,f*40";
const P2_CROUCH: &str = "d*120";

#[test]
fn standing_still_blocks_a_high_for_a_human_player() {
    let Some(a) = assets() else { return };
    let (g, hits) = fight(&a, JAB, "", true);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].blocked && hits[0].damage == 0, "{hits:?}");
    assert_eq!(hp(&g, 1), 130);
    // the blocker played the jab's block reaction (slot 10, standing guard)
    let ms = moveset(&a, "moveset_00.json");
    let reactions: kumite_sim::data::Reactions = kumite_sim::data::load(&a.root.join("gamedata/reactions.json")).unwrap();
    let want = ms.aliases.as_ref().unwrap()[reactions.entries[ms.moves[104].reaction.index].victims[10]];
    let mut g2 = Game::load(&a, [MODEL_71, MODEL_71]).unwrap();
    let mut seen = false;
    for pad in parse_script("f*40,n*4,1,n*12").unwrap() {
        g2.step([pad, Pad::default()]);
        let c = g2.fight.fighters[1].current;
        seen |= c.bank == want.bank && Some(c.index) == want.index;
    }
    assert!(seen, "P2 should play the guard reaction {want:?}");
}

#[test]
fn a_cpu_dummy_only_blocks_while_holding_back() {
    let Some(a) = assets() else { return };
    let (_, idle) = fight(&a, JAB, "", false);
    assert!(idle.len() == 1 && !idle[0].blocked, "idle dummy should be hit: {idle:?}");
    let (g, back) = fight(&a, JAB, P2_BACK, false);
    assert!(back.len() == 1 && back[0].blocked, "holding back should block: {back:?}");
    assert_eq!(hp(&g, 1), 130);
}

#[test]
fn lows_are_blocked_crouching_and_hit_standing() {
    let Some(a) = assets() else { return };
    let (_, standing) = fight(&a, LOW, "", true);
    assert!(standing.len() == 1 && !standing[0].blocked && standing[0].damage >= 13, "{standing:?}");
    let (g, crouching) = fight(&a, LOW, P2_CROUCH, true);
    assert!(crouching.len() == 1 && crouching[0].blocked, "{crouching:?}");
    assert_eq!(hp(&g, 1), 130);
}

#[test]
fn mids_are_blocked_standing_and_hit_crouching() {
    let Some(a) = assets() else { return };
    let (_, standing) = fight(&a, MID, "", true);
    assert!(standing.len() == 1 && standing[0].blocked, "{standing:?}");
    let (g, crouching) = fight(&a, MID, P2_CROUCH, true);
    assert!(crouching.len() == 1 && !crouching[0].blocked, "{crouching:?}");
    assert!(hp(&g, 1) < 130);
}

#[test]
fn highs_whiff_over_a_crouching_opponent() {
    let Some(a) = assets() else { return };
    for human in [true, false] {
        let (g, hits) = fight(&a, JAB, P2_CROUCH, human);
        assert!(hits.is_empty(), "jab should whiff over a crouch: {hits:?}");
        assert_eq!(hp(&g, 1), 130);
    }
}

#[test]
fn a_block_pushes_the_defender_back() {
    let Some(a) = assets() else { return };
    // positions just before the jab connects, then after the guard reaction
    let (before, _) = fight(&a, "f*40,n*4,1,n*5", "", true);
    let (after, hits) = fight(&a, "f*40,n*4,1,n*25", "", true);
    assert!(hits[0].blocked);
    let gap = |g: &Game| g.fight.fighters[0].position.distance(g.fight.fighters[1].position);
    // block curve 90 = 550 units; the guard animation adds its own root motion
    assert!(gap(&after) - gap(&before) > 400.0, "pushback {:.0} -> {:.0}", gap(&before), gap(&after));
}

// ---------------------------------------------------------------- matches and the CPU

use kumite_game::bout::{Announcement, Bout, Phase};

fn bout(a: &Assets, models: [usize; 2], cpu: [Option<(usize, usize)>; 2], intros: bool) -> Bout {
    let mut b = Bout::new(Game::load(a, models).unwrap(), kumite_game::ai_data(a).unwrap(), 2, intros);
    b.set_cpu(0, cpu[0]);
    b.set_cpu(1, cpu[1]);
    b
}

/// Run until the match is over (or a frame limit); returns every announcement with its frame.
fn play_out(b: &mut Bout, limit: u32) -> Vec<(u32, Announcement)> {
    let mut events = Vec::new();
    for f in 0..limit {
        b.step([Pad::default(); 2]);
        events.extend(b.events.iter().map(|e| (f, *e)));
        if b.finished() {
            break;
        }
    }
    events
}

#[test]
fn a_round_starts_with_the_fly_in_and_the_call_and_holds_the_fighters() {
    let Some(a) = assets() else { return };
    let mut b = bout(&a, [MODEL_71, MODEL_79], [None, None], true);
    assert_eq!(b.phase, Phase::Call, "the fly-in plays inside the round call");
    assert!(b.game.camera.in_intro());
    let mut events = vec![];
    let walk = parse_script("f").unwrap()[0];
    let mut fight_at = None;
    for f in 0..400 {
        b.step([walk, walk]);
        events.extend(b.events.clone());
        if b.phase == Phase::Fight && fight_at.is_none() {
            fight_at = Some(f);
            // nobody moved and the clock did not run before "Fight"
            assert!((b.game.fight.fighters[0].root().x - START_X).abs() < 60.0, "P1 at {} ({:?})", b.game.fight.fighters[0].root(), b.game.fight.fighters[0].animation(&b.game.fight.data));
            assert_eq!(b.game.fight.clock, kumite_sim::ROUND_FRAMES);
        }
    }
    assert!(events.contains(&Announcement::Fight), "{events:?}");
    let f = fight_at.expect("the fight should start");
    assert_eq!(f, kumite_game::bout::CALL_FIRST[2] as usize - 1, "the first round's call is 150 frames");
    assert!(!b.game.camera.in_intro(), "the 120-frame fly-in is over by then");
    assert!(b.game.fight.fighters[0].root().x > START_X + 300.0, "P1 should walk once the fight is on");
}

#[test]
fn a_cpu_match_is_decided_by_knockouts() {
    let Some(a) = assets() else { return };
    let mut b = bout(&a, [MODEL_71, MODEL_79], [Some((2, 9)), Some((2, 9))], false);
    let events = play_out(&mut b, 60 * 60 * 5);
    assert!(b.finished(), "match did not finish: {:?} wins {:?}", b.phase, b.wins);
    assert!(b.wins.iter().any(|&w| w >= 2));
    assert!(events.iter().any(|e| matches!(e.1, Announcement::Ko | Announcement::Great | Announcement::Perfect)), "{events:?}");
    assert!(events.iter().any(|e| matches!(e.1, Announcement::Wins(_))), "{events:?}");
}

#[test]
fn the_cpu_attacks_blocks_and_strings() {
    let Some(a) = assets() else { return };
    // Character 0 vs character 1, both ways round (one bout can end quickly): counts per character
    let (mut hits, mut blocks, mut strings, mut followups) = ([0; 2], [0; 2], [0; 2], 0);
    for order in [[0, 1], [1, 0]] {
        let models = [[MODEL_71, MODEL_79][order[0]], [MODEL_71, MODEL_79][order[1]]];
        let mut b = bout(&a, models, [Some((2, 9)), Some((2, 9))], false);
        let mut last_hit = [0u64; 2];
        for _ in 0..60 * 60 * 2 {
            b.step([Pad::default(); 2]);
            for h in &b.game.fight.hits {
                let (who, victim) = (order[h.attacker], order[1 - h.attacker]);
                if h.blocked {
                    blocks[victim] += 1;
                } else {
                    hits[who] += 1;
                    // two hits by the same player within 40 frames: a string / combo
                    // (the frame count starts again each round: wrapping, as a release build counts)
                    if b.game.fight.frame.wrapping_sub(last_hit[h.attacker]) < 40 {
                        strings[who] += 1;
                    }
                    last_hit[h.attacker] = b.game.fight.frame;
                }
            }
            if b.finished() {
                break;
            }
        }
        followups += b.cpu.iter().flatten().map(|ai| ai.followups).sum::<u32>();
    }
    eprintln!("hits {hits:?} blocks {blocks:?} strings {strings:?} follow-ups {followups}");
    assert!(hits.iter().all(|&h| h >= 5), "both CPUs should land hits: {hits:?}");
    assert!(blocks.iter().sum::<u32>() >= 3, "the CPUs should block: {blocks:?}");
    // (reaction picks and chain-throw parts: the game's follow-up strings are dead code)
    assert!(followups >= 3, "the CPUs should interrupt and counter: {followups}");
}

#[test]
fn the_cpu_beats_an_idle_player_and_poses() {
    let Some(a) = assets() else { return };
    let mut b = bout(&a, [MODEL_71, MODEL_79], [None, Some((2, 9))], false);
    let ms = moveset(&a, "moveset_01.json");
    let poses: Vec<_> = kumite_game::bout::WIN_POSES.iter().map(|&p| ms.aliases.as_ref().unwrap()[p]).collect();
    let (mut posed, mut down_after_ko, mut knockouts) = (false, true, 0);
    for _ in 0..60 * 60 * 4 {
        b.step([Pad::default(); 2]);
        let [p1, p2] = &b.game.fight.fighters;
        posed |= poses.iter().any(|r| r.bank == p2.current.bank && r.index == Some(p2.current.index));
        // the KO'd player is knocked down and stays down until the round result ends
        if let Phase::RoundOver { winner: Some(1) } = b.phase {
            if b.phase_frame == 89 && p1.health <= 0 {
                knockouts += 1;
                down_after_ko &= p1.posture(&b.game.fight.data) & kumite_sim::posture::LYING != 0;
            }
        }
        if b.finished() {
            break;
        }
    }
    assert_eq!(b.phase, Phase::MatchOver { winner: Some(1) }, "wins {:?}", b.wins);
    assert!(posed, "the winner should play a win pose");
    // an idle human still blocks highs and mids standing, so rounds can also go to time
    assert!(down_after_ko, "a KO'd player should lie on the floor at the end of the round ({knockouts} KOs)");
}

#[test]
fn cpu_matches_are_deterministic() {
    let Some(a) = assets() else { return };
    let run = || {
        let mut b = bout(&a, [MODEL_71, MODEL_79], [Some((1, 6)), Some((1, 6))], false);
        for _ in 0..3000 {
            b.step([Pad::default(); 2]);
        }
        (b.wins, b.game.fight.fighters.iter().map(|f| (f.health, f.current, f.position.to_array().map(f32::to_bits))).collect::<Vec<_>>())
    };
    assert_eq!(run(), run());
}

// ---------------------------------------------------------------- sound

#[test]
fn every_sound_code_in_the_table_has_a_sample() {
    let Some(a) = assets() else { return };
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    let table: serde_json::Value = kumite_sim::data::load(&a.root.join("gamedata/sound.json")).unwrap();
    let mut missing = Vec::new();
    for (i, c) in table["sfx_table"]["codes"].as_array().unwrap().iter().enumerate() {
        let code = c.as_u64().unwrap() as u16;
        if code != 0 && code >> 14 != 3 && s.file(code, "m071").is_none() {
            missing.push(format!("{i:#x}:{code:#06x}"));
        }
    }
    // every system / announcer code resolves (character-bank codes depend on the character)
    assert!(missing.is_empty(), "codes without a sample: {missing:?}");
    // (music by character name: characters 0 and 20, roster.json's)
    let roster: serde_json::Value = kumite_sim::data::load(&a.root.join("gamedata/roster.json")).unwrap();
    let name = |id: usize| roster["characters"][id]["name"].as_str().unwrap().to_string();
    assert!(s.music_for(&name(0)).is_some_and(|p| p.0.exists()) && s.music_for(&name(20)).is_some());
}

fn sounds_of(a: &Assets, p1: &str, p2_human: bool) -> Vec<String> {
    let s = kumite_game::sound::SoundData::load(a).unwrap();
    let mut b = Bout::new(Game::load(a, [MODEL_71, MODEL_71]).unwrap(), kumite_game::ai_data(a).unwrap(), 2, false).free_fight();
    b.game.fight.fighters[1].guard_input = p2_human;
    let mut last = kumite_game::sound::SoundState::default();
    let mut out = Vec::new();
    for pad in parse_script(p1).unwrap() {
        b.step([pad, Pad::default()]);
        for snd in s.step(&b, &mut last, ["m071", "m071"], [0, 0]) {
            assert!(snd.file.exists(), "{}", snd.file.display());
            out.push(snd.file.file_name().unwrap().to_string_lossy().to_string() + "@" + &snd.file.parent().unwrap().file_name().unwrap().to_string_lossy());
        }
    }
    out
}

#[test]
fn hits_and_blocks_make_their_sounds() {
    let Some(a) = assets() else { return };
    // jab: impact index 0x41 -> 0x61A2 (system bank, program 2, tone 2 -> key 62)
    let hit = sounds_of(&a, "f*40,n*4,1,n*30", false);
    assert!(hit.iter().any(|s| s.ends_with("@sys")), "jab hit: {hit:?}");
    // blocked jab: arm block index 0x4D -> 0x60C3 -> system key 63
    let block = sounds_of(&a, "f*40,n*4,1,n*30", true);
    assert!(block.contains(&"04.wav@sys".to_string()), "blocked jab: {block:?}");
    assert_ne!(hit, block);
}

#[test]
fn character_voices_come_from_the_characters_own_bank() {
    let Some(a) = assets() else { return };
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    let table: serde_json::Value = kumite_sim::data::load(&a.root.join("gamedata/sound.json")).unwrap();
    // each model's voice row is its actor +0x1A (0x8004F2C0: the descriptor's byte 8, `player_1A`), not the
    // Character id: every voice code of that row resolves in the model's own bank (character 14, character 20, character 17, character 18 too)
    let roster: serde_json::Value = kumite_sim::data::load(&a.root.join("gamedata/roster.json")).unwrap();
    let rows = table["voice_rows"]["rows"].as_array().unwrap();
    let mut resolved = 0;
    for e in roster["selections"]["entries"].as_array().unwrap() {
        let (Some(ch), Some(model), Some(row)) = (e["character"].as_u64(), e["model_record"].as_u64(), e["descriptor"]["player_1A"].as_u64()) else { continue };
        if ch >= 22 {
            continue;
        }
        let bank = format!("m{model:03}");
        for c in rows[row as usize]["codes"].as_array().unwrap() {
            let code = c.as_u64().unwrap() as u16;
            let f = s.file(code, &bank).unwrap_or_else(|| panic!("character {ch} (row {row}): voice {code:#06x} has no sample in {bank}"));
            assert!(f.starts_with(a.root.join("audio/sfx").join(&bank)));
            resolved += 1;
        }
    }
    assert!(resolved > 400, "{resolved}");
}

/// Every attack swings, hit or not: the system whoosh (sound-list entry 0x4A, sys/11) the frame before its first
/// active frame (0x800418F8).
#[test]
fn attacks_swing_before_they_strike() {
    let Some(a) = assets() else { return };
    let whiff = sounds_of(&a, "1,n*30", false);
    assert!(whiff.contains(&"11.wav@sys".to_string()), "a jab at the air: {whiff:?}");
    let hit = sounds_of(&a, "f*40,n*4,1,n*30", false);
    let swing = hit.iter().position(|s| s == "11.wav@sys").expect("the swing");
    assert!(hit.len() > swing + 1, "the impact after the swing: {hit:?}");
}

#[test]
fn the_announcer_calls_rounds_on_the_games_frames_with_the_traced_clips() {
    let Some(a) = assets() else { return };
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    let clip = |e: Announcement| s.announce(e, [true, false]).map(|x| x.file.file_name().unwrap().to_string_lossy().to_string());
    assert_eq!(clip(Announcement::Round(1)).as_deref(), Some("14.wav"));
    assert_eq!(clip(Announcement::RoundNumber(1)).as_deref(), Some("15.wav"));
    assert_eq!(clip(Announcement::RoundNumber(2)).as_deref(), Some("16.wav"));
    assert_eq!(clip(Announcement::FinalRound).as_deref(), Some("13.wav"));
    assert_eq!(clip(Announcement::Fight).as_deref(), Some("02.wav"));
    assert_eq!(clip(Announcement::Ko).as_deref(), Some("10.wav"));
    assert_eq!(clip(Announcement::Wins(0)).as_deref(), Some("11.wav"), "a human won: YOU WIN");
    assert_eq!(clip(Announcement::Wins(1)).as_deref(), Some("12.wav"), "the CPU won: YOU LOSE");
    for e in [Announcement::TimeUp, Announcement::Perfect, Announcement::DoubleKo, Announcement::Great, Announcement::Draw] {
        assert!(clip(e).is_some(), "{e:?}");
    }
    for c in [kumite_game::sound::Call::ContinueStart, kumite_game::sound::Call::Count(8), kumite_game::sound::Call::Count(1), kumite_game::sound::Call::ContinueZero] {
        assert!(s.call(c).is_some_and(|x| x.file.exists()), "{c:?}");
    }
    // the calls' frames in the round-call phase
    let mut b = bout(&a, [MODEL_71, MODEL_79], [Some((2, 9)), Some((2, 9))], true);
    let mut calls: Vec<(u32, u32, Announcement)> = Vec::new();
    for _ in 0..60 * 60 * 6 {
        b.step([Pad::default(); 2]);
        for e in &b.events {
            calls.push((b.round, b.phase_frame, *e));
        }
        if b.finished() {
            break;
        }
    }
    let at = |round: u32, e: Announcement| calls.iter().find(|c| c.0 == round && c.2 == e).map(|c| c.1);
    assert_eq!(at(1, Announcement::Round(1)), Some(61));
    assert_eq!(at(1, Announcement::RoundNumber(1)), Some(100));
    assert!(calls.iter().any(|c| c.0 == 1 && c.2 == Announcement::Fight), "{calls:?}");
    assert_eq!(at(2, Announcement::Round(2)), Some(1));
    assert_eq!(at(2, Announcement::RoundNumber(2)), Some(40));
    if b.round >= 3 {
        assert_eq!(at(3, Announcement::FinalRound), Some(1), "round 3 of a best-of-3 is the final round");
        assert!(at(3, Announcement::RoundNumber(3)).is_none(), "no number after FINAL ROUND");
    }
}

// ---------------------------------------------------------------- throws, counters, directions, juggles

fn common_index(a: &Assets, anim: &str) -> usize {
    moveset(a, "common.json").moves.iter().position(|m| m.anim == anim).unwrap_or_else(|| panic!("{anim}"))
}

#[test]
fn a_hit_on_an_airborne_fighter_juggles_it() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_71]);
    for pad in parse_script("f*40").unwrap() {
        g.step([pad, Pad::default()]);
    }
    // P2 is launched (the d+4 launch reaction, airborne posture) right in front of P1, who jabs
    let launch = kumite_sim::Current { bank: Bank::Common, index: common_index(&a, "common_162") };
    g.fight.fighters[1].play(launch);
    let mut events = Vec::new();
    let (mut max_y, mut juggled) = (0.0f32, 0);
    for pad in parse_script("n*2,1,n*60").unwrap() {
        g.step([pad, Pad::default()]);
        events.extend(g.fight.hits.iter().cloned());
        let f = &g.fight.fighters[1];
        max_y = max_y.max(f.position.y + f.anim_root_y);
        juggled = juggled.max(f.juggle);
    }
    assert_eq!(events.len(), 1, "{events:?}");
    // first air hit: 80 % of the jab's 5 x zone %
    assert!(events[0].damage <= 5, "juggle damage is scaled: {events:?}");
    assert_eq!(juggled, 1, "the juggle counter counts the air hit");
    assert!(max_y > 400.0, "popped up to at least the target height (440): {max_y}");
    let f = &g.fight.fighters[1];
    assert!(f.air.is_none() && f.position.y == 0.0, "landed again");
}

/// Steps both scripts; returns the game and all hit events.
fn duel(a: &Assets, models: [usize; 2], p1: &str, p2: &str, p2_human: bool, mut each: impl FnMut(usize, &mut Game)) -> (Game, Vec<kumite_sim::HitEvent>) {
    let mut g = Game::load(a, models).unwrap();
    g.fight.fighters[1].guard_input = p2_human;
    let (s1, s2) = (parse_script(p1).unwrap(), parse_script(p2).unwrap());
    let mut hits = Vec::new();
    for f in 0..s1.len().max(s2.len()) {
        each(f, &mut g);
        g.step([s1.get(f).copied().unwrap_or_default(), s2.get(f).copied().unwrap_or_default()]);
        hits.extend(g.fight.hits.iter().cloned());
    }
    (g, hits)
}

#[test]
fn hitting_an_attack_in_its_start_up_is_a_counter_hit() {
    let Some(a) = assets() else { return };
    // P2 starts d+4 (first active frame 14) just after P1's jab (active 8): the jab lands in its start-up
    let (_, hits) = duel(&a, [MODEL_71, MODEL_71], "f*40,n*4,1,n*40", "n*46,d+4,n*40", false, |_, _| {});
    let jab = hits.iter().find(|h| h.attacker == 0).expect("the jab lands");
    assert!(jab.counter, "{hits:?}");
    assert!(jab.damage >= 7, "counter hits do 12/10 + a bonus: {hits:?}");
    // the same jab on an idle dummy is a normal hit
    let (_, normal) = duel(&a, [MODEL_71, MODEL_71], "f*40,n*4,1,n*40", "", false, |_, _| {});
    assert!(!normal[0].counter && normal[0].damage < jab.damage, "{normal:?}");
}

#[test]
fn a_hit_from_behind_plays_the_back_reaction_and_cannot_be_blocked() {
    let Some(a) = assets() else { return };
    let ms = moveset(&a, "moveset_00.json");
    let reactions: kumite_sim::data::Reactions = kumite_sim::data::load(&a.root.join("gamedata/reactions.json")).unwrap();
    let back = ms.aliases.as_ref().unwrap()[reactions.entries[ms.moves[104].reaction.index].victims[8]];
    // P2 (human: its stance would block a front jab) turns its back just before the jab lands
    let mut reaction = None;
    let (_, hits) = duel(&a, [MODEL_71, MODEL_71], "f*40,n*4,1,n*20", "", true, |f, g| {
        if f >= 46 && g.fight.fighters[1].hit_flash == 0 && reaction.is_none() {
            g.fight.fighters[1].yaw = std::f32::consts::FRAC_PI_2; // facing +x, away from P1
            g.fight.fighters[1].root_yaw = std::f32::consts::FRAC_PI_2;
        }
        if reaction.is_none() && g.fight.fighters[1].hit_flash > 0 {
            reaction = Some(g.fight.fighters[1].current);
        }
    });
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(!hits[0].blocked, "back hits are never blocked");
    let c = reaction.expect("P2 reacts");
    assert_eq!((c.bank, Some(c.index)), (back.bank, back.index), "back reaction (slot 8)");
}

#[test]
fn a_throw_links_both_fighters_plays_the_victim_part_and_deals_its_damage() {
    let Some(a) = assets() else { return };
    // Character 0's 1+3 against character 1: the victim plays character 0's victim animation (from character 0's set) on character 1's body
    let mut partner_seen = false;
    let (g, hits) = duel(&a, [MODEL_71, MODEL_79], "f*40,n*4,1+3,n*120", "", true, |_, g| {
        partner_seen |= g.fight.fighters[1].current.bank == Bank::Partner;
        if g.fight.link.is_some() {
            let d = g.fight.fighters[0].root().distance(g.fight.fighters[1].root());
            assert!(d < 2000.0, "the pair stays together during the throw: {d}");
            assert!(g.pose(1).0.len() == g.bodies[1].character.nodes.len());
        }
    });
    assert!(partner_seen, "Character 1 should play the thrower's victim move");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!((hits[0].attacker, hits[0].damage), (0, 30), "front throw: 30");
    assert_eq!(hp(&g, 1), 100);
    assert!(g.fight.link.is_none(), "unlinked after the throw");
    assert!(g.fight.fighters[1].posture(&g.fight.data) & kumite_sim::posture::LYING != 0, "thrown to the floor");
}

#[test]
fn pressing_the_right_button_escapes_a_throw() {
    let Some(a) = assets() else { return };
    // the front throw's victim move breaks on button 1 within its window (cond 0x3A, frames 1-15)
    let (g, hits) = duel(&a, [MODEL_71, MODEL_79], "f*40,n*4,1+3,n*120", "n*59,1,n*80", true, |_, _| {});
    assert!(hits.is_empty(), "escaped: no damage {hits:?}");
    assert_eq!(hp(&g, 1), 130);
    // the wrong button does not escape
    let (g, hits) = duel(&a, [MODEL_71, MODEL_79], "f*40,n*4,1+3,n*120", "n*59,2,n*80", true, |_, _| {});
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hp(&g, 1), 100);
}

#[test]
fn throws_whiff_on_a_crouching_opponent() {
    let Some(a) = assets() else { return };
    let (g, hits) = duel(&a, [MODEL_71, MODEL_79], "f*40,n*4,1+3,n*80", "d*130", true, |_, _| {});
    assert!(hits.is_empty() && g.fight.link.is_none(), "{hits:?}");
}

// ---------------------------------------------------------------- effects

use kumite_game::vfx::{Blend, Vfx};

/// Run both scripts with effects; `each` sees the game and the effects after each step.
fn with_vfx(a: &Assets, models: [usize; 2], p1: &str, p2: &str, mut each: impl FnMut(usize, &Game, &Vfx)) {
    let mut g = Game::load(a, models).unwrap();
    // an empty P2 script means a dummy that does not guard; any script, a human (standing guard)
    g.fight.fighters[1].guard_input = !p2.is_empty();
    let mut v = Vfx::new(a);
    let (s1, s2) = (parse_script(p1).unwrap(), parse_script(p2).unwrap());
    for f in 0..s1.len().max(s2.len()) {
        g.step([s1.get(f).copied().unwrap_or_default(), s2.get(f).copied().unwrap_or_default()]);
        v.step(&g);
        each(f, &g, &v);
    }
}

#[test]
fn a_hit_throws_the_red_spark_and_a_block_shows_the_blue_one() {
    let Some(a) = assets() else { return };
    let first = |p1: &str, p2: &str| {
        let mut out = None;
        with_vfx(&a, [MODEL_71, MODEL_71], p1, p2, |_, g, v| {
            if out.is_none() && !g.fight.hits.is_empty() {
                let fx = v.slots.iter().flatten().last().cloned().expect("a hit-spark slot");
                assert!(fx.pos.distance(g.fight.hits[0].point / kumite_game::UNITS).max(0.0) < 0.3, "at the contact point");
                out = Some((fx, g.fight.hits[0].clone()));
            }
        });
        out.expect("a hit")
    };
    let data = Vfx::new(&a).data.expect("effects.json");
    let (hit, _) = first("f*40,n*4,1,n*20", "");
    assert!(std::sync::Arc::ptr_eq(&hit.strip.cells, &hit.strip.cells) && *hit.strip.cells == *data.spark.cells, "the red spark");
    assert!(hit.vel.length() > 0.04, "thrown along the strike: {:?}", hit.vel);
    let (block, h) = first("f*40,n*4,1,n*20", "n*1"); // P2 human: the stance blocks the jab
    assert!(h.blocked && *block.strip.cells == *data.block.cells && block.vel == glam::Vec3::ZERO, "the still block spark");
    // only three hit-spark slots ever
    let mut most = 0;
    with_vfx(&a, [MODEL_71, MODEL_71], "f*40,n*4,1,1,1,1,1,1,1,1,n*40", "", |_, _, v| most = most.max(v.slots.iter().flatten().count()));
    assert!(most <= 3);
}

#[test]
fn a_heavy_hit_adds_the_character_spark_its_light_and_flying_sparks() {
    let Some(a) = assets() else { return };
    // Character 0's b+1+2 (100 damage): 21+ damage
    let (mut lit, mut particles, mut charspark) = (None, 0, false);
    let data = Vfx::new(&a).data.expect("effects.json");
    let own = data.characters[&MODEL_71].clone();
    let index = moveset(&a, "moveset_00.json").moves.iter().position(|m| m.anim == "motion_031").unwrap();
    // walk in until it connects
    for walk in (10..80).step_by(5) {
        let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_79]);
        let mut v = Vfx::new(&a);
        for pad in parse_script(&format!("f*{walk}")).unwrap() {
            g.step([pad, Pad::default()]);
        }
        g.fight.fighters[0].play(kumite_sim::Current { bank: Bank::Own, index });
        for _ in 0..120 {
            g.step([Pad::default(); 2]);
            v.step(&g);
            if g.fight.hits.iter().any(|h| h.damage >= 21) {
                lit = v.hit_light();
            }
            particles = particles.max(v.particles.len());
            charspark |= v.slots.iter().flatten().any(|s| *s.strip.cells == *own.strip.cells);
        }
        if lit.is_some() {
            break;
        }
    }
    let (colour, strength, _) = lit.expect("the hit lights the fighters");
    assert_eq!((colour, strength), (own.light, 1.0), "character 0's orange");
    assert!(charspark, "Character 0's own spark");
    assert_eq!(particles, 26, "6 + 20 heavy-hit sparks");
}

#[test]
fn a_throw_shakes_the_camera_and_raises_dust() {
    let Some(a) = assets() else { return };
    // Character 1's front throw: shake at frame 78 and a ground impact as character 0 lands: 6 puffs, one every 2 frames
    let (mut shake, mut puffs, mut half) = (0, 0, false);
    with_vfx(&a, [MODEL_71, MODEL_79], "", "b*40,n*4,1+3,n*160", |_, _, v| {
        shake = shake.max(v.shake_pitch().abs());
        for g in v.grounds.iter().flatten() {
            puffs = puffs.max(g.puffs.len());
            half |= g.puffs.iter().all(|p| p.blend == Blend::Half);
        }
    });
    assert!(shake > 0, "the camera shakes");
    assert_eq!(puffs, 6, "a ground impact: 6 dust puffs");
    assert!(half, "the dust is half-transparent");
}

#[test]
fn a_juggled_fighter_raises_dust_where_it_lands() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_71]);
    let mut v = Vfx::new(&a);
    for pad in parse_script("f*40").unwrap() {
        g.step([pad, Pad::default()]);
    }
    g.fight.fighters[1].play(kumite_sim::Current { bank: Bank::Common, index: common_index(&a, "common_162") });
    let mut dust = false;
    for pad in parse_script("n*2,1,n*80").unwrap() {
        g.step([pad, Pad::default()]);
        v.step(&g);
        dust |= v.grounds.iter().flatten().count() > 0;
    }
    assert!(dust, "the landing raises dust");
}

#[test]
fn the_effect_atlas_has_the_game_palettes() {
    let Some(a) = assets() else { return };
    let mut dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(a.root.join("pictures/effects/atlas.png")).unwrap()));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut buf).unwrap();
    let data = Vfx::new(&a).data.unwrap();
    // the average opaque colour of a cell
    let avg = |cell: u32| {
        let (cx, cy) = ((cell % data.cols) * 32, (cell / data.cols) * 32);
        let mut s = [0u64; 3];
        let mut n = 0;
        for y in 0..32 {
            for x in 0..32 {
                let o = (((cy + y) * info.width + cx + x) * 4) as usize;
                if buf[o + 3] > 0 {
                    for k in 0..3 {
                        s[k] += buf[o + k] as u64;
                    }
                    n += 1;
                }
            }
        }
        s.map(|v| v / n.max(1))
    };
    let red = avg(data.spark.cells[6]);
    assert!(red[0] > 2 * red[1] && red[0] > 2 * red[2], "the hit spark is red: {red:?}");
    let blue = avg(data.block.cells[6]);
    assert!(blue[2] > blue[0], "the block spark is blue: {blue:?}");
    let grey = avg(data.dust.cells[6]);
    assert!(grey[0].abs_diff(grey[2]) < 20, "the dust is grey: {grey:?}");
}

/// After a K.O.: 90 frames, the half-speed replay of at least 150 frames on the replay camera, then the win pose
/// on the winner camera with only the winner drawn; START skips.
#[test]
fn a_knockout_is_replayed_and_the_winner_poses_alone() {
    let Some(a) = assets() else { return };
    let mut b = bout(&a, [MODEL_71, MODEL_79], [Some((2, 9)), Some((2, 9))], false);
    let (mut ko_frames, mut replay_frames, mut result_frames) = (0, 0, 0);
    let mut views = Vec::new();
    for _ in 0..60 * 60 * 6 {
        b.step([Pad::default(); 2]);
        if b.events.contains(&Announcement::TimeUp) {
            // a time up has no replay: try the next round
            ko_frames = -1000;
        }
        match b.phase {
            Phase::RoundOver { .. } => ko_frames += 1,
            Phase::Replay { .. } => {
                replay_frames += 1;
                assert!(b.game.display.is_some(), "the replay shows recorded frames");
                assert!(matches!(b.game.camera_mode, kumite_game::CameraMode::Replay));
            }
            Phase::Result { winner: Some(w) } => {
                result_frames += 1;
                assert_eq!(b.game.visible, [w == 0, w == 1], "only the winner is drawn");
                assert!(matches!(b.game.camera_mode, kumite_game::CameraMode::End { who, .. } if who == w));
                views.push(b.game.camera.view);
            }
            Phase::Call if ko_frames > 0 => break,
            Phase::Call if ko_frames < 0 => (ko_frames, replay_frames, result_frames) = (0, 0, 0),
            _ => {}
        }
    }
    assert_eq!(ko_frames, 90, "90 frames after the K.O.");
    assert!(replay_frames >= 2 * 150 - 4, "half speed, at least 150 recorded frames: {replay_frames}");
    assert!(result_frames > 10 && result_frames <= 300, "the win pose: {result_frames}");
    assert!(views.windows(2).any(|w| w[0] != w[1]), "the winner camera moves");
    assert!(b.game.visible == [true, true] && b.game.display.is_none(), "the next round shows both again");
    // START skips the next replay and, with rounds to go, the pose
    for _ in 0..60 * 60 * 3 {
        b.step([Pad::default(); 2]);
        if let Phase::Replay { .. } = b.phase {
            b.skip();
            b.step([Pad::default(); 2]);
            assert!(!matches!(b.phase, Phase::Replay { .. } | Phase::Result { .. }) || b.wins.iter().any(|&w| w >= 2), "{:?}", b.phase);
            break;
        }
    }
}

/// Online, START comes in the pads (every computer's same frame): pressed during the replay, it skips it as the
/// local key does; held on, it does not skip again.
#[test]
fn start_in_the_pads_skips_the_replay() {
    let Some(a) = assets() else { return };
    let mut b = bout(&a, [MODEL_71, MODEL_79], [Some((2, 9)), Some((2, 9))], false);
    // (one round to win: the first replay decides the match, so the win pose follows it)
    b.rounds_to_win = 1;
    let start = Pad { start: true, ..Pad::default() };
    for _ in 0..60 * 60 * 6 {
        b.step([Pad::default(); 2]);
        if b.events.contains(&Announcement::TimeUp) {
            continue;
        }
        if let Phase::Replay { .. } = b.phase {
            b.step([Pad::default(), start]);
            b.step([Pad::default(), start]);
            assert!(!matches!(b.phase, Phase::Replay { .. }), "{:?}", b.phase);
            // held on into the win pose: it is not skipped again (a new press would)
            assert!(matches!(b.phase, Phase::Result { .. }), "the deciding round's win pose: {:?}", b.phase);
            let before = b.phase;
            for _ in 0..5 {
                b.step([Pad::default(), start]);
            }
            assert_eq!(b.phase, before, "held, not pressed again");
            b.step([Pad::default(); 2]);
            b.step([Pad::default(), start]);
            b.step([Pad::default(); 2]);
            assert_ne!(b.phase, before, "pressed again: the pose ends");
            return;
        }
    }
    panic!("no replay");
}

/// The intro variant counter runs on across rounds; the replay and winner cameras' views are finite and look at
/// the fighter.
#[test]
fn end_cameras_aim_at_the_fighter() {
    let Some(a) = assets() else { return };
    let b = bout(&a, [MODEL_71, MODEL_79], [None, None], false);
    let g = &b.game;
    let data = g.camera.data().clone();
    let p = g.subject(0);
    let mut seed = 5i64;
    let mut random = |n: i64| {
        seed = (seed * 1103515245 + 12345) & 0x7FFF_FFFF;
        (seed >> 16) % n.max(1)
    };
    for b_variant in [false, true] {
        let mut cam = kumite_sim::camera::EndCam::winner(&data, &p, b_variant, &mut random);
        let v = cam.step(&data, &p);
        let (eye, fwd, _) = kumite_game::view_camera(&v);
        let root = glam::Vec3::new(p.root[0] as f32, -p.root[1] as f32, -p.root[2] as f32) / kumite_game::UNITS;
        let to = (root - eye).normalize();
        assert!(fwd.dot(to) > 0.9, "winner camera {b_variant}: looks at the fighter ({fwd:?} vs {to:?})");
        let d = ((v.eye[0] - p.root[0]).pow(2) + (v.eye[2] - p.root[2]).pow(2)) as f64;
        assert!(d.sqrt() > 2000.0 && d.sqrt() < 8000.0, "distance {}", d.sqrt());
    }
    let mut over = kumite_sim::camera::EndCam::loser(&data, &p, true, &mut random);
    let v = over.step(&data, &p);
    assert_eq!((v.pitch, v.eye[1]), (0x400, -7853), "the overhead shot looks straight down");
}

/// The render camera keeps the view's pitch sign: 4075 (a little up) is not "straight down", and a view straight
/// down (the loser shot) still has a defined direction.
#[test]
fn render_camera_pitch_wraps() {
    use kumite_sim::camera::View;
    let up = kumite_game::view_camera(&View { eye: [0, -737, -11517], pitch: 4075, yaw: 0, h: 500 }).1;
    assert!(up.y > 0.0 && up.y < 0.1 && up.z < -0.99, "looks ahead and a little up: {up:?}");
    let down = kumite_game::view_camera(&View { eye: [0, -7853, 0], pitch: 0x400, yaw: 0, h: 500 }).1;
    assert!(down.y < -0.99 && down.is_finite(), "straight down: {down:?}");
    let level = kumite_game::view_camera(&View { eye: [0, -1000, -5000], pitch: 0, yaw: 0, h: 500 }).1;
    assert!(level.y.abs() < 1e-6);
}

/// The backdrop drum is a world object 8 x its raw units (stage 0: a 19.2 m box), its base 460 units below the
/// floor, drawn after the floor (stage 11 before it).
#[test]
fn the_backdrop_drum_has_the_games_scale() {
    let Some(a) = assets() else { return };
    let st = a.stage(0).unwrap();
    assert!((st.sky_base_y - 0.46).abs() < 1e-6 && !st.sky_first);
    let xs: Vec<f32> = st.sky.iter().flat_map(|m| m.vertices.iter().map(|v| v.pos[0])).collect();
    let width = xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
    assert!((width - 19.2).abs() < 1.0, "drum width {width} m");
    assert!(a.stage(11).unwrap().sky_first);
}

/// Win poses speak: their timed sound script (type 8) plays a whoosh and the character's win voice at the
/// script's frames (character 0's 0xD68: frame 37 whoosh, frame 108 voice category 3).
#[test]
fn win_poses_play_their_sound_scripts() {
    let Some(a) = assets() else { return };
    let sound = kumite_game::sound::SoundData::load(&a).unwrap();
    let mut b = bout(&a, [MODEL_71, MODEL_79], [None, None], false).free_fight();
    for _ in 0..5 {
        b.step([Pad::default(); 2]);
    }
    assert!(b.game.fight.fighters[0].play_alias(0xD68));
    let mut last = kumite_game::sound::SoundState::default();
    let mut heard = Vec::new();
    for f in 0..130 {
        b.step([Pad::default(); 2]);
        for s in sound.step(&b, &mut last, ["m071", "m079"], [0, 1]) {
            heard.push((f, s.file.display().to_string()));
        }
    }
    eprintln!("{heard:?}");
    assert!(heard.iter().any(|(f, p)| (100..115).contains(f) && p.contains("m071")), "Character 0's win voice: {heard:?}");
    assert!(heard.iter().any(|(f, _)| (30..45).contains(f)), "the whoosh: {heard:?}");
}

/// No hit leaves the victim stuck: every damaging move of two characters lands on a walking opponent, who must
/// get back to its own moves (getting up when it tries to) instead of looping in a reaction - transition 0x1A
/// continues a self-cancelling reaction at the next frame rather than restarting it (0x80030424).
#[test]
fn hit_reactions_always_end() {
    let Some(a) = assets() else { return };
    for (attacker, victim) in [(MODEL_71, MODEL_79), (MODEL_79, MODEL_71)] {
        let mut game = Game::load(&a, [attacker, victim]).unwrap();
        let n = game.fight.fighters[0].moveset.moves.len();
        let fwd = parse_script("b").unwrap()[0]; // P2 walks toward P1 (not guarding)
        let tries = [parse_script("f").unwrap()[0], parse_script("u").unwrap()[0], parse_script("1").unwrap()[0], parse_script("d").unwrap()[0], Pad::default()];
        let mut stuck = Vec::new();
        for idx in 0..n {
            let mv = game.fight.fighters[0].moveset.moves[idx].clone();
            if mv.damage <= 0 || mv.attack.is_none() {
                continue;
            }
            game.reset_round();
            game.fight.fighters[1].position.x = game.fight.fighters[0].position.x + 700.0;
            for _ in 0..3 {
                game.step([Pad::default(), fwd]);
            }
            game.fight.fighters[0].play(kumite_sim::Current { bank: Bank::Own, index: idx });
            let mut hit = false;
            for f in 0..600 {
                // after 150 frames the victim tries to get up: forward, up, a punch, down, nothing (cycling)
                let p2 = if f > 150 { tries[(f / 4) % tries.len()] } else { Pad::default() };
                game.step([Pad::default(), p2]);
                hit |= game.fight.hits.iter().any(|h| h.victim == 1 && !h.blocked);
            }
            let v = &game.fight.fighters[1];
            if hit && v.current.bank != Bank::Own {
                stuck.push((mv.anim.clone(), v.animation(&game.fight.data).0.to_string()));
            }
        }
        assert!(stuck.is_empty(), "model {attacker}: victims stuck after {stuck:?}");
    }
}

#[test]
fn a_cloned_bout_resumes_exactly_where_it_was() {
    // debug rewind and save states restore clones: a clone must carry every bit of fight state (CPU too)
    let Some(a) = assets() else { return };
    let mut b = bout(&a, [MODEL_71, MODEL_79], [Some((2, 9)), Some((2, 9))], false);
    for _ in 0..900 {
        b.step([Pad::default(); 2]);
    }
    let saved = b.clone();
    let state = |b: &Bout| {
        let f = &b.game.fight;
        format!("{:?} {:?} {} {:?}", b.phase, b.wins, f.frame, f.fighters.iter().map(|x| (x.health, x.current, x.frame, x.root(), x.yaw)).collect::<Vec<_>>())
    };
    for _ in 0..600 {
        b.step([Pad::default(); 2]);
    }
    let first = state(&b);
    let mut again = saved;
    for _ in 0..600 {
        again.step([Pad::default(); 2]);
    }
    assert_eq!(state(&again), first);
}

#[test]
fn winning_a_round_against_the_final_boss_stops_the_match_for_the_scene_and_it_goes_on_after() {
    let Some(a) = assets() else { return };
    let mut b = bout(&a, [MODEL_71, 183], [None, Some((1, 9))], false);
    b.interlude_check = true;
    let mut ko = false;
    for _ in 0..3000 {
        if b.phase == Phase::Fight && !ko {
            b.game.fight.fighters[1].health = 0;
            ko = true;
        }
        b.step([Pad::default(); 2]);
        if b.phase == Phase::Interlude {
            break;
        }
    }
    assert_eq!((b.phase, b.wins, b.round), (Phase::Interlude, [1, 0], 1));
    // the next round of the same match (character 20 is loaded by the app in between)
    b.resume_match(1, [1, 0]);
    assert_eq!((b.phase, b.wins, b.round), (Phase::Call, [1, 0], 2));
}

#[test]
fn the_boss_change_scene_runs_its_script_and_camera() {
    use kumite_game::scene::{BossChangeData, BossChangeScene, SceneEvent};
    let Some(a) = assets() else { return };
    let data = BossChangeData::load(&a).unwrap();
    // Character 0's player sees character 13's body (costume 0: model 175); a character 13 player sees character 9's (143)
    assert_eq!((data.body_model(0, 0), data.body_model(13, 0), data.body_model(0, 1)), (Some(175), Some(143), Some(179)));
    let game = Game::load(&a, [175, 183]).unwrap();
    let mut s = BossChangeScene::new(game, data, 0);
    let (mut music, mut frames) = (Vec::new(), 0);
    let mut at = std::collections::HashMap::new();
    while !s.done && frames < 400 {
        s.step();
        frames += 1;
        music.extend(s.events.iter().copied());
        at.insert(frames, (s.game.visible, s.stage_visible, s.white, s.camera(), s.game.fight.fighters.each_ref().map(|f| (f.frame, f.root()))));
    }
    // 1 black frame, scene frames 0..=256, 4 white frames
    assert_eq!(frames, 1 + 257 + 4);
    assert_eq!(music, vec![SceneEvent::Music(5)]);
    // scene frame c = step - 2: nobody before c = 10, then move frame c - 9
    assert_eq!(at[&11].0, [false, false]);
    assert_eq!(at[&12].0, [true, true]);
    assert_eq!(at[&12].4[0].0, 1);
    // the fade: grey 4 (c - 192); the stage goes at c = 245; full white at the end
    assert_eq!(at[&(2 + 224)].2, 128);
    assert!(at[&(2 + 244)].1 && !at[&(2 + 245)].1);
    assert_eq!(at[&frames].2, 255);
    // the camera: key 200 looks at (-751, -1783, -438) game units, where the lifted body is
    let (_, target) = at[&(2 + 200)].3;
    assert!((target - glam::Vec3::new(-0.751, 1.783, 0.438)).length() < 0.002, "{target}");
    let body = at[&(2 + 200)].4[0].1;
    assert!((glam::Vec2::new(body.x, body.z) / 1000.0 - glam::Vec2::new(target.x, target.z)).length() < 0.6, "body {body} vs target {target}");
}

#[test]
fn a_reversal_catches_a_jab_and_deals_half_its_damage_plus_25() {
    let Some(a) = assets() else { return };
    // P1 character 0's b+1+3 stance catches the jab of P2 (reversal cond 0x49): the two are paired and the reversal's
    // timed damage is the jab's 5 / 2 + 25
    let mut paired = false;
    let (g, hits) = duel(&a, [MODEL_71, MODEL_71], "f*30,n*14,b+1+3,n*150", "n*44,1,n*150", true, |_, g| {
        paired |= g.fight.link.is_some();
    });
    assert!(paired, "the reversal pairs the fighters");
    assert!(hits.iter().all(|h| h.attacker == 0), "the jab never lands: {hits:?}");
    assert_eq!(g.fight.fighters[1].health / ONE, 130 - 27, "{hits:?}");
}

#[test]
fn law_parries_a_low_kick() {
    let Some(a) = assets() else { return };
    // P1 character 1's low parry (d+1+3, cond 0x48) against character 0's low d+4: no damage, the kicker staggers
    let parried = (0..12).find_map(|d| {
        // the parry stance (common_317), then a parry pair
        let (mut stance, mut paired) = (false, false);
        let (g, hits) = duel(&a, [MODEL_79, MODEL_71], &format!("f*30,n*{},d+1+3,n*80", 10 + d), "n*40,d+4,n*80", true, |_, g| {
            stance |= g.fight.fighters[0].animation(&g.fight.data).0 == "common_317";
            paired |= stance && g.fight.link.is_some();
        });
        let seen = paired;
        (seen && hits.iter().all(|h| h.attacker == 0)).then_some(g.fight.fighters[0].health / ONE)
    });
    assert_eq!(parried, Some(130), "some timing parries the low");
}

#[test]
fn a_juggled_fighter_tech_rolls_with_a_button_on_landing() {
    let Some(a) = assets() else { return };
    // juggle P2 as above, then press 1 (or 3) in the juggle fall's frames 38..40: a side roll instead of lying
    for button in [1u8, 4] {
        let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_71]);
        for pad in parse_script("f*40").unwrap() {
            g.step([pad, Pad::default()]);
        }
        g.fight.fighters[1].play(kumite_sim::Current { bank: Bank::Common, index: common_index(&a, "common_178") });
        let launch = kumite_sim::Current { bank: Bank::Common, index: common_index(&a, "common_162") };
        g.fight.fighters[1].play(launch);
        let mut rolled = None;
        let mut fall = Vec::new();
        for pad in parse_script("n*2,1,n*120").unwrap() {
            let f = &g.fight.fighters[1];
            let (anim, frame) = f.animation(&g.fight.data);
            fall.push(format!("{anim}@{frame}"));
            // the fall's move frame 38 = animation frame 37
            let p2 = if anim == "common_178" && frame == 37 { Pad { buttons: button, ..Pad::default() } } else { Pad::default() };
            g.step([pad, p2]);
            let shown = g.fight.fighters[1].animation(&g.fight.data).0.to_string();
            if shown == "common_209" || shown == "common_212" {
                rolled = Some(shown);
            }
        }
        assert!(rolled.is_some(), "button {button}: no tech roll; P2 went {:?}", fall.iter().step_by(4).collect::<Vec<_>>());
    }
}

#[test]
fn jins_move_0x241_spikes_an_airborne_opponent_into_the_floor() {
    let Some(a) = assets() else { return };
    // the air reaction table's spike (alias 0xD61) for the move the code names (character 9, alias 0x241)
    let spike = moveset(&a, "moveset_14.json").aliases.as_ref().unwrap()[0xD61];
    let common = moveset(&a, "common.json");
    let spike_anim = common.moves[spike.index.unwrap()].anim.clone();
    let launch = kumite_sim::Current { bank: Bank::Common, index: common_index(&a, "common_162") };
    let found = (0..24).find_map(|delay| {
        let mut g = load_vs_dummy(&a, [143, MODEL_71]);
        for pad in parse_script("f*40").unwrap() {
            g.step([pad, Pad::default()]);
        }
        g.fight.fighters[0].play_alias(0x241);
        for k in 0..80 {
            if k == delay {
                g.fight.fighters[1].play(launch);
            }
            g.step([Pad::default(); 2]);
            if !g.fight.hits.is_empty() {
                let f = &g.fight.fighters[1];
                let anim = f.animation(&g.fight.data).0.to_string();
                // 400 units a frame straight down
                let height = f.position.y + f.anim_root_y;
                for _ in 0..(height / 400.0).ceil() as usize + 1 {
                    g.step([Pad::default(); 2]);
                }
                return Some((anim, g.fight.fighters[1].air.is_none()));
            }
        }
        None
    });
    let (anim, landed) = found.expect("the move connects for some launch timing");
    assert_eq!(anim, spike_anim, "spiked");
    assert!(landed, "a spike drops 400 units a frame to the floor");
}

#[test]
fn two_throws_on_the_same_frame_go_to_the_frame_parity() {
    let Some(a) = assets() else { return };
    // both Pauls press 1+3 together, close: one throws the other; one frame later the other one wins
    let winner = |wait: usize| {
        let (g, _) = duel(&a, [MODEL_71, MODEL_71], &format!("f*30,n*{wait},1+3,n*200"), &format!("n*30,n*{wait},1+3,n*200"), true, |_, _| {});
        let thrown: Vec<bool> = g.fight.fighters.iter().map(|f| f.health < f.max_health * ONE).collect();
        thrown
    };
    let (w1, w2) = (winner(14), winner(15));
    assert!(w1.iter().filter(|&&t| t).count() == 1 && w2.iter().filter(|&&t| t).count() == 1, "one is thrown: {w1:?} {w2:?}");
    assert_ne!(w1, w2, "the parity decides");
}

#[test]
fn a_close_range_sweet_spot_is_a_clean_hit() {
    let Some(a) = assets() else { return };
    // P1 lies face-up (get-up menu 0x9D) with P2 walking up to its feet; its mid get-up kick (4) has a
    // close-range sweet spot (under 1100 units): clean, 1.5x without the zone scaling
    let (_, hits) = duel(&a, [MODEL_71, MODEL_71], "f*20,n*20,4,n*60", "b*45,n*60", false, |f, g| {
        if f == 30 {
            g.fight.fighters[0].play_alias(0x9D);
        }
    });
    let h = hits.iter().find(|h| h.attacker == 0).expect("the get-up kick lands");
    assert!(h.clean && h.damage == h.base_damage * 3 / 2, "{h:?}");
}

#[test]
fn the_attract_embu_follows_its_script() {
    use kumite_game::embu::{EmbuData, EmbuEvent, EmbuScene};
    let Some(a) = assets() else { return };
    let data = std::sync::Arc::new(EmbuData::load(&a).unwrap());
    if !a.root.join("characters").join(&data.variants[0].models["12"]).exists() {
        return; // the embu-only meshes come with the character conversion (--with-embu)
    }
    let mut s = EmbuScene::new(&a, data.clone(), 0).unwrap();
    let (mut steps, mut music, mut impact_at, mut white_end) = (0, None, None, 0);
    let mut seen = std::collections::HashMap::new();
    while !s.done && steps < 4000 {
        s.step();
        steps += 1;
        for e in &s.events {
            match e {
                EmbuEvent::Music(id) => music = Some(*id),
                EmbuEvent::GroundImpact(_) if impact_at.is_none() => impact_at = Some(s.frame()),
                _ => {}
            }
        }
        seen.insert(s.frame(), ([s.pose(0).is_some(), s.pose(1).is_some()], s.models));
        white_end = s.white;
    }
    assert_eq!(music, Some(0), "BGM 0");
    assert_eq!(s.frame(), 3542, "it ends at its last script frame");
    assert_eq!(seen[&0], ([true, false], [12, 0]), "Character 6's solo kata");
    assert_eq!(seen[&760].0, [false, false], "the stage alone between shots");
    assert_eq!(seen[&768], ([true, true], [3, 0]), "Character 1 and character 0 together");
    // move 4 (character 1's part of the pair) hits the floor at its frame 66: script frame 768 + 65
    assert_eq!(impact_at, Some(833));
    assert!(white_end >= 250, "it ends in white: {white_end}");
}

/// The angle (degrees) between fighter `i`'s head +x axis (its face) and the line to the midpoint of the
/// opponent's chest and head, in glTF space (the game's +x maps to +x).
fn head_aim_error(g: &Game, i: usize, animated: bool) -> f32 {
    let at = |k: usize, name: &str| {
        let (pose, place) = if animated { g.pose_animated_for_tests(k) } else { g.pose(k) };
        let n = g.bodies[k].character.nodes.iter().position(|n| n.name == name).unwrap();
        place * pose[n]
    };
    let head = at(i, "head");
    let target = (at(1 - i, "chest").w_axis + at(1 - i, "head").w_axis).truncate() / 2.0;
    let face = head.x_axis.truncate().normalize();
    let to = (target - head.w_axis.truncate()).normalize();
    face.dot(to).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The heads follow the opponent in the stance (0x80039074): the face turned to the opponent (more than the
/// animation has it), looking down at a crouching opponent; off outside the live round.
#[test]
fn heads_follow_the_opponent() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_79]);
    for _ in 0..120 {
        g.step([Pad::default(); 2]);
    }
    assert!(g.heads[0].angles().is_none(), "not before the round is live");
    g.head_live = true;
    for _ in 0..120 {
        g.step([Pad::default(); 2]);
    }
    let stand = g.heads[0].angles().expect("P1's head follows in the stance");
    for i in 0..2 {
        let (anim, aimed) = (head_aim_error(&g, i, true), head_aim_error(&g, i, false));
        eprintln!("standing P{}: angles {:?}, off the opponent {anim:.1} deg animated, {aimed:.1} deg aimed", i + 1, g.heads[i].angles());
        assert!(aimed < 5.0, "P{} faces the opponent: {aimed}", i + 1);
    }
    let down = Pad { down: true, ..Pad::default() };
    for _ in 0..120 {
        g.step([Pad::default(), down]);
    }
    let crouch = g.heads[0].angles().expect("still following");
    eprintln!("P2 crouching: P1 {crouch:?}, off {:.1} deg", head_aim_error(&g, 0, false));
    assert!(crouch[1] > stand[1] + 0x200, "looks down: {stand:?} -> {crouch:?}");
}

/// Jumps and hops drift along the facing over their jump window (record +0x16, 0x8003F3A8): Character 0's held u/f
/// jump goes 7.2 units a frame forward, u/b 5.4 back, u straight up; a tapped u/f hops forward.
#[test]
fn jumps_drift_forward_and_back() {
    let Some(a) = assets() else { return };
    let travel = |script: &str| {
        let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_79]);
        g.fight.fighters[1].position.x = 3000.0;
        let x0 = g.fight.fighters[0].root().x;
        for pad in parse_script(script).unwrap() {
            g.step([pad, Pad::default()]);
        }
        g.fight.fighters[0].root().x - x0
    };
    let (f, b, u, hop) = (travel("n*60,u/f*40,n*30"), travel("n*60,u/b*40,n*30"), travel("n*60,u*40,n*30"), travel("n*60,u/f*3,n*60"));
    eprintln!("u/f {f}, u/b {b}, u {u}, hop {hop}");
    assert!((200.0..250.0).contains(&f), "forward jump {f}");
    assert!((-190.0..-150.0).contains(&b), "back jump {b}");
    assert!(u.abs() < 10.0, "neutral jump {u}");
    assert!((100.0..150.0).contains(&hop), "forward hop {hop}");
}

/// Fighters blink (faces.json, 0x80034100): shut 4 frames at a time, open (rand & 0xFE) + 1 frames.
#[test]
fn fighters_blink() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_79]);
    assert!(g.face_data.blink.contains_key(&g.model_id(0)), "Character 0 has a blink rectangle");
    let mut shut = Vec::new();
    for k in 0..600 {
        g.step([Pad::default(); 2]);
        if g.faces[0].closed {
            shut.push(k);
        }
    }
    eprintln!("Character 0's eyes shut at {shut:?}");
    assert_eq!(&shut[..4], &[0, 1, 2, 3]);
    assert!(shut.len() >= 8 && shut.len() < 80, "{} frames shut", shut.len());
}

/// Character 17's pupils follow the opponent (0x800397FC): offsets within ±17 once they move.
#[test]
fn gon_pupils_follow() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [239, MODEL_71]);
    let mut seen = Vec::new();
    for k in 0..300 {
        let pad = if k > 100 { Pad { right: true, ..Pad::default() } } else { Pad::default() };
        g.step([Pad::default(), pad]);
        if k % 30 == 0 {
            seen.push(g.faces[0].pupils);
        }
    }
    eprintln!("Character 17's pupils: {seen:?}");
    assert!(g.face_data.pupils.contains_key(&g.model_id(0)));
    assert!(seen.iter().flatten().flatten().all(|v| v.abs() <= 17));
}

/// Step both scripts together and collect every hit event.
fn run_both(g: &mut Game, p1: &str, p2: &str) -> Vec<kumite_sim::HitEvent> {
    let (a, b) = (parse_script(p1).unwrap(), parse_script(p2).unwrap());
    let mut hits = Vec::new();
    for k in 0..a.len().max(b.len()) {
        g.step([a.get(k).copied().unwrap_or_default(), b.get(k).copied().unwrap_or_default()]);
        hits.extend(g.fight.hits.iter().cloned());
    }
    hits
}

/// A jab trade (0x8004439C judges both hits before any reaction): both take the jab's damage.
#[test]
fn jab_trade_hurts_both() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_71]);
    let hits = run_both(&mut g, "n*60,f*30,n*4,1,n*30", "n*60,b*30,n*4,1,n*30");
    eprintln!("{:?}", hits.iter().map(|h| (h.attacker, h.damage, h.anim.clone())).collect::<Vec<_>>());
    let trade: Vec<_> = hits.iter().filter(|h| h.damage > 0).collect();
    assert!(trade.iter().any(|h| h.attacker == 0) && trade.iter().any(|h| h.attacker == 1), "both jabs land: {hits:?}");
}

/// A K.O. jab collapses the victim at once (0xD66, 0x80045420; the main modes), and health stops at 0.
#[test]
fn ko_jab_drops_at_once() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_79]);
    for _ in 0..60 {
        g.step([Pad::default(); 2]);
    }
    g.fight.fighters[1].health = 3 * ONE;
    let hits = run_both(&mut g, "f*40,n*2,1,n*10", "");
    assert!(hits.iter().any(|h| h.victim == 1 && h.damage > 0), "the jab lands: {hits:?}");
    assert_eq!(g.fight.fighters[1].health, 0);
    let (anim, _) = g.fight.fighters[1].animation(&g.fight.data);
    assert_eq!(anim, "common_219", "the main modes' K.O. collapse (0xD66) at once");
}

/// Every character character 0's jab hits reacts: the reactions a set lacks are borrowed from the opponent's set at match
/// load (0x8002D07C), e.g. Character 1's front hit is character 0's move 280.
#[test]
fn everyone_reacts_to_a_jab() {
    let Some(a) = assets() else { return };
    let roster: serde_json::Value = kumite_sim::data::load(&a.root.join("gamedata/roster.json")).unwrap();
    let mut victims: Vec<(String, usize)> = Vec::new();
    for e in roster["selections"]["entries"].as_array().unwrap() {
        if let (Some(r), Some(n)) = (e["model_record"].as_u64(), e["name"].as_str()) {
            if e["costume"].as_u64() == Some(0) && !victims.iter().any(|v| v.0 == n) {
                victims.push((n.to_string(), r as usize));
            }
        }
    }
    let mut silent = Vec::new();
    for (name, model) in &victims {
        let mut g = load_vs_dummy(&a, [MODEL_71, *model]);
        let stance = g.fight.fighters[1].animation(&g.fight.data).0.to_string();
        let mut reacted = None;
        for pad in parse_script("n*60,f*40,1,n*20").unwrap() {
            g.step([pad, Pad::default()]);
            if g.fight.hits.iter().any(|h| h.victim == 1 && !h.blocked) {
                g.step([Pad::default(); 2]);
                reacted = Some(g.fight.fighters[1].animation(&g.fight.data).0.to_string());
                break;
            }
        }
        // (a high whiffs over character 17 and character 19: no hit, nothing to react to)
        if let Some(anim) = reacted.filter(|a| *a == stance) {
            silent.push(format!("{name}: {anim}"));
        }
    }
    assert!(silent.is_empty(), "no reaction to the jab: {silent:?}");
}

/// Hurt-sphere radii by character (0x8003F048): Character 11's are larger than character 0's.
#[test]
fn hurt_radii_by_character() {
    let Some(a) = assets() else { return };
    let g = Game::load(&a, [MODEL_71, 159]).expect("load");
    let r = |i: usize| g.bodies[i].hurt.iter().map(|h| h.2).sum::<f32>();
    assert!(r(1) > r(0) * 1.3, "Character 11 {} vs character 0 {}", r(1), r(0));
}

/// An air hit's flight (0x8002F074): the fall clip plays from frame 1 and the pop is stretched so it reaches its
/// hold frame before landing (common_178 holds at 37; the flight lasts at least that).
#[test]
fn juggle_flight_reaches_the_hold_frame() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_79]);
    let p1 = parse_script("n*60,f*25,n*12,d/f+1,n*100").unwrap();
    let p2 = parse_script("n*80,u*40,n*100").unwrap();
    let (mut air_frames, mut first, mut landed_at) = (0, None, None);
    for (i, pad) in p1.into_iter().enumerate() {
        g.step([pad, p2.get(i).copied().unwrap_or_default()]);
        let f = &g.fight.fighters[1];
        if f.air.is_some() {
            air_frames += 1;
            first.get_or_insert(f.frame);
        } else if air_frames > 0 && landed_at.is_none() {
            landed_at = Some(f.frame);
        }
    }
    eprintln!("air {air_frames} frames, first frame {first:?}, landed at frame {landed_at:?}");
    assert_eq!(first, Some(1), "the fall starts at frame 1");
    assert!(air_frames >= 37, "{air_frames} frames in the air");
}

/// A jump's take-off and a juggle landing make their system sounds (move-sound table entries 0x53 and 0x4F,
/// 0x80041830 / 0x80040DF4): sys/06 and sys/02.
#[test]
fn jump_and_landing_sounds() {
    let Some(a) = assets() else { return };
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    let mut b = Bout::new(Game::load(&a, [MODEL_71, MODEL_79]).unwrap(), kumite_game::ai_data(&a).unwrap(), 2, false).free_fight();
    b.game.fight.fighters[1].guard_input = false;
    let mut last = kumite_game::sound::SoundState::default();
    let (p1, p2) = (parse_script("n*60,f*25,n*12,d/f+1,n*100").unwrap(), parse_script("n*80,u*40,n*100").unwrap());
    let mut out = Vec::new();
    for (k, pad) in p1.into_iter().enumerate() {
        b.step([pad, p2.get(k).copied().unwrap_or_default()]);
        for snd in s.step(&b, &mut last, ["m071", "m079"], [0, 1]) {
            out.push(snd.file.file_name().unwrap().to_string_lossy().to_string() + "@" + &snd.file.parent().unwrap().file_name().unwrap().to_string_lossy());
        }
    }
    eprintln!("{out:?}");
    assert!(out.contains(&"06.wav@sys".to_string()), "take-off: {out:?}");
    assert!(out.contains(&"02.wav@sys".to_string()), "juggle landing: {out:?}");
}

/// The impact sound follows limb and damage (0x80041CB4): Character 0's jab (5, arm) one of table entries 0x41 / 0x64,
/// his 4 kick (20, leg) entry 0x46.
#[test]
fn impact_sounds_by_limb_and_damage() {
    let Some(a) = assets() else { return };
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    let table: serde_json::Value = kumite_sim::data::load(&a.root.join("gamedata/sound.json")).unwrap();
    let file = |i: usize| s.file(table["sfx_table"]["codes"][i].as_u64().unwrap() as u16, "m071").unwrap().to_path_buf();
    let impact = |script: &str| {
        let mut b = Bout::new(Game::load(&a, [MODEL_71, MODEL_71]).unwrap(), kumite_game::ai_data(&a).unwrap(), 2, false).free_fight();
        b.game.fight.fighters[1].guard_input = false;
        let mut last = kumite_game::sound::SoundState::default();
        let mut out = Vec::new();
        for pad in parse_script(script).unwrap() {
            b.step([pad, Pad::default()]);
            let sounds = s.step(&b, &mut last, ["m071", "m071"], [0, 0]);
            if !b.game.fight.hits.is_empty() {
                out.extend(sounds.into_iter().map(|x| x.file));
            }
        }
        out
    };
    let jab = impact("f*40,n*4,1,n*30");
    assert!(jab.contains(&file(0x41)) || jab.contains(&file(0x64)), "{jab:?}");
    assert!(impact("f*40,n*4,4,n*40").contains(&file(0x46)));
}

#[test]
fn extra_fighters_speak_with_their_own_voices() {
    let Some(a) = assets() else { return };
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    let table: serde_json::Value = kumite_sim::data::load(&a.root.join("gamedata/sound.json")).unwrap();
    for (ch, model) in [(23usize, 900usize), (24, 938)] {
        let Some(row) = table["voice_rows"]["rows"].get(ch).filter(|r| r["character"].as_u64() == Some(ch as u64)) else { continue };
        let bank = format!("m{model:03}");
        for c in row["codes"].as_array().unwrap() {
            let f = s.file(c.as_u64().unwrap() as u16, &bank).unwrap_or_else(|| panic!("{bank}: voice {c} unresolved"));
            assert!(f.starts_with(a.root.join("audio/sfx").join(&bank)));
        }
        // OpenKumite's hurt sounds (0x98 / 0x99: bank 3, notes 74 / 75) find one of its own hit voices
        assert!(s.file(0xE0CE, &bank).is_some(), "{bank}: hurt sound");
    }
    // model 900's move on its motion 97 (30 damage) voices its own attack voice
    let index: MovesetIndex = kumite_sim::data::load(&a.root.join("gamedata/movesets/index.json")).unwrap();
    let Some(file) = index.models.iter().find(|m| m.model_record == 900).map(|m| m.moveset_file.clone()) else { return };
    let ms = moveset(&a, &file);
    let ewgf = ms.moves.iter().find(|m| m.anim.ends_with("m_097") && m.damage == 30).expect("motion 97's move");
    assert!(ewgf.sounds.iter().flatten().any(|e| e.kind == 2), "a voice entry");
}

/// The extra fighters keep their own game's voice rules: an attack without a voice of its own sometimes cries a
/// random voice of the character's list (0x80073C50), and a throw's script plays its own voices and slams (system
/// bank sounds in the fighter's own bank).
#[test]
fn extra_fighters_attacks_cry_random_voices_and_throws_play_their_scripts() {
    let Some(a) = assets() else { return };
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    if !a.root.join("audio/sfx/m900").exists() {
        return;
    }
    let run = |script: &str| {
        let mut b = Bout::new(Game::load(&a, [900, MODEL_71]).unwrap(), kumite_game::ai_data(&a).unwrap(), 2, false).free_fight();
        let mut state = kumite_game::sound::SoundState::default();
        let mut out = Vec::new();
        for pad in parse_script(script).unwrap() {
            b.step([pad, Pad::default()]);
            for x in s.step(&b, &mut state, ["m900", "m071"], [23, 0]) {
                out.push(x.file.strip_prefix(a.root.join("audio/sfx")).unwrap().to_string_lossy().to_string());
            }
        }
        out
    };
    // jabs at the air: voices from the random list (m900 program 0), not on every jab
    let jabs = run(&"1,n*25,".repeat(12));
    let voices = jabs.iter().filter(|f| f.starts_with("m900/") && !f.starts_with("m900/s")).count();
    assert!((1..12).contains(&voices), "{voices} voices in 12 jabs: {jabs:?}");
    // a throw up close: its script's system sounds and the victim's cry
    let throw = run("f*40,n*2,1+3,n*150");
    assert!(throw.iter().any(|f| f.starts_with("m900/s")), "throw: {throw:?}");
    assert!(throw.iter().any(|f| f.starts_with("m071/")), "victim's voice: {throw:?}");
}

/// An extra level's walls hold the fighters: walking back stops at the wall (a blocked column 3 m behind
/// P1 here), and the arena's own walk grid keeps its stage-40 fight spot free.
#[test]
fn arena_walls_stop_the_fighters() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_79]);
    // a 40 x 40 m grid, cells of 0.5 m, blocked for x < -3 m
    let (cell, n) = (0.5f32, 80usize);
    let row: Vec<u8> = (0..n.div_ceil(8)).map(|b| (0..8).fold(0u8, |acc, k| acc | (u8::from(-20.0 + (b * 8 + k) as f32 * cell < -3.0) << k))).collect();
    g.fight.walls = Some(std::sync::Arc::new(kumite_sim::Walls { origin: [-20.0, -20.0], cell, size: [n, n], rows: vec![row; n] }));
    for pad in parse_script("b*600").unwrap() {
        g.step([pad, Pad::default()]);
    }
    let x = g.fight.fighters[0].root().x;
    assert!((-3000.0..-2000.0).contains(&x), "stopped at {x}");
    // the next round keeps them
    g.reset_round();
    assert!(g.fight.walls.is_some());
    // the installed level: its grid frees the fight spot and the start marks
    if let Ok(st) = kumite_assets::Stage::load(&a.root.join("stages/stage_40.glb")) {
        let w = st.walls.expect("walk grid");
        let w = kumite_sim::Walls { origin: w.origin, cell: w.cell, size: w.size, rows: w.rows };
        for x in [-1000.0, 0.0, 1000.0] {
            assert!(!w.blocked(glam::Vec3::new(x, 0.0, 0.0)), "{x}");
        }
        assert!(w.blocked(glam::Vec3::new(0.0, 0.0, 100_000.0)));
        assert!(st.extra_level.as_deref().is_some_and(|n| !n.is_empty()), "the level's name");
        assert!(st.filtered.iter().all(|f| *f));
    }
}

/// After a K.O. the winner keeps control (it can still hit the falling loser, as in the original); the loser's pad
/// does nothing.
#[test]
fn the_winner_can_still_attack_after_the_ko() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_79]);
    for _ in 0..10 {
        g.step([Pad::default(); 2]);
    }
    g.fight.state = RoundState::Ko { winner: 0 };
    let before = [g.fight.fighters[0].current, g.fight.fighters[1].current];
    let jab = parse_script("1").unwrap()[0];
    g.step([jab, jab]);
    for _ in 0..3 {
        g.step([Pad::default(); 2]);
    }
    assert_ne!(g.fight.fighters[0].current, before[0], "the winner's jab");
    assert_eq!(g.fight.fighters[1].current, before[1], "the loser stands");
}

/// With no one to fight (Force's soldiers all down) the player turns to the idle target: the way on.
#[test]
fn with_no_one_left_the_player_faces_the_way_on() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_79]);
    g.fight.fighters[1].inactive = true;
    g.fight.fighters[1].position = glam::Vec3::new(-5000.0, 0.0, 0.0);
    let f = &mut g.fight.fighters[0];
    (f.yaw, f.root_yaw) = (-std::f32::consts::FRAC_PI_2, -std::f32::consts::FRAC_PI_2);
    g.fight.idle_target = Some(glam::Vec3::new(100_000.0, 0.0, 0.0));
    for _ in 0..20 {
        g.step([Pad::default(); 2]);
    }
    let yaw = g.fight.fighters[0].yaw.rem_euclid(std::f32::consts::TAU);
    assert!((yaw - std::f32::consts::FRAC_PI_2).abs() < 0.2, "turned to +x: yaw {yaw}");
    // walking to +x on the screen (this test's fight camera may have +x on the left) takes it on
    let x0 = g.fight.fighters[0].root().x;
    let on = if g.fight.screen_right.x > 0.0 { "f" } else { "b" };
    for pad in parse_script(&format!("{on}*60,n*20")).unwrap() {
        g.step([pad, Pad::default()]);
    }
    assert!(g.fight.fighters[0].root().x > x0 + 300.0, "walked on to +x: {} -> {}", x0, g.fight.fighters[0].root().x);
    assert!(g.fight.facing_right(0) == (g.fight.screen_right.x > 0.0), "forward on the pad is the way on");
}

/// Forward is toward the opponent on the screen (0x80046234: the sides are the pelvises' screen places, +0x1278):
/// with the opponent got past P1's back, P1's forward is the pad's left.
#[test]
fn forward_follows_the_opponent_on_the_screen() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_79]);
    for _ in 0..5 {
        g.step([Pad::default(); 2]);
    }
    assert!(g.fight.facing_right(0) && !g.fight.facing_right(1));
    // the opponent behind P1's back
    g.fight.fighters[1].position = g.fight.fighters[0].position - glam::Vec3::new(800.0, 0.0, 0.0);
    g.step([Pad::default(); 2]);
    g.step([Pad::default(); 2]);
    assert!(!g.fight.facing_right(0) && g.fight.facing_right(1), "sides follow the screen");
}

/// The pads' vibration from a fight (0x800759D4): a jab that lands shakes the victim's small motor (pattern
/// 0x11) and the attacker's both (0x05: large 207 first); the K.O. the loser's (0x16: large 143) and the winner's
/// (0x0B: large 255).
#[test]
fn hits_and_the_ko_rumble_both_pads() {
    let Some(a) = assets() else { return };
    let data = kumite_game::rumble::RumbleData::load(&a);
    assert!(data.is_some(), "rumble.json");
    let mut r = kumite_game::rumble::Rumble::new(data);
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_71]);
    let step = |g: &mut Game, r: &mut kumite_game::rumble::Rumble, pad: Pad| {
        g.step([pad, Pad::default()]);
        let f = &g.fight;
        r.fight(f, &f.data, &[(&f.fighters[0], 0, 1), (&f.fighters[1], 1, 0)]);
        r.tick()
    };
    let mut first_hit = None;
    for pad in parse_script("f*40,n*4,1,n*30").unwrap() {
        let out = step(&mut g, &mut r, pad);
        if first_hit.is_none() && !g.fight.hits.is_empty() {
            first_hit = Some(out);
        }
    }
    let out = first_hit.expect("the jab lands");
    assert_eq!(out[0], (0.75, 207), "attacker: pattern 0x05 (small 0x3F: 6 ticks of 8)");
    assert_eq!(out[1], (1.0, 0), "victim: pattern 0x11, small motor only (0xFF)");
    // the K.O.
    g.fight.fighters[1].health = ONE;
    let mut script = String::new();
    for _ in 0..10 {
        script.push_str("f*40,n*4,1,n*30,");
    }
    let mut ko = None;
    for pad in parse_script(script.trim_end_matches(',')).unwrap() {
        let out = step(&mut g, &mut r, pad);
        if ko.is_none() && g.fight.fighters[1].health <= 0 {
            ko = Some(out);
            break;
        }
    }
    let out = ko.expect("a K.O.");
    assert_eq!(out[1].1, 143, "loser: pattern 0x16");
    assert_eq!(out[0].1, 255, "winner: pattern 0x0B");
}

/// After character 0's throw over the shoulder (1+3, motion_123) character 1 lies on character 0's other side: the pad's direction
/// toward character 1 on the screen is character 0's forward (0x80046234), and the camera does not swing round to put character 0 back on
/// the left (its framing axis follows the screen order).
#[test]
fn after_a_throw_over_the_shoulder_forward_is_toward_the_opponent() {
    let Some(a) = assets() else { return };
    let mut g = load_vs_dummy(&a, [MODEL_71, MODEL_79]);
    for pad in parse_script("f*45,n*2,1+3,n*150").unwrap() {
        g.step([pad, Pad::default()]);
    }
    let on_right = |g: &Game, i: usize| g.fight.fighters[i].side > g.fight.fighters[1 - i].side;
    assert!(on_right(&g, 0), "Character 0 now right of character 1 on the screen");
    assert!(!g.fight.facing_right(0), "Character 0's forward is the pad's left");
    // walking forward (the pad's left) brings character 0 to character 1
    let gap = |g: &Game| (g.fight.fighters[0].root() - g.fight.fighters[1].root()).length();
    let before = gap(&g);
    for _ in 0..40 {
        g.step([Pad { left: true, ..Pad::default() }, Pad::default()]);
    }
    assert!(gap(&g) < before, "closer: {before} -> {}", gap(&g));
}

/// The original's own voices (0x80041600 / 0x80041734 / 0x80040F0C): struck cleanly, a fighter groans (its row's hurt
/// list); attacking, now and then it shouts (the attack list, 1 in 4); K.O.'d, it screams (category 2).
#[test]
fn fighters_groan_shout_and_scream() {
    let Some(a) = assets() else { return };
    let mut script = String::from("f*40");
    for _ in 0..12 {
        script.push_str(",f*20,n*4,1,n*30");
    }
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    let mut b = Bout::new(Game::load(&a, [MODEL_71, MODEL_71]).unwrap(), kumite_game::ai_data(&a).unwrap(), 2, false).free_fight();
    b.game.fight.fighters[1].guard_input = false;
    let mut state = kumite_game::sound::SoundState::default();
    let mut voices = 0;
    let mut ko = false;
    for pad in parse_script(&script).unwrap() {
        if b.game.fight.fighters[1].health <= 20 * ONE && !ko {
            b.game.fight.fighters[1].health = 2 * ONE;
        }
        b.step([pad, Pad::default()]);
        let out = s.step(&b, &mut state, ["m071", "m071"], [0, 0]);
        voices += out.iter().filter(|x| x.file.parent().unwrap().ends_with("m071")).count();
        if b.game.fight.fighters[1].health <= 0 && !ko {
            ko = true;
            assert!(out.iter().any(|x| x.file.parent().unwrap().ends_with("m071")), "a K.O. scream: {out:?}");
        }
    }
    assert!(voices >= 3, "groans and shouts: {voices}");
}

/// An extra fighter's attack sounds are its own (0x80073390 / 0x800733B0): model 935's f+2 swings with its bank's
/// sound 9 on the first active frame and lands with its sound 0.
#[test]
fn an_extra_fighter_swings_and_lands_with_its_own_sounds() {
    let Some(a) = assets() else { return };
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    // (the extra fighters only when converted)
    if !a.root.join("audio/sfx/m935").exists() {
        return;
    }
    // (its voice row: roster.json's)
    let roster: serde_json::Value = serde_json::from_slice(&std::fs::read(a.root.join("gamedata/roster.json")).unwrap()).unwrap();
    let row = roster["selections"]["entries"].as_array().unwrap().iter().find(|e| e["model_record"] == 935).unwrap()["voice_row"].as_u64().unwrap() as usize;
    let mut b = Bout::new(Game::load(&a, [935, MODEL_71]).unwrap(), kumite_game::ai_data(&a).unwrap(), 2, false).free_fight();
    b.game.fight.fighters[1].guard_input = false;
    let mut state = kumite_game::sound::SoundState::default();
    let mut heard = Vec::new();
    for pad in parse_script("f*40,n*4,f+2,n*40").unwrap() {
        b.step([pad, Pad::default()]);
        for x in s.step(&b, &mut state, ["m935", "m071"], [row, 0]) {
            heard.push(x.file.strip_prefix(a.root.join("audio/sfx")).unwrap().display().to_string());
        }
    }
    assert!(heard.iter().any(|f| f == "m935/s09.wav"), "the swing: {heard:?}");
    assert!(heard.iter().any(|f| f == "m935/s00.wav"), "the hit: {heard:?}");
}

/// SePlay names a program's tone by its index (SsUtKeyOnV 0x800885D8), not by key range: system code 0x6198
/// (program 1, tone 8) is sample 24, which the key range would have taken for 23.
#[test]
fn sounds_pick_the_programs_tone_by_index() {
    let Some(a) = assets() else { return };
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    let f = s.file(0x6198, "").expect("a sample");
    assert!(f.ends_with("sys/24.wav"), "{}", f.display());
}

/// The K.O. sound plays once with or without impact entries (0x80040EA0: 0x7044 an arm, 0x7048 a leg): a throw's
/// K.O. sounds too.
#[test]
fn a_throws_ko_has_its_ko_sound() {
    let Some(a) = assets() else { return };
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    let table: serde_json::Value = kumite_sim::data::load(&a.root.join("gamedata/sound.json")).unwrap();
    let code = |i: usize| table["sfx_table"]["codes"][i].as_u64().unwrap() as u16;
    let ko: Vec<std::path::PathBuf> = [0x44, 0x48].iter().filter_map(|&i| s.file(code(i), "m071").map(|p| p.to_path_buf())).collect();
    let mut b = Bout::new(Game::load(&a, [MODEL_71, MODEL_79]).unwrap(), kumite_game::ai_data(&a).unwrap(), 2, false).free_fight();
    b.game.fight.fighters[1].guard_input = false;
    b.game.fight.fighters[1].health = ONE;
    let mut state = kumite_game::sound::SoundState::default();
    let mut heard = false;
    for pad in parse_script("f*45,n*2,1+3,n*150").unwrap() {
        b.step([pad, Pad::default()]);
        let out = s.step(&b, &mut state, ["m071", "m079"], [0, 1]);
        heard |= out.iter().any(|x| ko.contains(&x.file));
    }
    assert!(b.game.fight.fighters[1].health <= 0, "the throw K.O.s");
    assert!(heard, "the K.O. sound");
}

/// A wooden character 15 (picked with down held, 0x800954A8): a clean hit on it sounds one of five wood knocks
/// (0x80041C40: sound-list entries 0x9A..0x9E) instead of the impact.
#[test]
fn a_wooden_fighter_knocks_as_wood() {
    let Some(a) = assets() else { return };
    let s = kumite_game::sound::SoundData::load(&a).unwrap();
    let table: serde_json::Value = kumite_sim::data::load(&a.root.join("gamedata/sound.json")).unwrap();
    const MODEL_191: usize = 191;
    let bank = format!("m{MODEL_191:03}");
    let wood: Vec<std::path::PathBuf> = (0x9A..=0x9E).filter_map(|i| s.file(table["sfx_table"]["codes"][i].as_u64().unwrap() as u16, &bank).map(|p| p.to_path_buf())).collect();
    assert!(!wood.is_empty());
    let mut b = Bout::new(Game::load(&a, [MODEL_71, MODEL_191]).unwrap(), kumite_game::ai_data(&a).unwrap(), 2, false).free_fight();
    b.game.fight.fighters[1].guard_input = false;
    let mut state = kumite_game::sound::SoundState { wooden: vec![false, true], ..Default::default() };
    let mut heard = false;
    for pad in parse_script("f*40,n*4,1,n*30").unwrap() {
        b.step([pad, Pad::default()]);
        heard |= s.step(&b, &mut state, ["m071", &bank], [0, 16]).iter().any(|x| wood.contains(&x.file));
    }
    assert!(heard, "a wood knock");
}

/// A huge count is an error, not terabytes asked for (found by fuzzing).
#[test]
fn a_script_too_long_is_an_error() {
    assert!(parse_script("f*99999999999").is_err());
    assert!(parse_script(&format!("n*{},f", kumite_game::MAX_SCRIPT_FRAMES)).is_err());
    assert_eq!(parse_script(&format!("n*{}", kumite_game::MAX_SCRIPT_FRAMES)).map(|p| p.len()).ok(), Some(kumite_game::MAX_SCRIPT_FRAMES));
}

#[test]
fn a_move_reference_past_the_moves_is_a_load_error() {
    let Some(a) = assets() else { return };
    let dir = a.root.join("gamedata/movesets");
    let common = kumite_sim::data::load::<Moveset>(&dir.join("common.json")).unwrap().moves.len();
    let mut checked = 0;
    for e in std::fs::read_dir(&dir).unwrap() {
        let path = e.unwrap().path();
        if path.file_name().unwrap().to_string_lossy().starts_with("moveset_") {
            kumite_sim::data::load::<Moveset>(&path).unwrap().check(common).unwrap_or_else(|e| panic!("{}: {e:#}", path.display()));
            checked += 1;
        }
    }
    assert!(checked > 0);
    let ms: Moveset = kumite_sim::data::load(&dir.join("moveset_00.json")).unwrap();
    let broken = |bank: Bank, index: usize| {
        let mut ms = ms.clone();
        let r = ms.aliases.as_mut().unwrap().iter_mut().find(|r| r.bank == bank).unwrap();
        r.index = Some(index);
        ms.check(common).is_err()
    };
    assert!(broken(Bank::Own, ms.moves.len()) && !broken(Bank::Own, ms.moves.len() - 1));
    assert!(broken(Bank::Common, common) && !broken(Bank::Common, common - 1));
    let mut empty = ms.clone();
    empty.moves.clear();
    assert!(empty.check(common).is_err());
}

/// The rules the code keeps for particular moves and Ball's sounds, read from the conversion, are the original's
/// (0x80044310, 0x8002EE94, record 8's 0x800B068C / 0x800B16F4 and its SePlay calls).
#[test]
fn the_rules_of_particular_moves_are_the_originals() {
    let Some(a) = assets() else { return };
    let g = Game::load(&a, [MODEL_71, MODEL_79]).expect("load fight");
    let (m, b) = (&g.fight.data.moves, &g.fight.data.ball);
    assert_eq!(m.long_hitstop, Some((0x898, 30)));
    let spikes: Vec<_> = m.air_spikes.iter().map(|s| (s.moveset, s.alias, s.unless_queued.clone())).collect();
    assert_eq!(spikes, [(9, 0x241, vec![]), (16, 0x14A, vec![0x848, 0x849])]);
    let pushes: Vec<_> = b.pushes.iter().map(|p| (p.aliases.clone(), p.push, p.frames, p.cancel)).collect();
    assert_eq!(pushes, [(vec![0x18, 0x1A], 30.0, [3, 9], 0), (vec![0x1E, 0x20], 150.0, [5, 12], 18), (vec![0x1F, 0x21], 0.0, [0, 0], 18)]);
    assert_eq!((b.end(0x1E), b.end(0x21), b.end(0x18)), (Some(18), Some(18), None));
    assert_eq!(b.self_charge, Some((4, 0x181)));
    let s = &b.sounds;
    assert_eq!([s.serve, s.drop, s.bounce, s.pop, s.smash, s.hit[0], s.hit[1], s.guarded, s.bump], [0x4CE8, 0x4CE9, 0x4E6E, 0x7082, 0x4DAF, 0x4DAF, 0x4E6D, 0x7043, 0x704E]);
}
