//! Face texture animation (gamedata/faces.json; EXE 0x80034100, 0x800397FC / 0x80033ECC): every fighter whose
//! model has a blink rectangle blinks, 4 frames shut then (rand & 0xFE) + 1 open, shut on the first frame, held
//! shut by the move (flags24 bit 0x40000, or lying without bit 0x40000000); one character's pupils slide toward the
//! opponent's head. The app copies the rectangles in the fighter's texture when the state changes.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

/// A blink: the eyes' rectangle in the atlas (x, y, w, h) and where the closed eyes are (x, y).
#[derive(Clone, Copy, Debug)]
pub struct Blink {
    pub dest: [u32; 4],
    pub closed: [u32; 2],
}

/// One of the moving pupils' eye strips: columns x..x+w; rows source_y + offset are shown at dest_y (h rows).
#[derive(Clone, Copy, Debug)]
pub struct Pupil {
    pub x: u32,
    pub w: u32,
    pub h: u32,
    pub dest_y: u32,
    pub source_y: u32,
}

/// faces.json by model id ((model record − 71) / 4).
#[derive(Clone, Debug, Default)]
pub struct FaceData {
    pub blink: HashMap<usize, Blink>,
    pub pupils: HashMap<usize, [Pupil; 2]>,
    /// The character whose pupils move (faces.json `pupils.character`).
    pub pupils_character: Option<i64>,
}

impl FaceData {
    pub fn load(gamedata: &Path) -> Self {
        let Some(v) = kumite_fs::read(&gamedata.join("faces.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else {
            return Self::default();
        };
        let u = |v: &Value| v.as_u64().unwrap_or(0) as u32;
        let mut out = Self { pupils_character: v["pupils"]["character"].as_i64(), ..Self::default() };
        for (k, m) in v["models"].as_object().into_iter().flatten() {
            let Ok(id) = k.parse::<usize>() else { continue };
            let b = &m["blink"];
            if b.is_object() {
                out.blink.insert(id, Blink { dest: [0, 1, 2, 3].map(|i| u(&b["dest"][i])), closed: [u(&b["closed"][0]), u(&b["closed"][1])] });
            }
            if let Some(p) = m["pupils"].as_array().filter(|p| p.len() == 2) {
                let pupil = |p: &Value| Pupil { x: u(&p["x"]), w: u(&p["w"]), h: u(&p["h"]), dest_y: u(&p["dest_y"]), source_y: u(&p["source_y"]) };
                out.pupils.insert(id, [pupil(&p[0]), pupil(&p[1])]);
            }
        }
        out
    }
}

/// A fighter's face state.
#[derive(Clone, Copy, Debug, Default)]
pub struct Face {
    timer: i32,
    /// The eyes are shut.
    pub closed: bool,
    /// The moving pupils: each eye's row offset once it has been moved (None: as loaded).
    pub pupils: [Option<i32>; 2],
    cache: [i32; 2],
}

impl Face {
    /// One blink frame (0x80034100); `rand` the game's rand() (called on each reopening).
    pub fn blink(&mut self, forced: bool, rand: &mut impl FnMut() -> u32) {
        if forced {
            self.timer = 2;
            self.closed = true;
        }
        if self.timer == 0 {
            if self.closed {
                self.timer = (rand() & 0xFE) as i32 + 1;
                self.closed = false;
            } else {
                self.timer = 4;
                self.closed = true;
            }
        }
        self.timer -= 1;
    }

    /// The moving pupils (0x80033ECC) for the opponent's head at angle `a` (4096ths of a turn) in his head's frame.
    pub fn pupils(&mut self, a: i32) {
        let e = if a < 0x800 { (-(a / 60)).max(-17) } else { (-((a - 0xF00) / 60)).min(17) };
        let v = [e.max(0), e.min(0)];
        for (k, v) in v.into_iter().enumerate() {
            if v != self.cache[k] {
                self.cache[k] = v;
                self.pupils[k] = Some(v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blinks_four_frames_shut() {
        let mut f = Face::default();
        let mut r = || 9u32; // opens for (9 & 0xFE) + 1 = 9 frames
        let seq: Vec<bool> = (0..20)
            .map(|_| {
                f.blink(false, &mut r);
                f.closed
            })
            .collect();
        assert_eq!(&seq[..4], &[true; 4], "shut on the first frame, for 4 frames");
        assert_eq!(&seq[4..13], &[false; 9]);
        assert!(seq[13]);
    }

    #[test]
    fn pupils_follow_the_angle() {
        let mut f = Face::default();
        f.pupils(0);
        assert_eq!(f.pupils, [None, None], "nothing moves until the offset changes");
        f.pupils(600);
        assert_eq!(f.pupils, [None, Some(-10)]);
        f.pupils(0xF00 - 1200);
        assert_eq!(f.pupils[0], Some(17));
    }
}
