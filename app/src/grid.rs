//! Select screen B (EXE mode 10, 0x80055590): the 7x3 grid select of VS MODE. Both players move and pick at the
//! same time, then set their LIFE handicap; the win tally of the VS session sits in the middle. Drawn
//! like the original from screens.json `grid` (backdrop quads in the mode colours, scrolling title letters and
//! banner, framed faces, blinking cursors with 1P / 2P tags, names with costume bars, handicap gauges).

use crate::art::{additive, int as u, modulate, num as n, pair as xy, Art, Screen as View, Tex};
use crate::menu::{sfx, FightSetup, Key, Mode, Outcome, Roster};
use egui::Color32;
use serde_json::Value;

fn rgb(v: &Value) -> Option<Color32> {
    v.as_array().map(|c| Color32::from_rgb(u(&c[0]) as u8, u(&c[1]) as u8, u(&c[2]) as u8))
}

/// A quad: corners (x, y) top-left, top-right, bottom-left, bottom-right; colours (None = the mode colour).
#[derive(Clone, Debug, Default)]
struct Quad {
    xy: [f32; 8],
    colours: [Option<Color32>; 4],
}

impl Quad {
    fn from(v: &Value) -> Self {
        Self { xy: std::array::from_fn(|k| n(&v["xy"][k])), colours: std::array::from_fn(|k| rgb(&v["colours"][k])) }
    }
    fn rect(&self) -> [f32; 4] {
        [self.xy[0], self.xy[1], self.xy[2] - self.xy[0], self.xy[5] - self.xy[1]]
    }
}

/// Per-player constants (0x80098160).
#[derive(Clone, Debug, Default)]
struct PlayerData {
    frame_set: usize,
    /// The 1P / 2P tag's uv (its page: `GridData::tag`).
    tag: [u32; 2],
    tag_dx: f32,
    default_char: usize,
    name_x: f32,
    strip: [f32; 2],
    side: usize,
    gauge_x: f32,
    gauge_hidden: f32,
    gauge_y: f32,
    life: [f32; 2],
    percent: [f32; 2],
}

/// Ball's balls on the grid's chooser (BEACH, GUM, IRON).
pub const BALL_KINDS: usize = 3;

/// screens.json `grid`.
#[derive(Clone, Debug, Default)]
pub struct GridData {
    cells: Vec<(f32, f32, usize)>,
    colour_a: Color32,
    colour_b: Color32,
    background: Vec<Quad>,
    glows: Vec<Quad>,
    letters: Vec<[u32; 4]>,
    banner: String,
    lines: Vec<(usize, [f32; 4])>,
    line_sets: Vec<[Color32; 3]>,
    players: [PlayerData; 2],
    title: String,
    ticker: String,
    bar_cluts: Vec<[u32; 2]>,
    win_colours: Vec<u32>,
    draw_colours: Vec<u32>,
    pieces: Vec<(f32, [u32; 2], [u32; 2])>,
    levels: Vec<String>,
    /// VS MODE's LIFE handicap: each level's full health in percent.
    percents: Vec<i32>,
    /// The gauge's frame and arrow CLUTs.
    gauge_cluts: [[u32; 2]; 2],
    default_level: usize,
    /// The VS tally's DRAW and the gauges' LIFE.
    draw_text: String,
    life: String,
    /// PUSH P1 START / PUSH P2 START (screens.json hud), a team's size: its format and PLAYER / PLAYERS.
    push: [String; 2],
    team_size: (String, [String; 2]),
    /// Per game mode: the mode colours A / B and the title.
    mode_colours: Vec<(Color32, Color32)>,
    titles: Vec<String>,
    /// The info text while a team's size is set ("%1x %s\n ON TEAM", PLAYER / PLAYERS) and the join text.
    team_strip: [[f32; 2]; 2],
    /// Team Battle's and Ball's rows of the mode colours and titles.
    mode_rows: [usize; 2],
    /// A cell's size.
    cell: [f32; 2],
    /// The VS panel (0x800B2FE0): its box [x0, y0, x1, y1], soft sides' width, the tab below (its top edge
    /// [x0, y0, x1, y1], its bottom edge's ends), the darkening.
    panel: [f32; 4],
    soft: f32,
    tab: [f32; 4],
    tab_bottom: [f32; 2],
    darken: u8,
    /// The banner (font, colour, y, period, speed) and the big letters (page, glyph w, h, scale, y, step, period,
    /// speed).
    banner_text: (usize, u32, f32, i32, i32),
    letters_page: [u32; 2],
    letters_draw: [i32; 7],
    /// A face's place in its cell.
    face_d: [f32; 2],
    /// The names: font, colour, y; the costume bar (x, extra width, y from the name's, height, uv); PUSH START's y
    /// from the name's.
    names: (usize, u32, f32),
    name_bar: [f32; 4],
    name_bar_uv: [u32; 2],
    push_dy: f32,
    /// A cursor's tag: y from the cell's, size, page.
    tag: (f32, [u32; 2], [u32; 2]),
    /// VS between the sides (Team Battle, Ball).
    vs_text: crate::modes::Text,
    /// The tally: wins (x per side, y, font), the dash and DRAW (x, y, font, colour), the draws (x, y, font).
    wins: ([f32; 2], f32, usize),
    dash: ([f32; 2], usize, u32),
    draw_place: ([f32; 2], usize, u32),
    draws: ([f32; 2], usize),
    /// The LIFE gauge (record 5 0x800B2A18) and its slide a frame.
    gauge: Gauge,
    gauge_slide: f32,
    /// The title (font, colour, y, idle frames before the ticker) and the ticker (font, y, colours).
    title_text: (usize, u32, f32, u32),
    ticker_text: (usize, f32, [u32; 2]),
}

/// The LIFE gauge's pieces (screens.json `grid.vs.gauge.draw`).
#[derive(Clone, Debug, Default)]
struct Gauge {
    page: [u32; 2],
    left: f32,
    cap: [u32; 2],
    cap_uv: [[u32; 2]; 2],
    cap_dx: f32,
    middle: [f32; 2],
    middle_uv: [u32; 2],
    middle_texels: u32,
    dark: [f32; 4],
    pieces_x: [f32; 2],
    pieces_y: [f32; 2],
    piece_uv: [u32; 2],
    arrow_dx: [f32; 2],
    arrow: [u32; 2],
    arrow_uv: [u32; 2],
}

/// The y offset of a cell on the second page.
const PAGE: f32 = 1000.0;

/// A cell's page and its y on the screen.
fn on_page(y: f32) -> (usize, f32) {
    let page = (y / PAGE).floor().max(0.0);
    (page as usize, y - page * PAGE)
}

impl GridData {
    /// The full health in percent of LIFE level `level` (100 past the levels).
    pub fn handicap(&self, level: usize) -> i32 {
        self.percents.get(level).copied().unwrap_or(100)
    }

    /// A handicap the LIFE levels allow.
    pub fn allows(&self, handicap: i32) -> bool {
        handicap == 100 || self.percents.contains(&handicap)
    }

    /// The character ids of the grid's cells, row by row (7 a row).
    pub fn cell_ids(&self) -> Vec<usize> {
        self.cells.iter().map(|c| c.2).collect()
    }

    /// One more cell on the extra fighters' pages: row and column of the original's 7 x 3 layout, rows past the third
    /// on the next page. The page's cells keep the original's places with y raised by PAGE per page, so they make the
    /// rows after the original's (the cursor moves on down into them) and draw one page at a time.
    pub fn add_page_cell(&mut self, row: usize, col: usize, id: usize) {
        let rows = self.rows();
        let base: Vec<&Vec<usize>> = rows.iter().filter(|r| r.first().is_some_and(|&i| self.cells[i].1 < PAGE)).collect();
        if base.is_empty() {
            return;
        }
        // (past the page's rows, the next page)
        let (page, row) = (1 + row / base.len(), row % base.len());
        let r = base[row];
        let full = base.iter().max_by_key(|r| r.len()).copied().unwrap_or(r);
        let (Some(&at), Some(&first)) = (full.get(col.min(full.len() - 1)), r.first()) else { return };
        self.cells.push((self.cells[at].0, self.cells[first].1 + PAGE * page as f32, id));
    }

    /// The cells (indices) of each row, rows top to bottom, cells left to right.
    fn rows(&self) -> Vec<Vec<usize>> {
        let mut ys: Vec<f32> = Vec::new();
        for c in &self.cells {
            if !ys.iter().any(|y| (y - c.1).abs() < 1.0) {
                ys.push(c.1);
            }
        }
        ys.sort_by(|a, b| a.total_cmp(b));
        ys.iter()
            .map(|y| {
                let mut r: Vec<usize> = (0..self.cells.len()).filter(|&i| (self.cells[i].1 - y).abs() < 1.0).collect();
                r.sort_by(|&a, &b| self.cells[a].0.total_cmp(&self.cells[b].0));
                r
            })
            .collect()
    }

    /// (row, column) of cell `i`.
    fn place(&self, i: usize) -> (usize, usize) {
        for (r, row) in self.rows().iter().enumerate() {
            if let Some(c) = row.iter().position(|&x| x == i) {
                return (r, c);
            }
        }
        (0, 0)
    }

    pub fn load(screens: &Value) -> Self {
        let g = &screens["grid"];
        let arr = |v: &Value| v.as_array().cloned().unwrap_or_default();
        // (the modes' rows of the mode colour and title tables: the game's battle types)
        let row = |k: &str| u(&g["modes"][k]) as usize;
        let vs = row("vs");
        let player = |p: &Value| PlayerData {
            frame_set: (u(&p["frame_flags"]) & 3) as usize,
            tag: [u(&p["tag_uv"]) & 0xFF, u(&p["tag_uv"]) >> 8],
            tag_dx: n(&p["tag_dx"]),
            default_char: (u(&p["default_sel"]) / 4) as usize,
            name_x: n(&p["name_x"]),
            strip: [n(&p["vs_strip_x"]), n(&p["vs_strip_y"])],
            side: u(&p["side"]) as usize,
            gauge_x: n(&p["gauge_x"]),
            gauge_hidden: n(&p["gauge_hidden_x"]),
            gauge_y: n(&p["gauge_y"]),
            life: [n(&p["life_dx"]), n(&p["life_y"])],
            percent: [n(&p["percent_dx"]), n(&p["percent_y"])],
        };
        let players = [player(&g["players"][0]), player(&g["players"][1])];
        let (vsd, gd) = (&g["vs"], &g["vs"]["gauge"]["draw"]);
        let pair = |v: &Value| [n(&v[0]), n(&v[1])];
        let i = |v: &Value| v.as_i64().unwrap_or(0) as i32;
        let (b, l) = (&g["banner"], &g["letters"]);
        Self {
            cell: [n(&g["cell_w"]), n(&g["cell_h"])],
            panel: [0, 1, 2, 3].map(|k| n(&vsd["panel"]["box"][k])),
            soft: n(&vsd["panel"]["soft"]),
            tab: [0, 1, 2, 3].map(|k| n(&vsd["panel"]["tab"][k])),
            tab_bottom: [0, 1].map(|k| n(&vsd["panel"]["tab_bottom"][k])),
            darken: u(&vsd["panel"]["darken"]).min(255) as u8,
            banner_text: (u(&b["font"]) as usize, u(&b["colour"]), n(&b["y"]), i(&b["period"]), i(&b["speed"])),
            letters_page: xy(&l["page"]),
            letters_draw: ["w", "h", "scale", "y", "step", "period", "speed"].map(|k| i(&l[k])),
            face_d: [n(&g["face"]["dx"]), n(&g["face"]["dy"])],
            names: (u(&g["names"]["font"]) as usize, u(&g["names"]["colour"]), n(&g["names"]["y"])),
            name_bar: [n(&g["names"]["bar_dx"]), n(&g["names"]["bar_dw"]), n(&g["names"]["bar_dy"]), n(&g["names"]["bar_h"])],
            name_bar_uv: [u(&g["names"]["bar_u"]), u(&g["names"]["bar_v"])],
            push_dy: n(&g["names"]["push_dy"]),
            tag: (n(&g["tag"]["dy"]), [u(&g["tag"]["w"]), u(&g["tag"]["h"])], xy(&g["tag"]["page"])),
            vs_text: crate::modes::Text::from(&g["vs_text"]),
            wins: (pair(&vsd["win_x"]), n(&vsd["win_y"]), u(&vsd["win_font"]) as usize),
            dash: (pair(&vsd["dash"]), u(&vsd["dash_font"]) as usize, u(&vsd["dash_colour"])),
            draw_place: ([n(&vsd["draw_text"][1]), n(&vsd["draw_text"][2])], u(&vsd["draw_text_font"]) as usize, u(&vsd["draw_text_colour"])),
            draws: (pair(&vsd["draws"]), u(&vsd["draw_font"]) as usize),
            gauge: Gauge {
                page: xy(&gd["page"]),
                left: n(&gd["left"]),
                cap: [u(&gd["caps"]["w"]), u(&gd["caps"]["h"])],
                cap_uv: [xy(&gd["caps"]["uv"][0]), xy(&gd["caps"]["uv"][1])],
                cap_dx: n(&gd["caps"]["right_dx"]),
                middle: [n(&gd["middle"]["dx"]), n(&gd["middle"]["w"])],
                middle_uv: xy(&gd["middle"]["uv"]),
                middle_texels: u(&gd["middle"]["texels"]),
                dark: [0, 1, 2, 3].map(|k| n(&gd["box"][k])),
                pieces_x: pair(&gd["pieces_x"]),
                pieces_y: pair(&gd["pieces_y"]),
                piece_uv: xy(&gd["piece_uv"]),
                arrow_dx: pair(&gd["arrow"]["dx"]),
                arrow: [u(&gd["arrow"]["w"]), u(&gd["arrow"]["h"])],
                arrow_uv: xy(&gd["arrow"]["uv"]),
            },
            gauge_slide: n(&g["vs"]["gauge"]["slide"]),
            title_text: (u(&g["title"]["font"]) as usize, u(&g["title"]["colour"]), n(&g["title"]["y"]), u(&g["title"]["idle_frames"])),
            ticker_text: (u(&g["ticker"]["font"]) as usize, n(&g["ticker"]["y"]), [u(&g["ticker"]["colours"][0]), u(&g["ticker"]["colours"][1])]),
            cells: arr(&g["cells"]).iter().map(|c| (n(&c["x"]), n(&c["y"]), u(&c["character"]) as usize)).collect(),
            colour_a: rgb(&g["mode_colours"][vs][0]).unwrap_or(Color32::RED),
            colour_b: rgb(&g["mode_colours"][vs][1]).unwrap_or(Color32::BLUE),
            background: arr(&g["background"]).iter().map(Quad::from).collect(),
            glows: arr(&g["glows"]).iter().map(Quad::from).collect(),
            letters: arr(&g["letters"]["cells"]).iter().map(|c| [u(&c["u"]), u(&c["v"]), u(&c["clut"][0]), u(&c["clut"][1])]).collect(),
            banner: g["banner"]["text"].as_str().unwrap_or_default().to_string(),
            lines: arr(&g["frame_lines"]).iter().map(|l| (u(&l["colour"]) as usize, std::array::from_fn(|k| n(&l["xy"][k])))).collect(),
            line_sets: arr(&g["frame_colours"]).iter().map(|s| std::array::from_fn(|k| rgb(&s[k]).unwrap_or(Color32::BLACK))).collect(),
            players,
            title: g["titles"][vs].as_str().unwrap_or_default().to_string(),
            ticker: g["titles"][u(&g["ticker"]["index"]) as usize].as_str().unwrap_or_default().to_string(),
            bar_cluts: arr(&screens["hud"]["names"]["plate_cluts"]).iter().map(xy).collect(),
            win_colours: arr(&g["vs"]["win_colours"]).iter().map(u).collect(),
            draw_colours: arr(&g["vs"]["draw_colours"]).iter().map(u).collect(),
            pieces: arr(&g["vs"]["gauge"]["pieces"]).iter().map(|p| (n(&p["w"]), xy(&p["lit"]), xy(&p["unlit"]))).collect(),
            levels: arr(&g["vs"]["gauge"]["levels"]).iter().filter_map(|s| s.as_str().map(String::from)).collect(),
            percents: arr(&g["vs"]["gauge"]["percents"]).iter().filter_map(|p| p.as_i64()).map(|p| p as i32).collect(),
            draw_text: g["vs"]["draw_text"][0].as_str().unwrap_or_default().to_string(),
            life: g["vs"]["gauge"]["life"].as_str().unwrap_or_default().to_string(),
            push: [0, 1].map(|k| screens["hud"]["under_bar"]["push"]["texts"][k].as_str().unwrap_or_default().to_string()),
            team_size: (g["team_size"]["format"].as_str().unwrap_or_default().to_string(), [0, 1].map(|k| g["team_size"]["players"][k].as_str().unwrap_or_default().to_string())),
            gauge_cluts: [xy(&g["vs"]["gauge"]["frame_clut"]), xy(&g["vs"]["gauge"]["arrow_clut"])],
            default_level: g["vs"]["gauge"]["default"].as_u64().unwrap_or(0) as usize,
            mode_colours: arr(&g["mode_colours"]).iter().map(|c| (rgb(&c[0]).unwrap_or(Color32::RED), rgb(&c[1]).unwrap_or(Color32::BLUE))).collect(),
            titles: arr(&g["titles"]).iter().filter_map(|t| t.as_str().map(String::from)).collect(),
            team_strip: [0, 1].map(|k| [n(&g["players"][k]["team_strip_x"]), n(&g["players"][k]["team_strip_y"])]),
            mode_rows: [row("team"), row("ball")],
        }
    }
}

/// The VS session's tally (wins per player, draws), kept between matches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VsRecord {
    pub wins: [u32; 2],
    pub draws: u32,
}

/// A team being picked: its size and the members so far (character id, costume).
type TeamPicks = (usize, Vec<(usize, usize)>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Team Battle: a side nobody plays yet (START joins).
    NotJoined,
    /// Team Battle: setting the team's size.
    Size,
    /// Ball: choosing the ball type (the first player done with the pick, 0x800B51D8).
    BallType,
    Choosing,
    /// Picked; setting the LIFE handicap.
    Handicap,
    Done,
}

#[derive(Clone, Copy, Debug)]
struct Player {
    state: State,
    col: usize,
    row: usize,
    /// (character id, costume index in the roster's list)
    pick: Option<(usize, usize)>,
    level: usize,
    gauge: f32,
}

pub struct GridSelect {
    data: GridData,
    pub difficulty: usize,
    pub record: VsRecord,
    players: [Player; 2],
    frame: u32,
    /// Frames with no key (the title gives way to the ticker after 480).
    idle: u32,
    /// Team Battle: each side's size and members picked so far.
    team: Option<[TeamPicks; 2]>,
    /// Ball: the ball type, who chose it, the carousel's offset (the slide, ±0x1000) and its target.
    ball: Option<BallPick>,
    /// The page of cells shown (the extra fighters' is 1): the last moved cursor's.
    page: usize,
}

/// Ball's select state (ctx+0x41 / +0x40 / +0x43, the chooser's ball +0x68 / +0x78 / +0xB0).
#[derive(Clone, Copy, Debug)]
pub struct BallPick {
    pub kind: usize,
    pub chooser: Option<usize>,
    x: i32,
    target: i32,
    dir: i32,
    spin: [i32; 3],
}

impl GridSelect {
    pub fn new(data: GridData, difficulty: usize, record: VsRecord) -> Self {
        let start = |k: usize| {
            let p = &data.players[k];
            let cell = data.cells.iter().position(|c| c.2 == p.default_char).unwrap_or(0);
            let (row, col) = data.place(cell);
            Player { state: State::Choosing, col, row, pick: None, level: data.default_level, gauge: p.gauge_hidden }
        };
        let players = [start(0), start(1)];
        Self { data, difficulty, record, players, frame: 0, idle: 0, team: None, ball: None, page: 0 }
    }

    /// Ball's select (screen B, its game mode): VS's grid, P2 joins with START; the first player done with
    /// the pick chooses the ball (`kind` is the last one chosen).
    pub fn ball(roster: &Roster, difficulty: usize, kind: usize) -> Self {
        let mut data = roster.grid.clone();
        if let Some(&(a, b)) = data.mode_colours.get(data.mode_rows[1]) {
            (data.colour_a, data.colour_b) = (a, b);
        }
        if let Some(t) = data.titles.get(data.mode_rows[1]) {
            data.title = t.clone();
        }
        let mut g = Self::new(data, difficulty, VsRecord::default());
        g.players[1].state = State::NotJoined;
        g.ball = Some(BallPick { kind: kind.min(2), chooser: None, x: 0, target: 0, dir: 0, spin: [0; 3] });
        g
    }

    /// A Ball key of side `k`.
    fn ball_key(&mut self, k: usize, key: Key, roster: &Roster) -> Outcome {
        let Some(mut b) = self.ball else { return Outcome::None };
        match self.players[k].state {
            State::NotJoined => {
                if key == Key::Start {
                    self.players[k].state = State::Choosing;
                    sfx::play(sfx::join());
                }
            }
            State::Choosing => match key {
                Key::Left | Key::Right | Key::Up | Key::Down => self.move_cursor(k, key),
                Key::Button(_) | Key::Start => {
                    let id = self.cell(k).2;
                    let Some(c) = roster.by_id(id) else { return Outcome::None };
                    let Some(want) = c.costume_for(key) else { return Outcome::None };
                    // no model twice on screen: the next costume the other player has not taken, or none
                    let other = self.players[1 - k].pick;
                    let Some(costume) = crate::menu::free_costume(c.costumes.len(), want, |c| other == Some((id, c))) else {
                        sfx::play(sfx::back());
                        return Outcome::None;
                    };
                    self.players[k].pick = Some((id, costume));
                    sfx::play(sfx::pick());
                    // the first to finish chooses the ball (0x80053A64)
                    if b.chooser.is_none() {
                        b.chooser = Some(k);
                        self.players[k].state = State::BallType;
                    } else {
                        self.players[k].state = State::Done;
                    }
                }
                Key::Back => {
                    sfx::play(sfx::start());
                    return Outcome::Quit;
                }
            },
            // the carousel: left the next type, right the previous; input once it has settled
            State::BallType => {
                if (b.x - b.target).abs() >= 0x40 {
                    self.ball = Some(b);
                    return Outcome::None;
                }
                match key {
                    Key::Left => {
                        (b.target, b.dir) = (-0x1000, -1);
                        sfx::play(sfx::select_cursor());
                    }
                    Key::Right => {
                        (b.target, b.dir) = (0x1000, 1);
                        sfx::play(sfx::select_cursor());
                    }
                    Key::Button(_) | Key::Start => {
                        sfx::play(sfx::pick());
                        self.players[k].state = State::Done;
                    }
                    Key::Back => {
                        sfx::play(sfx::pick());
                        b.chooser = None;
                        self.players[k].pick = None;
                        self.players[k].state = State::Choosing;
                    }
                    _ => {}
                }
            }
            State::Done if key == Key::Back => {
                if b.chooser == Some(k) {
                    b.chooser = None;
                }
                self.players[k].pick = None;
                self.players[k].state = State::Choosing;
                sfx::play(sfx::pick());
            }
            _ => {}
        }
        self.ball = Some(b);
        Outcome::None
    }

    /// Ball: the carousel's motion, and the fight once every joined side is done (a side nobody plays is
    /// the CPU, picked by the app: roster index `menu::CPU_SIDE`).
    fn ball_tick(&mut self, roster: &Roster) -> Outcome {
        let Some(mut b) = self.ball else { return Outcome::None };
        b.spin = [b.spin[0] + 1, b.spin[1] + 8, b.spin[2] + 2];
        if self.players.iter().any(|p| p.state == State::BallType) {
            b.spin[1] += 8;
        }
        b.x += (b.target - b.x) >> 3;
        if b.x < -0x7FF && b.dir == -1 {
            (b.kind, b.x, b.target, b.dir) = ((b.kind + 1) % BALL_KINDS, 0x800, 0, 0);
        }
        if b.x >= 0x800 && b.dir == 1 {
            (b.kind, b.x, b.target, b.dir) = ((b.kind + BALL_KINDS - 1) % BALL_KINDS, -0x800, 0, 0);
        }
        self.ball = Some(b);
        let joined = [0, 1].map(|k| self.players[k].state != State::NotJoined);
        if (0..2).any(|k| joined[k] && self.players[k].state != State::Done) {
            return Outcome::None;
        }
        let pick = |k: usize| match self.players[k].pick {
            Some((id, costume)) if joined[k] => (roster.characters.iter().position(|c| c.id == id).unwrap_or(0), costume),
            _ => (crate::menu::CPU_SIDE, 0),
        };
        let (p1, p2) = (pick(0), pick(1));
        let level = (self.difficulty * 4 + 1).min(9);
        Outcome::Fight(
            FightSetup {
                mode: Mode::Ball,
                players: [p1, p2],
                stage: roster.ball_stage,
                difficulty: self.difficulty,
                level,
                handicap: [100; 2],
                ball: Some((b.kind, b.chooser.unwrap_or(0))),
                partners: None,
            },
            Vec::new(),
        )
    }

    /// The ball on the select screen and its texts (0x800B5478): a 2D projection of the ball mesh (where the select
    /// camera puts it: ball.json `constants.select.arrows.ball`), the type's level and name, BALL DAMAGE and its
    /// percent.
    pub fn draw_ball(&self, painter: &egui::Painter, v: &View, art: &Art, roster: &Roster) {
        let ball = roster.ball.as_deref();
        let (Some(b), Some(data)) = (self.ball, ball) else { return };
        if b.chooser.is_none() {
            return;
        }
        let Some(kind) = data.types.get(b.kind) else { return };
        let r = |a: i32| a as f32 * std::f32::consts::TAU / 4096.0;
        let rot = glam::Mat3::from_rotation_z(r(b.spin[2])) * glam::Mat3::from_rotation_y(r(b.spin[1])) * glam::Mat3::from_rotation_x(r(b.spin[0]));
        let sel = &roster.ball_select;
        let at = &sel["ball"];
        // (the mesh's radius in game units: its first vertex's distance)
        let r0 = data.mesh.vertices.first().map_or(1.0, |p| glam::Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32).length().max(1.0));
        let radius = n(&at["radius"]);
        let (cx, cy, scale) = (n(&at["x"]) + b.x as f32 * radius / r0 / 8.0, n(&at["y"]), radius / r0);
        let pts: Vec<glam::Vec3> = data.mesh.vertices.iter().map(|p| rot * glam::Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32)).collect();
        let normals: Vec<glam::Vec3> = data.mesh.normals.iter().map(|n| rot * glam::Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32) / 4096.0).collect();
        let mut faces: Vec<([usize; 3], usize, bool)> = data.mesh.triangles.iter().enumerate().map(|(k, t)| (*t, k, true)).collect();
        for (k, q) in data.mesh.quads.iter().enumerate() {
            faces.push(([q[0], q[1], q[2]], k, false));
            faces.push(([q[1], q[3], q[2]], k, false));
        }
        faces.sort_by(|a, b| {
            let z = |f: &[usize; 3]| pts[f[0]].z + pts[f[1]].z + pts[f[2]].z;
            z(&b.0).total_cmp(&z(&a.0))
        });
        let mut mesh = egui::Mesh::default();
        let light = glam::Vec3::new(-0.3, -0.6, -0.75).normalize();
        for (f, k, tri) in faces {
            let c = kind.colours.get(if tri { k & 7 } else { 8 + (k & 7) }).copied().unwrap_or([255; 3]);
            let n = (normals[f[0]] + normals[f[1]] + normals[f[2]]).normalize_or_zero();
            let lit = (0.25 + 0.85 * n.dot(light).max(0.0)).min(1.2);
            let col = Color32::from_rgb((c[0] as f32 * lit).min(255.0) as u8, (c[1] as f32 * lit).min(255.0) as u8, (c[2] as f32 * lit).min(255.0) as u8);
            let base = mesh.vertices.len() as u32;
            for i in f {
                mesh.colored_vertex(v.pos(cx + pts[i].x * scale, cy + pts[i].y * scale), col);
            }
            mesh.add_triangle(base, base + 1, base + 2);
        }
        painter.add(egui::Shape::mesh(mesh));
        let ar = &sel["arrows"];
        if self.players.iter().any(|p| p.state == State::BallType) && self.frame & ar["blink"].as_u64().unwrap_or(0) as u32 != 0 {
            let c = |k: usize| ar["colour"][k].as_u64().unwrap_or(0) as u8;
            for t in ar["triangles"].as_array().into_iter().flatten() {
                let p: Vec<egui::Pos2> = t.as_array().into_iter().flatten().map(|q| v.pos(n(&q[0]), n(&q[1]))).collect();
                painter.add(egui::Shape::convex_polygon(p, Color32::from_rgb(c(0), c(1), c(2)), egui::Stroke::NONE));
            }
        }
        if (b.x - b.target).abs() < 0x40 {
            let font = |v: &serde_json::Value| v["font"].as_u64().unwrap_or(0) as usize;
            let colour = |v: &serde_json::Value| v["colour"].as_u64().unwrap_or(0) as u32;
            let centre = crate::art::CENTRE;
            for (t, text) in [(&sel["level"], &kind.level), (&sel["name"], &kind.name)] {
                art.text(painter, v, font(t), colour(t), centre - art.text_w(font(t), text) / 2.0, n(&t["y"]), text, Color32::WHITE);
            }
            let (l, dm, pc) = (&sel["damage_label"], &sel["damage"], &sel["percent"]);
            art.text(painter, v, font(l), colour(l), n(&l["x"]), n(&l["y"]), l["text"].as_str().unwrap_or_default(), Color32::WHITE);
            art.text(painter, v, font(dm), colour(dm), n(&dm["x"]), n(&dm["y"]), &format!("{:3}", kind.damage_percent), Color32::WHITE);
            art.text(painter, v, font(pc), colour(dm), n(&pc["x"]), n(&pc["y"]), "%", Color32::WHITE);
        }
    }

    /// Team Battle's select (screen B, its game mode; init 0x80053448): P1 sets the team size (its default at first),
    /// then picks members in order; P2 can join with START. The mode's colours and title.
    pub fn team(roster: &Roster, difficulty: usize) -> Self {
        let mut data = roster.grid.clone();
        if let Some(&(a, b)) = data.mode_colours.get(data.mode_rows[0]) {
            (data.colour_a, data.colour_b) = (a, b);
        }
        if let Some(t) = data.titles.get(data.mode_rows[0]) {
            data.title = t.clone();
        }
        let size = roster.modes.team_size[2];
        let mut g = Self::new(data, difficulty, VsRecord::default());
        g.players[0].state = State::Size;
        g.players[1].state = State::NotJoined;
        g.team = Some([(size, Vec::new()), (size, Vec::new())]);
        g
    }

    /// A Team Battle key of side `k` (states 1 / 2 / 9 of 0x80053448's table).
    fn team_key(&mut self, k: usize, key: Key, roster: &Roster) -> Outcome {
        let [min, max, _] = roster.modes.team_size;
        if self.team.is_none() {
            return Outcome::None;
        }
        fn team(g: &mut GridSelect, k: usize) -> &mut TeamPicks {
            // (checked above: the team select's picks)
            &mut g.team.get_or_insert_with(Default::default)[k]
        }
        match self.players[k].state {
            State::NotJoined => {
                if key == Key::Start {
                    self.players[k].state = State::Size;
                    sfx::play(sfx::join());
                }
            }
            // the size, 1..8 (0x800537BC)
            State::Size => match key {
                Key::Left | Key::Right => {
                    let size = team(self, k).0;
                    let next = if key == Key::Right { (size + 1).min(max) } else { size.saturating_sub(1).max(min) };
                    if next != size {
                        team(self, k).0 = next;
                        sfx::play(sfx::select_cursor());
                    }
                }
                Key::Button(_) | Key::Start => {
                    self.players[k].state = State::Choosing;
                    sfx::play(sfx::pick());
                }
                Key::Back => {
                    sfx::play(sfx::start());
                    return Outcome::Quit;
                }
                _ => {}
            },
            // picking in order; START: the rest is chosen for you; SELECT undoes
            State::Choosing => match key {
                Key::Left | Key::Right | Key::Up | Key::Down => self.move_cursor(k, key),
                Key::Start => {
                    self.players[k].state = State::Done;
                    sfx::play(sfx::pick());
                }
                Key::Button(b) => {
                    let id = self.cell(k).2;
                    let Some(c) = roster.by_id(id) else { return Outcome::None };
                    if team(self, k).1.iter().any(|m| m.0 == id) {
                        return Outcome::None; // already in the team
                    }
                    // the other team's members meet this one: a costume they have not taken, or none
                    let theirs = self.team.as_ref().map(|t| t[1 - k].1.clone()).unwrap_or_default();
                    let Some(costume) = crate::menu::free_costume(c.costumes.len(), c.costume_for(Key::Button(b)).unwrap_or(0), |c| theirs.contains(&(id, c))) else {
                        sfx::play(sfx::back());
                        return Outcome::None;
                    };
                    let t = team(self, k);
                    t.1.push((id, costume));
                    let full = t.1.len() >= t.0;
                    sfx::play(sfx::pick());
                    if full {
                        self.players[k].state = State::Done;
                    }
                }
                Key::Back => {
                    sfx::play(sfx::pick());
                    if team(self, k).1.pop().is_none() {
                        self.players[k].state = State::Size;
                    }
                }
            },
            State::Done => {
                if key == Key::Back {
                    team(self, k).1.pop();
                    self.players[k].state = State::Choosing;
                    sfx::play(sfx::pick());
                }
            }
            State::Handicap | State::BallType => {}
        }
        Outcome::None
    }

    fn move_cursor(&mut self, k: usize, key: Key) {
        let p = self.players[k];
        let rows = self.data.rows();
        let n = rows.get(p.row).map_or(7, Vec::len).max(1);
        match key {
            Key::Left | Key::Right => {
                self.players[k].col = if key == Key::Right { (p.col + 1) % n } else { (p.col + n - 1) % n };
                sfx::play(sfx::select_cursor());
            }
            _ => {
                let row = if key == Key::Down { (p.row + 1).min(rows.len().saturating_sub(1)) } else { p.row.saturating_sub(1) };
                if row != p.row {
                    // the nearest cell by x in the new row (the original's rows line up column for column)
                    let x = rows[p.row].get(p.col).map_or(0.0, |&i| self.data.cells[i].0);
                    let col = (0..rows[row].len()).min_by(|&a, &b| (self.data.cells[rows[row][a]].0 - x).abs().total_cmp(&(self.data.cells[rows[row][b]].0 - x).abs())).unwrap_or(0);
                    self.players[k].row = row;
                    self.players[k].col = col;
                    sfx::play(sfx::select_cursor());
                    self.page = on_page(self.cell(k).1).0;
                }
            }
        }
    }

    /// Team Battle: every side that plays is done: the teams, filled up to their size (0x800B225C: characters not
    /// in the team at random; a side nobody plays takes the other's size).
    fn team_done(&self, roster: &Roster, seed: &mut u32) -> Option<Outcome> {
        let team = self.team.as_ref()?;
        let joined = [0, 1].map(|k| self.players[k].state != State::NotJoined);
        if (0..2).any(|k| joined[k] && self.players[k].state != State::Done) {
            return None;
        }
        let mut teams: [crate::modes::Team; 2] = Default::default();
        let mut picks: [Vec<(usize, usize)>; 2] = [0, 1].map(|k| if joined[k] { team[k].1.clone() } else { Vec::new() });
        for k in 0..2 {
            let size = if joined[k] { team[k].0 } else { team[1 - k].0 };
            let picked = picks[k].len();
            // the rest at random, each in a costume the other team has not taken (no model twice on screen)
            let costumes = |id: usize| roster.by_id(id).map_or(1, |c| c.costumes.len());
            let free = |id: usize, theirs: &[(usize, usize)]| crate::menu::free_costume(costumes(id), 0, |c| theirs.contains(&(id, c)));
            let mut pool: Vec<usize> = roster.characters.iter().map(|c| c.id).filter(|id| !picks[k].iter().any(|m| m.0 == *id)).collect();
            while picks[k].len() < size && !pool.is_empty() {
                *seed = seed.wrapping_mul(0x41C6_4E6D).wrapping_add(12345);
                let id = pool.remove(((*seed >> 16) & 0x7FFF) as usize % pool.len());
                if let Some(c) = free(id, &picks[1 - k]) {
                    picks[k].push((id, c));
                }
            }
            teams[k].members = picks[k].iter().filter_map(|&(id, cos)| roster.index_of(id).map(|i| (i, cos))).collect();
            teams[k].picked = picked;
            teams[k].human = joined[k];
        }
        Some(Outcome::Run(Box::new(crate::modes::Run::team(teams, self.difficulty))))
    }

    fn cell(&self, k: usize) -> (f32, f32, usize) {
        let p = &self.players[k];
        let rows = self.data.rows();
        rows.get(p.row).and_then(|r| r.get(p.col)).and_then(|&i| self.data.cells.get(i)).copied().unwrap_or((0.0, 0.0, 0))
    }

    /// Online, the select's time is up: every side still choosing takes the character under its cursor (or the
    /// next one it can have), the LIFE and the ball as they stand; a side nobody joined stays the CPU's.
    pub fn finish(&mut self, roster: &Roster) {
        for k in 0..2 {
            for _ in 0..64 {
                match self.players[k].state {
                    State::Choosing => {
                        let before = self.players[k].pick;
                        let _ = self.key(k, Key::Button(0), roster);
                        if self.players[k].state == State::Choosing && self.players[k].pick == before {
                            self.move_cursor(k, Key::Right);
                        }
                    }
                    State::Handicap => {
                        let _ = self.key(k, Key::Button(0), roster);
                    }
                    State::BallType => {
                        if let Some(b) = self.ball.as_mut() {
                            (b.x, b.target, b.dir) = (0, 0, 0);
                        }
                        let _ = self.key(k, Key::Start, roster);
                    }
                    _ => break,
                }
            }
        }
    }

    /// A key of `player`; returns the outcome (back to the menu: Quit).
    pub fn key(&mut self, player: usize, key: Key, roster: &Roster) -> Outcome {
        self.idle = 0;
        let k = player.min(1);
        if self.team.is_some() {
            return self.team_key(k, key, roster);
        }
        if self.ball.is_some() {
            return self.ball_key(k, key, roster);
        }
        let p = self.players[k];
        match p.state {
            State::Choosing => match key {
                Key::Left | Key::Right | Key::Up | Key::Down => self.move_cursor(k, key),
                Key::Button(_) | Key::Start => {
                    let id = self.cell(k).2;
                    let Some(c) = roster.by_id(id) else { return Outcome::None };
                    // square the first costume, cross / circle the second, triangle / START the extra (0x8010DAB8)
                    let Some(want) = c.costume_for(key) else { return Outcome::None };
                    // the same character and costume as the other player: the next costume (0x8004F04C); none left,
                    // the character cannot be played (no model twice on screen)
                    let other = self.players[1 - k].pick;
                    let Some(costume) = crate::menu::free_costume(c.costumes.len(), want, |c| other == Some((id, c))) else {
                        sfx::play(sfx::back());
                        return Outcome::None;
                    };
                    self.players[k].pick = Some((id, costume));
                    self.players[k].state = State::Handicap;
                    sfx::play(sfx::pick());
                }
                Key::Back => {
                    sfx::play(sfx::start());
                    return Outcome::Quit;
                }
            },
            State::Handicap => match key {
                Key::Left | Key::Right => {
                    // P1: left raises it, P2: right
                    let up = (key == Key::Right) == (self.data.players[k].side == 1);
                    let level = if up { (p.level + 1).min(self.data.percents.len().saturating_sub(1)) } else { p.level.saturating_sub(1) };
                    if level != p.level {
                        self.players[k].level = level;
                        sfx::play(sfx::select_cursor());
                    }
                }
                Key::Back => {
                    self.players[k].pick = None;
                    self.players[k].state = State::Choosing;
                    sfx::play(sfx::pick());
                }
                Key::Button(_) | Key::Start => {
                    self.players[k].state = State::Done;
                    sfx::play(sfx::pick());
                }
                _ => {}
            },
            State::Done | State::NotJoined | State::Size | State::BallType => {}
        }
        Outcome::None
    }

    /// Per frame: animation, gauges; both players done -> the fight.
    pub fn tick(&mut self, roster: &Roster, seed: &mut u32) -> Outcome {
        self.frame += 1;
        self.idle += 1;
        if self.team.is_some() {
            return self.team_done(roster, seed).unwrap_or(Outcome::None);
        }
        if self.ball.is_some() {
            return self.ball_tick(roster);
        }
        for k in 0..2 {
            let d = &self.data.players[k];
            let target = if self.players[k].state == State::Choosing { d.gauge_hidden } else { d.gauge_x };
            let (g, slide) = (&mut self.players[k].gauge, self.data.gauge_slide);
            *g = if (target - *g).abs() < slide { target } else { *g + slide * (target - *g).signum() };
        }
        if self.players.iter().all(|p| p.state == State::Done) {
            let pick = |k: usize| {
                let (id, costume) = self.players[k].pick.unwrap_or((0, 0));
                (roster.characters.iter().position(|c| c.id == id).unwrap_or(0), costume)
            };
            let (p1, p2) = (pick(0), pick(1));
            let stage = roster.home_stage(roster.characters[p2.0].id, p2.1);
            let handicap = [0, 1].map(|k| self.data.handicap(self.players[k].level));
            return Outcome::Fight(FightSetup { mode: Mode::Versus, players: [p1, p2], stage, difficulty: self.difficulty, level: 5, handicap, ball: None, partners: None }, Vec::new());
        }
        Outcome::None
    }

    /// The character a player shows (under the cursor or picked) and its costume.
    fn shown(&self, k: usize) -> (usize, usize) {
        self.players[k].pick.unwrap_or((self.cell(k).2, 0))
    }

    pub fn draw(&self, painter: &egui::Painter, v: &View, art: &Art, roster: &Roster) {
        let d = &self.data;
        let src: &[&str] = &["exe"];
        let solid = |c: Color32| [c; 4];
        let (sw, sh) = (crate::art::SCREEN_W, crate::art::SCREEN_H);
        // 1. black, the four backdrop quads in the mode colour
        Art::shade(painter, v, [0.0, 0.0, sw, sh], solid(Color32::BLACK));
        for q in &d.background {
            Art::shade(painter, v, q.rect(), q.colours.map(|c| c.unwrap_or(d.colour_a)));
        }
        // 2. the VS panel: a darkened box with soft sides and a tab below, narrowing to its bottom edge (subtractive in
        // the original)
        let dark = Color32::from_black_alpha(d.darken);
        let ([x0, y0, x1, y1], soft, [t0, ty0, t1, ty1], [b0, b1]) = (d.panel, d.soft, d.tab, d.tab_bottom);
        Art::shade(painter, v, [x0 + soft, y0, x1 - x0 - 2.0 * soft, y1 - y0], solid(dark));
        Art::shade(painter, v, [x0, y0, soft, y1 - y0], [Color32::TRANSPARENT, dark, Color32::TRANSPARENT, dark]);
        Art::shade(painter, v, [x1 - soft, y0, soft, y1 - y0], [dark, Color32::TRANSPARENT, dark, Color32::TRANSPARENT]);
        let mut tab = egui::Mesh::default();
        for (x, y) in [(t0, ty0), (t1, ty0), (b0, ty1), (b1, ty1)] {
            tab.colored_vertex(v.pos(x, y), dark);
        }
        tab.indices.extend([0, 1, 2, 1, 3, 2]);
        painter.add(egui::Shape::mesh(tab));
        // 3. the banner, scrolling left
        let (font, colour, y, period, speed) = d.banner_text;
        let period = period.max(1);
        let mut x = -(((self.frame as i32 * -speed) % period) as f32) + speed as f32;
        while x < sw {
            art.text(painter, v, font, colour, x, y, &d.banner, Color32::WHITE);
            x += period as f32;
        }
        // 4. the big title letters (font 2 glyphs scaled up), scrolling left
        let [w, h, scale, y, step, period, speed] = d.letters_draw;
        let period = period.max(1);
        let base = -(((self.frame as i32 * -speed) % period) as f32) + speed as f32;
        for rep in 0..2 {
            for (k, l) in d.letters.iter().enumerate() {
                let lx = base + (step * k as i32) as f32 + (period * rep) as f32;
                if (-((w * scale) as f32) + 1.0..sw).contains(&lx) {
                    let t = Tex { page: d.letters_page, bpp: 4, u: l[0], v: l[1], clut: Some([l[2], l[3]]) };
                    art.quad(painter, v, src, &t, w as u32, h as u32, [lx, y as f32, (w * scale) as f32, (h * scale) as f32], [Color32::WHITE; 4]);
                }
            }
        }
        // 5. the side glows (additive, colour B)
        for q in &d.glows {
            Art::shade(painter, v, q.rect(), q.colours.map(|c| additive(c.unwrap_or(d.colour_b))));
        }
        // 6. the grid: framed faces ("?" for characters not in the game data)
        for &(cx, cy, id) in &d.cells {
            let (page, cy) = on_page(cy);
            if page != self.page {
                continue;
            }
            self.frame_lines(painter, v, cx, cy, 0, 0x80, None);
            self.face(painter, v, art, roster, id, cx + d.face_d[0], cy + d.face_d[1]);
        }
        // 7. each player's face under the panel (Team: the strip of members), the name with its costume bar
        for k in 0..2 {
            let p = &d.players[k];
            if let Some(team) = &self.team {
                if self.team_side(painter, v, art, roster, k, team) {
                    continue;
                }
            }
            if self.ball.is_some() && self.players[k].state == State::NotJoined {
                if self.frame & 0x30 != 0 {
                    let t = &d.push[k];
                    art.text(painter, v, 0, 5, p.name_x - art.text_w(0, t) / 2.0, d.names.2 + d.push_dy, t, Color32::WHITE);
                }
                continue;
            }
            let (id, costume) = self.shown(k);
            if self.team.is_none() {
                self.frame_lines(painter, v, p.strip[0], p.strip[1], 0, 0x80, None);
                self.face(painter, v, art, roster, id, p.strip[0] + d.face_d[0], p.strip[1] + d.face_d[1]);
            }
            let cos = roster.by_id(id).and_then(|c| c.costumes.get(costume)).and_then(|m| roster.costume_of.get(m)).copied().unwrap_or(0);
            let name = roster.ui.selection_names.get(id * 4 + cos).cloned().unwrap_or_default();
            let (font, colour, y) = d.names;
            let w = art.text_w(font, &name);
            let x = p.name_x - w / 2.0;
            if let Some(&clut) = d.bar_cluts.get(cos) {
                let [bdx, bdw, bdy, bh] = d.name_bar;
                let bar = Tex { page: d.gauge.page, bpp: 4, u: d.name_bar_uv[0], v: d.name_bar_uv[1], clut: Some(clut) };
                art.quad(painter, v, src, &bar, 2, bh as u32, [x + bdx, y + bdy, w + bdw, bh], [Color32::WHITE; 4]);
            }
            art.text(painter, v, font, colour, x, y, &name, Color32::WHITE);
        }
        // 8. cursors: a blinking frame in the player's colours while choosing, steady once picked; a shared cell
        // shows P1's half on the left and P2's on the right
        let blink = crate::menu::triangle(self.frame << 4);
        let same = self.cell(0).0 == self.cell(1).0 && self.cell(0).1 == self.cell(1).1;
        for k in [1, 0] {
            if matches!(self.players[k].state, State::NotJoined | State::Size) || ((self.team.is_some() || self.ball.is_some()) && matches!(self.players[k].state, State::Done | State::BallType)) {
                continue;
            }
            let (cx, cy, _) = self.cell(k);
            let (page, cy) = on_page(cy);
            if page != self.page {
                continue;
            }
            let b = if self.players[k].state == State::Choosing { blink } else { 0x80 };
            let half = same.then_some(k);
            self.frame_lines(painter, v, cx, cy, d.players[k].frame_set, b, half);
            let p = &d.players[k];
            let tint = modulate(b, b, b);
            let (dy, [w, h], page) = d.tag;
            art.sprite(painter, v, src, &Tex { page, bpp: 4, u: p.tag[0], v: p.tag[1], clut: None }, w, h, cx + p.tag_dx, cy + dy, tint);
        }
        // the ball being chosen (Ball)
        self.draw_ball(painter, v, art, roster);
        // 9. the title, giving way to the ticker after 480 idle frames
        self.title(painter, v, art);
        // 10. the VS tally and the LIFE gauges (other modes: "VS")
        if self.team.is_some() || self.ball.is_some() {
            if self.ball.is_none_or(|b| b.chooser.is_none()) {
                d.vs_text.draw(painter, v, art);
            }
            if self.frame < 16 {
                Art::shade(painter, v, [0.0, 0.0, sw, sh], solid(Color32::from_black_alpha(255 - (self.frame * 16).min(255) as u8)));
            }
            return;
        }
        let r = &self.record;
        let wc = |n: u32, t: &[u32]| t.get(((n / 100) & 3) as usize).copied().unwrap_or(2);
        let (wx, wy, wf) = d.wins;
        for (&n, &x) in r.wins.iter().zip(&wx) {
            art.text(painter, v, wf, wc(n, &d.win_colours), x, wy, &format!("{:02}", n % 100), Color32::WHITE);
        }
        let (([dx, dy], df, dc), ([tx, ty], tf, tc), ([nx, ny], nf)) = (d.dash, d.draw_place, d.draws);
        art.text(painter, v, df, dc, dx, dy, "-", Color32::WHITE);
        art.text(painter, v, tf, tc, tx, ty, &d.draw_text, Color32::WHITE);
        art.text(painter, v, nf, wc(r.draws, &d.draw_colours), nx, ny, &format!("{:02}", r.draws % 100), Color32::WHITE);
        for k in 0..2 {
            self.gauge(painter, v, art, k);
        }
        // 11. the fade-in from black (16 frames)
        if self.frame < 16 {
            Art::shade(painter, v, [0.0, 0.0, sw, sh], solid(Color32::from_black_alpha(255 - (self.frame * 16).min(255) as u8)));
        }
    }

    /// Team Battle's side `k`: the strip of members (0x80054194: 4 per row from the team origin, picks, the next
    /// slot blinking, "?" for the rest) and the info text (the size, the join text, or the name under the cursor).
    /// False when the normal name should be drawn.
    #[allow(clippy::too_many_arguments)]
    fn team_side(&self, painter: &egui::Painter, v: &View, art: &Art, roster: &Roster, k: usize, team: &[TeamPicks; 2]) -> bool {
        let (size, picks) = (&team[k].0, &team[k].1);
        let [ox, oy] = self.data.team_strip[k];
        let state = self.players[k].state;
        let cx = self.data.players[k].name_x;
        if state != State::NotJoined {
            for i in 0..*size {
                let (x, y) = (ox + self.data.cell[0] * (i & 3) as f32, oy + self.data.cell[1] * (i >> 2) as f32);
                let next = i == picks.len() && state == State::Choosing;
                if next && self.frame & 0x10 == 0 {
                    continue;
                }
                self.frame_lines(painter, v, x, y, 0, 0x80, None);
                let id = match picks.get(i) {
                    Some(&(id, _)) => id,
                    None if next => self.cell(k).2,
                    None => roster.ui.question,
                };
                crate::modes::face(painter, v, art, roster, id, x + self.data.face_d[0], y + self.data.face_d[1], 0);
            }
        }
        match state {
            State::NotJoined => {
                if self.frame & 0x30 != 0 {
                    let t = &self.data.push[k];
                    art.text(painter, v, 0, 5, cx - art.text_w(0, t) / 2.0, self.data.names.2 + self.data.push_dy, t, Color32::WHITE);
                }
                true
            }
            State::Size => {
                // "%1x %s\n ON TEAM" with PLAYER / PLAYERS (each line centred: OpenKumite's)
                let (format, players) = &self.data.team_size;
                let text = crate::art::printf(format, &[*size as u32]).replace("%s", players[usize::from(*size != 1)].trim_end());
                for (line, y) in text.split('\n').zip([262.0, 286.0]) {
                    art.text(painter, v, 1, 6, cx - art.text_w(1, line) / 2.0, y, line, Color32::WHITE);
                }
                true
            }
            State::Done => true,
            _ => false,
        }
    }

    /// The cell frame (0x800215EC): 10 one-pixel lines in colour set `set`, brightness `b` (0x80 = as is); `half`
    /// keeps only that player's half (0 left, 1 right).
    #[allow(clippy::too_many_arguments)]
    fn frame_lines(&self, painter: &egui::Painter, v: &View, x: f32, y: f32, set: usize, b: u32, half: Option<usize>) {
        let Some(colours) = self.data.line_sets.get(set) else { return };
        let clip = match half {
            Some(h) => {
                let [w, ch] = self.data.cell;
                painter.with_clip_rect(v.rect(x + w / 2.0 * h as f32, y, w / 2.0, ch))
            }
            None => painter.clone(),
        };
        for &(ci, [x0, y0, x1, y1]) in &self.data.lines {
            let c = colours[ci.min(2)];
            let s = |ch: u8| ((ch as u32 * 2 * b) >> 8).min(255) as u8;
            let c = Color32::from_rgb(s(c.r()), s(c.g()), s(c.b()));
            Art::shade(&clip, v, [x + x0, y + y0, x1 - x0 + 1.0, y1 - y0 + 1.0], [c; 4]);
        }
    }

    /// A face (0x8004B928): the character's, some in slices, or a blue "?".
    #[allow(clippy::too_many_arguments)]
    fn face(&self, painter: &egui::Painter, v: &View, art: &Art, roster: &Roster, id: usize, x: f32, y: f32) {
        let src: &[&str] = &["exe"];
        let [fw, fh] = roster.ui.face_size;
        if art.extra_face(painter, v, roster, id, x, y, fh as f32) {
            return;
        }
        if roster.by_id(id).is_none() {
            // "?" in slices, in the blue CLUT
            let clut = Some(roster.ui.question_cluts[1]);
            for &(t, h, dy) in &roster.ui.question_face {
                art.sprite(painter, v, src, &Tex { clut, ..t }, fw, h, x, y + dy, Color32::WHITE);
            }
            return;
        }
        let Some(t) = roster.ui.faces.get(id) else { return };
        if let Some(slices) = roster.ui.face_slices.get(&id) {
            for &[du, dy, h] in slices {
                art.sprite(painter, v, src, &t.offset(du, 0), fw, h, x, y + dy as f32, Color32::WHITE);
            }
        } else {
            art.sprite(painter, v, src, t, fw, fh, x, y, Color32::WHITE);
        }
    }

    /// The title or, after its idle frames, the ticker (PUSH START+SELECT TO EXIT) in turns, sliding a screen's
    /// width over 32 frames.
    fn title(&self, painter: &egui::Painter, v: &View, art: &Art) {
        let ((font, colour, y, idle), (tfont, ty, colours)) = (self.data.title_text, self.data.ticker_text);
        let sw = crate::art::SCREEN_W;
        let t = self.idle.saturating_sub(idle);
        let half = t & 0x100 != 0;
        let mut c = if half { sw * 1.5 } else { sw / 2.0 };
        if t & 0xE0 == 0xE0 {
            let k = (t & 31) as f32;
            c += if half { -k * sw / 32.0 } else { k * sw / 32.0 };
        }
        let title = &self.data.title;
        art.text(painter, v, font, colour, c - (art.text_w(font, title) as u32 / 2) as f32, y, title, Color32::WHITE);
        // the ticker, a screen to the left of the title, words in turns in its two colours: centred on its strlen
        // (its colour codes counted) in the font's advance, halved to a whole pixel (0x80054CA4)
        let len = self.data.ticker.len();
        let mut x = c - (art.text_w(tfont, &" ".repeat(len)) as u32 / 2) as f32 - sw;
        for (k, part) in self.data.ticker.split("%c").enumerate() {
            art.text(painter, v, tfont, colours[k % 2], x, ty, part, Color32::WHITE);
            x += art.text_w(tfont, part);
        }
    }

    /// The LIFE gauge (record 5 0x800B2A18): frame, 8 pieces lit up to the level, the arrow, LIFE and the percent.
    fn gauge(&self, painter: &egui::Painter, v: &View, art: &Art, k: usize) {
        let d = &self.data;
        let p = &d.players[k];
        let pl = &self.players[k];
        let (x, y) = (pl.gauge, p.gauge_y);
        let src: &[&str] = &["exe"];
        let b = if pl.state == State::Handicap { crate::menu::triangle(self.frame << 4) } else { 0x80 };
        let tint = modulate(b, b, b);
        let g = &d.gauge;
        let base = if p.side == 1 { x } else { x + g.left };
        let tex = |[u, v]: [u32; 2], clut: [u32; 2]| Tex { page: g.page, bpp: 4, u, v, clut: Some(clut) };
        let frame_clut = d.gauge_cluts[0];
        let [cw, ch] = g.cap;
        let ch_f = ch as f32;
        art.quad(painter, v, src, &tex(g.middle_uv, frame_clut), g.middle_texels, ch, [base + g.middle[0], y, g.middle[1], ch_f], [tint; 4]);
        art.sprite(painter, v, src, &tex(g.cap_uv[0], frame_clut), cw, ch, base, y, tint);
        art.sprite(painter, v, src, &tex(g.cap_uv[1], frame_clut), cw, ch, base + g.cap_dx, y, tint);
        let [bx, by, bw, bh] = g.dark;
        Art::shade(painter, v, [base + bx, y + by, bw, bh], [Color32::BLACK; 4]);
        // the pieces: P2 stacks rightwards, P1 leftwards
        let mut at = x + g.pieces_x[usize::from(p.side == 1)];
        let [py0, py1] = g.pieces_y;
        for (i, &(w, lit, unlit)) in d.pieces.iter().enumerate() {
            let clut = if pl.level >= i { lit } else { unlit };
            let x0 = if p.side == 1 { at } else { at - w };
            art.quad(painter, v, src, &tex(g.piece_uv, clut), 2, (py1 - py0) as u32, [x0, y + py0, w, py1 - py0], [Color32::WHITE; 4]);
            at = if p.side == 1 { at + w } else { at - w };
        }
        let ax = x + g.arrow_dx[usize::from(p.side == 1)];
        art.sprite(painter, v, src, &tex(g.arrow_uv, d.gauge_cluts[1]), g.arrow[0], g.arrow[1], ax, y, Color32::WHITE);
        art.text(painter, v, 0, 5, x + p.life[0], p.life[1], &d.life, Color32::WHITE);
        if let Some(level) = d.levels.get(pl.level) {
            art.text(painter, v, 0, 6, x + p.percent[0], p.percent[1], level, Color32::WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> Option<Roster> {
        let gd = crate::test_assets().join("gamedata");
        kumite_fs::exists(&gd.join("screens.json")).then(|| Roster::load(&gd))
    }

    #[test]
    fn vs_mode_picks_on_the_grid_with_handicaps() {
        let Some(r) = roster() else { return };
        let mut g = GridSelect::new(r.grid.clone(), 1, VsRecord::default());
        let p2 = g.cell(1).2;
        assert_ne!(g.cell(0).2, p2, "the two start on different characters");
        // both on P2's character with the same costume: P1 gets the other one
        g.key(1, Key::Button(0), &r);
        (g.players[0].row, g.players[0].col) = (g.players[1].row, g.players[1].col);
        assert_eq!(g.cell(0).2, p2);
        g.key(0, Key::Button(0), &r);
        assert_eq!(g.players[0].pick, Some((p2, 1)), "the clash rule");
        // handicaps: P1 left raises, P2 right raises
        g.key(0, Key::Left, &r);
        g.key(1, Key::Left, &r);
        g.key(0, Key::Start, &r);
        assert!(matches!(g.tick(&r, &mut 1), Outcome::None), "P2 has not confirmed");
        g.key(1, Key::Button(2), &r);
        match g.tick(&r, &mut 1) {
            Outcome::Fight(setup, _) => {
                assert_eq!(setup.mode, Mode::Versus);
                assert_eq!(setup.handicap, [110, 90]);
                assert!(crate::room::MatchSetup::Fight(setup.clone()).valid(&r), "a host's own setup passes a guest's check");
                assert_eq!((r.characters[setup.players[0].0].id, setup.players[0].1), (p2, 1));
            }
            _ => panic!("both done: the fight"),
        }
        // back out of the grid
        let mut g = GridSelect::new(r.grid.clone(), 1, VsRecord::default());
        assert!(matches!(g.key(0, Key::Back, &r), Outcome::Quit));
    }

    /// Some extra fighters are two each (the second their second costume, as in their own game, its own name): both
    /// players can pick one, the second gets the second costume.
    #[test]
    fn both_players_on_a_two_costume_extra_get_both_costumes() {
        let Some(r) = roster() else { return };
        let two_each = |c: &&crate::menu::Character| !c.prefix.is_empty() && c.costumes.len() == 2 && r.ui.selection_names.get(c.id * 4 + 1).is_some_and(|n| *n != r.ui.selection_names[c.id * 4]);
        for c in r.characters.iter().filter(two_each) {
            let name = &c.name;
            let mut g = GridSelect::new(r.grid.clone(), 1, VsRecord::default());
            let cell = g.data.cells.iter().position(|x| x.2 == c.id).unwrap();
            let (row, col) = g.data.place(cell);
            for k in 0..2 {
                (g.players[k].row, g.players[k].col) = (row, col);
            }
            g.key(0, Key::Button(0), &r);
            g.key(1, Key::Button(0), &r);
            assert_eq!((g.players[0].pick, g.players[1].pick), (Some((c.id, 0)), Some((c.id, 1))), "{name}");
        }
        let extras = r.characters.iter().any(|c| !c.prefix.is_empty());
        assert!(!extras || r.characters.iter().any(|c| two_each(&c)), "some extra fighters are two each");
    }

    /// No model twice on screen: a character whose every costume the other player has taken cannot be picked.
    #[test]
    fn a_character_with_every_costume_taken_cannot_be_picked() {
        let Some(r) = roster() else { return };
        let Some(one) = r.characters.iter().find(|c| c.costumes.len() == 1 && r.grid.cell_ids().contains(&c.id)).map(|c| c.id) else { return };
        let mut g = GridSelect::new(r.grid.clone(), 1, VsRecord::default());
        let at = |g: &GridSelect| g.cell(0).2;
        // both cursors on it; P2 takes it, then P1 tries
        let cell = g.data.cells.iter().position(|c| c.2 == one).unwrap();
        let (row, col) = g.data.place(cell);
        for k in 0..2 {
            (g.players[k].row, g.players[k].col) = (row, col);
        }
        g.key(1, Key::Button(0), &r);
        assert_eq!(g.players[1].pick, Some((one, 0)));
        g.key(0, Key::Button(0), &r);
        assert_eq!((at(&g), g.players[0].pick, g.players[0].state), (one, None, State::Choosing), "refused");
    }
}
