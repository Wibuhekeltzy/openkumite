//! The Rust fight camera against golden traces of the reference prototype (`assets/tests/camera_golden.json`, written
//! from the disc by kumite_converter, see docs/development/testing.md).
//! Needs the converted `assets/gamedata/camera.json` (or `$OPENKUMITE_ASSETS`) and the traces; skipped when absent.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // tests and tools: a failure stops them

use kumite_sim::camera::{CameraData, FightCamera, Sample};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Deserialize)]
struct Golden {
    scenarios: Vec<Scenario>,
}
#[derive(Deserialize)]
struct Scenario {
    name: String,
    intro: bool,
    variant: usize,
    first_round: bool,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
struct Frame {
    p: [[i64; 3]; 2],
    fss: i64,
    mc: [i64; 2],
    view: [i64; 6],
}

fn root() -> PathBuf {
    std::env::var_os("OPENKUMITE_ASSETS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
}

fn data() -> Option<Arc<CameraData>> {
    match CameraData::load(&root().join("gamedata/camera.json")) {
        Ok(d) => Some(Arc::new(d)),
        Err(e) => {
            eprintln!("skipping: {e:#}");
            None
        }
    }
}

#[test]
fn camera_matches_the_reference_prototype_frame_by_frame() {
    let Some(data) = data() else { return };
    let path = root().join("tests/camera_golden.json");
    if !path.exists() {
        eprintln!("skipping: no {} (kumite_converter writes it from the disc)", path.display());
        return;
    }
    let golden: Golden = kumite_sim::data::load(&path).unwrap();
    assert!(golden.scenarios.len() >= 10);
    for s in &golden.scenarios {
        let mut cam = FightCamera::new(data.clone(), s.intro, s.variant, s.first_round);
        for (f, fr) in s.frames.iter().enumerate() {
            let side = [-100, 100];
            let p = std::array::from_fn(|i| Sample { x: fr.p[i][0], y: fr.p[i][1], z: fr.p[i][2], side: side[i] });
            let v = cam.step(p, fr.fss, fr.mc);
            let got = [v.eye[0], v.eye[1], v.eye[2], v.pitch, v.yaw, v.h];
            assert_eq!(got, fr.view, "scenario {} frame {f}", s.name);
        }
        assert!(!cam.in_intro(), "{}: intro should be over", s.name);
    }
}

#[test]
fn ratan2_quadrants() {
    let Some(d) = data() else { return };
    assert_eq!(d.ratan2(1, 0), 0);
    assert_eq!(d.ratan2(0, 1), 0x400);
    assert_eq!(d.ratan2(-1, 0), 0x800);
    assert_eq!(d.ratan2(0, -1), 0xC00);
    assert_eq!(d.ratan2(100, 100), 0x200);
}
