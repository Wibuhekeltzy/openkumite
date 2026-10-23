//! The tag brawl's HUD (OpenKumite's own mode), in the versus HUD's pieces: per team two health bars stacked (each
//! for themselves: two a side), each with its name plate and who plays it, the round gems per team, the timer,
//! and the round texts.

use super::{bar_step, Text, View};
use crate::art::{Art, Tex};
use egui::Color32;
use kumite_game::bout::Announcement;
use kumite_game::tag::{TagGame, TagPhase};

/// The second row of bars sits this far below the first (px).
pub const ROW_DY: f32 = 52.0;

/// Who plays each actor and how it is shown.
#[derive(Clone, Debug, Default)]
pub struct TagInfo {
    pub models: [usize; 4],
    pub costumes: [usize; 4],
    /// "1P".."4P" or "CPU".
    pub labels: [String; 4],
    /// Character names (each for themselves: the winner's text).
    pub names: [String; 4],
    /// Each team has a human.
    pub humans: Vec<bool>,
    /// TAG ARCADE: the pair fought (1..) of how many.
    pub stage: Option<(usize, usize)>,
}

/// Where actor `i`'s bar goes: the side (0 left, 1 right) and the row. Two teams: a side each, a row per member;
/// each for themselves: 1, 2 on top, 3, 4 below.
fn place(g: &TagGame, i: usize) -> (usize, usize) {
    if g.wins.len() == 2 {
        let team = g.melee.team[i];
        (team, g.members(team).position(|m| m == i).unwrap_or(0))
    } else {
        (i % 2, i / 2)
    }
}

#[derive(Clone, Debug, Default)]
pub struct TagHud {
    shown: [i32; 4],
    trail: [i32; 4],
    ko: Option<Announcement>,
    wins_at_start: Vec<u32>,
    round: u32,
    frame: u32,
}

impl TagHud {
    /// `bar_w`: the health bars' width (px).
    pub fn step(&mut self, g: &TagGame, bar_w: i32) {
        self.frame += 1;
        if g.round != self.round {
            self.round = g.round;
            (self.shown, self.trail) = ([0; 4], [0; 4]);
            self.ko = None;
            self.wins_at_start = g.wins.clone();
        }
        for e in &g.events {
            if matches!(e, Announcement::TimeUp | Announcement::Perfect | Announcement::DoubleKo | Announcement::Great | Announcement::Ko) {
                self.ko = Some(*e);
            }
        }
        for (i, f) in g.melee.actors.iter().enumerate() {
            let max = (f.max_health * kumite_sim::ONE) as i64;
            let unit = (max / bar_w.max(1) as i64).max(1);
            let target = ((f.health.max(0) as i64 + unit - 1) / unit).min(bar_w as i64) as i32;
            (self.shown[i], self.trail[i]) = bar_step(self.shown[i], self.trail[i], target);
        }
    }

    pub fn draw(&self, ctx: &egui::Context, art: &Art, d: &super::HudData, g: &TagGame, info: &TagInfo) {
        let v = View::fit(ctx.content_rect());
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("hud"))).with_clip_rect(v.rect);
        let src: &[&str] = &["exe"];
        for i in 0..4 {
            let (team, row) = place(g, i);
            let row = row as f32;
            let [x, y] = d.bar_xy[team];
            let y = y + ROW_DY * row;
            super::draw_bar(d, &painter, &v, art, [x, y], team, self.shown[i], self.trail[i]);
            // the name plate under the bar, and who plays it beside the plate
            let name_y = d.name_y + ROW_DY * row;
            let w = super::name_plate(d, &painter, &v, art, info.models[i], info.costumes[i], d.name_x[team], name_y, team == 1).unwrap_or(60.0);
            let nx = if team == 0 { d.name_x[0] } else { d.name_x[1] - w };
            let label = &info.labels[i];
            let lw = art.text_w(0, label);
            let lx = if team == 0 { nx + w + 10.0 } else { nx - 10.0 - lw };
            let colour = if label == "CPU" { 6 } else { d.push_colour };
            art.text(&painter, &v, 0, colour, lx, name_y + 1.0, label, Color32::WHITE);
        }
        // the timer
        let secs = g.melee.clock.div_ceil(60).min(99) as usize;
        for (k, dg) in [secs / 10, secs % 10].into_iter().enumerate() {
            if let Some(&[u, vv]) = d.timer_uv.get(dg) {
                let t = Tex { page: d.timer_page, bpp: 4, u, v: vv, clut: Some(d.timer_clut) };
                art.sprite(&painter, &v, src, &t, d.timer_size[0], d.timer_size[1], d.timer_x[k], d.timer_y, Color32::WHITE);
            }
        }
        // round gems per team under the timer (each for themselves: by each one's bar row), the newest win blinking
        for t in 0..g.wins.len() {
            let wins = g.wins[t];
            let (side, row) = if g.wins.len() == 2 { (t, 0) } else { place(g, t) };
            for k in 0..g.rounds_to_win {
                let newest = k + 1 == wins && self.wins_at_start.get(t) != Some(&wins);
                let won = k < wins && (!newest || self.frame & 0x10 != 0);
                let gem = Tex { page: d.bar_page, bpp: 4, u: d.gem_uv[0], v: d.gem_uv[1], clut: Some(d.gem_cluts[usize::from(!won)]) };
                art.sprite(&painter, &v, src, &gem, d.gem_size[0], d.gem_size[1], d.gem_x[side] + d.gem_step[side] * k as f32, d.gem_y + ROW_DY * row as f32, Color32::WHITE);
            }
        }
        // TAG ARCADE: which pair of how many
        if let Some((n, of)) = info.stage {
            let t = format!("STAGE {n}/{of}");
            art.text(&painter, &v, 0, 6, crate::art::CENTRE - art.text_w(0, &t) / 2.0, d.gem_y + ROW_DY + 4.0, &t, Color32::WHITE);
        }
        crate::debug::mark_under(&painter);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("hud_front"))).with_clip_rect(v.rect);
        let first = usize::from(g.round != 1);
        match g.phase {
            TagPhase::Call if g.is_final_round() => d.final_round.draw(&painter, &v, art),
            TagPhase::Call => {
                let f = g.phase_frame;
                if f >= d.round_from[first] {
                    let mut t = d.round.clone();
                    t.text = t.text.replace("%1x", &format!("{:x}", g.round));
                    t.draw(&painter, &v, art);
                }
                if f >= d.ready_from[first] {
                    d.ready.draw(&painter, &v, art);
                }
            }
            TagPhase::RoundOver { .. } => {
                let t = match self.ko {
                    Some(Announcement::TimeUp) => &d.time_up,
                    Some(Announcement::Perfect) => &d.perfect,
                    Some(Announcement::DoubleKo) => &d.double_ko,
                    _ => &d.ko,
                };
                t.draw(&painter, &v, art);
            }
            TagPhase::Result { winner } | TagPhase::MatchOver { winner } => {
                let Some(w) = winner else {
                    d.draw.draw(&painter, &v, art);
                    return;
                };
                // one side with humans: YOU WIN / YOU LOSE; else who won
                if info.humans.iter().filter(|&&h| h).count() == 1 {
                    (if info.humans[w] { &d.you_win } else { &d.you_lose }).draw(&painter, &v, art);
                    return;
                }
                let name = if g.wins.len() == 2 { format!("TEAM {}", w + 1) } else { info.names[w].clone() };
                let mut t: Text = d.wins.clone();
                t.text = t.text.replace("%s", &name);
                let [centre, per_char, offset] = d.wins_x;
                t.x = centre - (per_char * name.len() as f32 + offset);
                t.draw(&painter, &v, art);
            }
            TagPhase::Fight => {}
        }
    }
}
