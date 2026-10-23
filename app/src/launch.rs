//! Debugging from the command line: `--mode NAME` starts a mode straight away, past the menus, with the fighters
//! and costumes, the stage, the difficulty and how far into the mode (an arcade or Time Attack stage, the Survival
//! wins, a Tag Arcade pair); for Force its stage (the original's four, the final one, OpenKumite's extra
//! ones), straight to the boss, the keys held and a co-op partner.
//!
//! ```text
//! --mode force --player NAME --force-stage 5 --boss
//! --mode arcade --player 0 --costume 2 --opponent 14 --level 10 --difficulty hard
//! --mode tag-free --fighters NAME,ID,PREFIX:NAME,ID:2 --stage ID
//! ```

use crate::menu::Roster;
use anyhow::{bail, Context, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    Arcade,
    Versus,
    Practice,
    TimeAttack,
    Survival,
    Ball,
    Force,
    TagFree,
    TagTeams,
    TagArcade,
    ExtraLevels,
}

impl LaunchMode {
    /// (flag value, mode)
    pub const NAMES: [(&'static str, LaunchMode); 11] = [
        ("arcade", LaunchMode::Arcade),
        ("vs", LaunchMode::Versus),
        ("practice", LaunchMode::Practice),
        ("time-attack", LaunchMode::TimeAttack),
        ("survival", LaunchMode::Survival),
        ("ball", LaunchMode::Ball),
        ("force", LaunchMode::Force),
        ("tag-free", LaunchMode::TagFree),
        ("tag-teams", LaunchMode::TagTeams),
        ("tag-arcade", LaunchMode::TagArcade),
        ("levels", LaunchMode::ExtraLevels),
    ];
}

/// What to start.
#[derive(Clone, Debug)]
pub struct LaunchSpec {
    pub mode: LaunchMode,
    /// Roster index and costume: P1, P2 (the opponent; Practice: the dummy; Force: the co-op partner), the tag
    /// brawl's 3rd and 4th (Ball 2 ON 2: P1's and P2's partners).
    pub players: [(usize, usize); 4],
    /// Which of `players` were given (the others are the mode's own choice).
    pub given: [bool; 4],
    /// A stage id (stages.json), or None: the mode's own.
    pub stage: Option<usize>,
    pub difficulty: usize,
    /// Arcade / Time Attack stage, Tag Arcade pair (0-based); Survival: the wins so far.
    pub level: usize,
    /// P2 played by the CPU (VS, Ball; the tag brawl: every slot past the first, the arcade's 2P).
    pub p2_cpu: bool,
    /// Force: the stage's number less one (past the original's stages, the extra stages); None: the final stage.
    pub force_stage: Option<usize>,
    pub force_boss: bool,
    pub force_coop: bool,
    pub force_keys: u32,
}

/// A character by id, or by name (case, spaces, dots and dashes aside; one without a prefix before one of the
/// same name with a prefix, roster.json `prefix`: `PREFIX:NAME` picks that one), with `:COSTUME` (1..) after it.
pub fn fighter(roster: &Roster, spec: &str) -> Result<(usize, usize)> {
    let (name, costume) = match spec.rsplit_once(':').filter(|(_, c)| c.parse::<usize>().is_ok()) {
        Some((n, c)) => (n, c.parse::<usize>().unwrap_or(1).max(1) - 1),
        None => (spec, 0),
    };
    let norm = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let (prefix, name) = match name.split_once(':').filter(|(p, _)| roster.characters.iter().any(|c| !c.prefix.is_empty() && c.prefix == *p)) {
        Some((p, n)) => (p, n),
        None => ("", name),
    };
    let want = norm(name);
    let found: Vec<usize> = roster.characters.iter().enumerate().filter(|(_, c)| name.parse::<usize>().ok() == Some(c.id) || norm(&c.name) == want).map(|(i, _)| i).collect();
    // (the asked prefix's fighter first; a name only one has needs no prefix)
    let idx = found.iter().copied().find(|&i| roster.characters[i].prefix == prefix).or_else(|| found.first().copied().filter(|_| prefix.is_empty()));
    let idx = idx.with_context(|| {
        let names: Vec<String> = roster.characters.iter().map(|c| if c.prefix.is_empty() { norm(&c.name) } else { format!("{}:{}", c.prefix, norm(&c.name)) }).collect();
        format!("no character {spec:?} (names: {})", names.join(", "))
    })?;
    Ok((idx, costume.min(roster.characters[idx].costumes.len().saturating_sub(1))))
}

/// The launch the command line asks for (`--mode`), or None.
pub fn from_args(args: &[String], roster: &Roster) -> Result<Option<LaunchSpec>> {
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let Some(m) = arg("--mode") else { return Ok(None) };
    let names: Vec<&str> = LaunchMode::NAMES.iter().map(|n| n.0).collect();
    let mode = LaunchMode::NAMES.iter().find(|n| n.0 == m).map(|n| n.1).with_context(|| format!("--mode {m}: one of {}", names.join(", ")))?;
    let mut players = [(0, 0), (1, 0), (2, 0), (3, 0)];
    let mut given = [false; 4];
    // the fighters: --fighters A,B,C,D, or --player / --opponent with --costume / --costume2
    if let Some(list) = arg("--fighters") {
        for (k, f) in list.split(',').filter(|f| !f.is_empty()).take(4).enumerate() {
            (players[k], given[k]) = (fighter(roster, f)?, true);
        }
    }
    for (k, name, costume) in [(0, "--player", "--costume"), (1, "--opponent", "--costume2")] {
        if let Some(p) = arg(name) {
            players[k] = fighter(roster, &p)?;
            given[k] = true;
        }
        if let Some(c) = arg(costume) {
            let c: usize = c.parse().with_context(|| format!("{costume} 1.."))?;
            let n = roster.characters.get(players[k].0).map_or(1, |c| c.costumes.len().max(1));
            players[k].1 = c.clamp(1, n) - 1;
        }
    }
    let difficulty = match arg("--difficulty").as_deref() {
        None => 1,
        Some("easy") | Some("0") => 0,
        Some("medium") | Some("1") => 1,
        Some("hard") | Some("2") => 2,
        Some(d) => bail!("--difficulty {d}: easy, medium or hard (0..2)"),
    };
    let level = match arg("--level") {
        Some(l) => {
            let n: usize = l.parse().context("--level N")?;
            if mode == LaunchMode::Survival {
                n
            } else {
                n.max(1) - 1
            }
        }
        None => 0,
    };
    let stage = arg("--stage").map(|s| s.parse::<usize>().context("--stage ID (stages.json)")).transpose()?;
    // Force: --force-stage N (1.., the extra stages after the original's), or final
    let force_stage = match arg("--force-stage").as_deref() {
        None => Some(0),
        Some("final") => None,
        Some(n) => match n.parse::<usize>() {
            Ok(n @ 1..) => Some(n - 1),
            _ => bail!("--force-stage {n}: 1.. or final"),
        },
    };
    let p2_cpu = match mode {
        LaunchMode::Versus => flag("--p2-cpu"),
        LaunchMode::TagArcade => flag("--p2-cpu"),
        _ => !flag("--p2-human"),
    };
    Ok(Some(LaunchSpec {
        mode,
        players,
        given,
        stage,
        difficulty,
        level,
        p2_cpu,
        force_stage,
        force_boss: flag("--boss"),
        force_coop: flag("--coop"),
        force_keys: arg("--force-keys").and_then(|k| k.parse().ok()).unwrap_or(0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> Option<Roster> {
        let gd = crate::test_assets().join("gamedata");
        kumite_fs::exists(&gd.join("screens.json")).then(|| Roster::load(&gd))
    }

    fn args(s: &str) -> Vec<String> {
        s.split(' ').map(String::from).collect()
    }

    /// `s` with each `{N}` the name of character N (roster.json's, as a player types it).
    fn named(r: &Roster, s: &str) -> Vec<String> {
        args(s).into_iter().map(|a| r.characters.iter().fold(a, |a, c| a.replace(&format!("{{{}}}", c.id), &c.name.to_lowercase()))).collect()
    }

    #[test]
    fn the_command_line_names_a_mode_its_fighters_and_how_far_in() {
        let Some(r) = roster() else { return };
        let id = |p: (usize, usize)| (r.characters[p.0].id, p.1);
        let s = from_args(&named(&r, "--mode force --player {7} --costume 2 --force-stage 5 --boss --force-keys 3"), &r).unwrap().unwrap();
        assert_eq!((s.mode, id(s.players[0]), s.force_stage, s.force_boss, s.force_keys), (LaunchMode::Force, (7, 1), Some(4), true, 3));
        let s = from_args(&named(&r, "--mode arcade --player 0 --opponent {14} --level 10 --difficulty hard"), &r).unwrap().unwrap();
        assert_eq!((id(s.players[0]).0, id(s.players[1]).0, s.level, s.difficulty, s.given[1]), (0, 14, 9, 2, true));
        assert_eq!(from_args(&args("--mode force --force-stage final"), &r).unwrap().unwrap().force_stage, None);
        assert!(from_args(&args("--mode nothing"), &r).is_err());
        assert!(from_args(&args("--fight"), &r).unwrap().is_none());
        // a fighter of the same name by its prefix, a costume after a colon
        let twin = r.characters.iter().find(|c| !c.prefix.is_empty() && r.by_id(0).is_some_and(|c0| c.id != c0.id && c.name.eq_ignore_ascii_case(&c0.name)));
        if let (Some(twin), Some(c33)) = (twin, r.by_id(33)) {
            let line = format!("--mode tag-free --fighters {{0}},{}:{{0}},{{11}}:2,{}", twin.prefix, c33.name.to_lowercase());
            let s = from_args(&named(&r, &line), &r).unwrap().unwrap();
            let ids: Vec<(usize, usize)> = s.players.iter().map(|&p| id(p)).collect();
            assert_eq!(ids[0], (0, 0));
            assert_eq!(r.characters[s.players[1].0].id, twin.id);
            assert_eq!(ids[2].1, 1);
            assert_eq!(ids[3].0, 33, "a prefixed name without the prefix");
            assert_eq!(s.given, [true; 4]);
        }
    }
}
