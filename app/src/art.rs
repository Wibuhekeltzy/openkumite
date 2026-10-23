//! The game's own 2D art from the converted `assets/pictures` (PNG), as egui textures. The screens draw it the way the
//! original hardware does: a sprite names a VRAM place, a depth and a CLUT (the pictures' `vram` and `cluts.json`), on
//! the original 368x480 screen. Also the timer digits for the HUD.

use egui::{Color32, Pos2, Rect, TextureHandle, Vec2};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

/// The original screen: 368x480 (interlaced), shown 4:3.
pub const SCREEN_W: f32 = 368.0;
pub const SCREEN_H: f32 = 480.0;
/// The screen's middle (x) and the whole screen [x, y, w, h].
pub const CENTRE: f32 = SCREEN_W / 2.0;
pub const FULL: [f32; 4] = [0.0, 0.0, SCREEN_W, SCREEN_H];

/// A number of screens.json (0 when missing), as the screens read them.
pub fn num(v: &serde_json::Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}
pub fn int(v: &serde_json::Value) -> u32 {
    v.as_u64().unwrap_or(0) as u32
}
pub fn pair(v: &serde_json::Value) -> [u32; 2] {
    [int(&v[0]), int(&v[1])]
}

/// Maps the original screen's pixel coordinates into the window.
#[derive(Clone, Copy)]
pub struct Screen {
    pub rect: Rect,
}

impl Screen {
    /// The largest 4:3 area of the window, centred.
    pub fn fit(window: Rect) -> Self {
        let (w, h) = (window.width(), window.height());
        let (fw, fh) = if w / h > 4.0 / 3.0 { (h * 4.0 / 3.0, h) } else { (w, w * 3.0 / 4.0) };
        Self { rect: Rect::from_center_size(window.center(), Vec2::new(fw, fh)) }
    }

    pub fn pos(&self, x: f32, y: f32) -> Pos2 {
        Pos2::new(self.rect.left() + x / SCREEN_W * self.rect.width(), self.rect.top() + y / SCREEN_H * self.rect.height())
    }

    /// A rectangle given in screen pixels.
    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_max(self.pos(x, y), self.pos(x + w, y + h))
    }
}

/// A texture and its size in pixels.
type Pic = (TextureHandle, [usize; 2]);

/// A picture as it sits in the original hardware's VRAM (pictures/index.json `vram`), in pixels of its own depth.
struct Placed {
    file: String,
    index: Option<String>,
    source: String,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    bpp: u32,
    clut: Option<[u32; 2]>,
}

/// Where a sprite's texels are: a texture page (VRAM words), depth and texel offset, like an original-hardware SPRT /
/// POLY_FT4.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tex {
    pub page: [u32; 2],
    pub bpp: u32,
    pub u: u32,
    pub v: u32,
    /// CLUT VRAM place; None = the picture's own colours.
    pub clut: Option<[u32; 2]>,
}

impl Tex {
    /// A sprite of screens.json (as the original draws it with 0x8004DB9C): its texels, w, h and screen x, y.
    pub fn sprite(s: &serde_json::Value) -> (Self, u32, u32, f32, f32) {
        let ([vx, vy], bpp) = (pair(&s["vram"]), int(&s["bpp"]).max(4));
        let tex = Self { page: [vx & !63, vy & !255], bpp, u: (vx & 63) * 16 / bpp, v: vy & 255, clut: Some(pair(&s["clut"])) };
        (tex, int(&s["w"]), int(&s["h"]), int(&s["x"]) as f32, int(&s["y"]) as f32)
    }

    /// The texels at VRAM (x words, y) in depth `bpp`.
    pub fn at(vram: [u32; 2], bpp: u32, clut: Option<[u32; 2]>) -> Self {
        Self { page: vram, bpp, u: 0, v: 0, clut }
    }
    pub fn offset(self, du: u32, dv: u32) -> Self {
        Self { u: self.u + du, v: self.v + dv, ..self }
    }
}

/// One of the game's three bitmap fonts (screens.json `fonts`).
pub struct Font {
    pub advance: f32,
    pub w: f32,
    pub h: f32,
    page: [u32; 2],
    bpp: u32,
    clut_base: u32,
    /// Colour c's CLUT: (x + 16 * (c & 15), y + (c >> 4)) with c the colour + `clut_base` (screens.json
    /// `fonts.clut_origin`).
    clut_origin: [u32; 2],
    glyphs: HashMap<char, [u32; 2]>,
}

impl Font {
    pub fn clut(&self, colour: u32) -> [u32; 2] {
        let c = colour + self.clut_base;
        [self.clut_origin[0] + 16 * (c & 15), self.clut_origin[1] + (c >> 4)]
    }
}

pub struct Art {
    ctx: egui::Context,
    /// assets/pictures
    dir: std::path::PathBuf,
    /// Every converted picture used by the screens, by file name relative to `pictures/` ("r009/029.png").
    pics: RefCell<HashMap<String, Option<(TextureHandle, [usize; 2])>>>,
    placed: Vec<Placed>,
    /// CLUT rows by source ("exe", "r009") and VRAM (x, y): 15-bit colours.
    cluts: HashMap<String, HashMap<(u32, u32), Vec<u16>>>,
    /// Pictures coloured with another CLUT, made on first use.
    recoloured: RefCell<HashMap<(String, [u32; 2]), Pic>>,
    /// Index maps' sizes (None: no such map), by file name.
    index_sizes: RefCell<HashMap<String, Option<[usize; 2]>>>,
    pub fonts: Vec<Font>,
    /// gamedata/screens.json: the places of the screens' pictures.
    pub screens: serde_json::Value,
}

/// An original-hardware 15-bit colour (0 = transparent).
fn ps_color(v: u16) -> [u8; 4] {
    if v == 0 {
        return [0; 4];
    }
    [((v & 31) << 3) as u8, (((v >> 5) & 31) << 3) as u8, (((v >> 10) & 31) << 3) as u8, 255]
}

/// Texture modulation colour: the original hardware's 0x80 is "as drawn", so the channels double (and clamp).
pub fn modulate(r: u32, g: u32, b: u32) -> Color32 {
    Color32::from_rgb((r * 2).min(255) as u8, (g * 2).min(255) as u8, (b * 2).min(255) as u8)
}

/// The game's printf numbers (0x8004CE74): each %d, %D or %x (a width, 0 first: padded with zeros) takes the
/// next number; the rest stays.
pub fn printf(fmt: &str, args: &[u32]) -> String {
    let (mut out, mut args, mut rest) = (String::new(), args.iter(), fmt);
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        let spec = &rest[at + 1..];
        let digits = spec.find(|c: char| !c.is_ascii_digit()).unwrap_or(spec.len());
        let conv = spec[digits..].chars().next();
        if !matches!(conv, Some('d' | 'D' | 'x')) {
            out.push('%');
            rest = spec;
            continue;
        }
        let n = args.next().copied().unwrap_or(0);
        let s = if conv == Some('x') { format!("{n:x}") } else { n.to_string() };
        let pad = if spec.starts_with('0') { '0' } else { ' ' };
        // (the width: one digit 1-8, 0x8004D080)
        out.extend(std::iter::repeat_n(pad, spec[..digits].parse::<usize>().unwrap_or(0).min(8).saturating_sub(s.len())));
        out.push_str(&s);
        rest = &spec[digits + 1..];
    }
    out + rest
}

/// An additive (original-hardware semi-transparency mode 1) colour: premultiplied with alpha 0, so it adds to what is
/// below.
pub fn additive(c: Color32) -> Color32 {
    Color32::from_rgba_premultiplied(c.r(), c.g(), c.b(), 0)
}

impl Art {
    /// Load the screens' pictures (`assets/pictures`), their palettes and the fonts (`assets/gamedata/screens.json`).
    pub fn load(ctx: &egui::Context, assets: &Path) -> Self {
        let pictures = assets.join("pictures");
        let read = |p: &Path| -> serde_json::Value { kumite_fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() };
        // the screens' pictures: loaded when first drawn (in a browser fetched then, shown once arrived); their VRAM
        // places from the index
        let mut placed = Vec::new();
        let index = read(&pictures.join("index.json"));
        let u = int;
        for set in index.as_array().into_iter().flatten() {
            // (the sets the screens draw from their VRAM places)
            if set["ui"] != true {
                continue;
            }
            for img in set["images"].as_array().into_iter().flatten() {
                let (Some(file), Some(vram)) = (img["file"].as_str(), img["vram"].as_array()) else { continue };
                let bpp = u(&img["bpp"]);
                // (a depth the original hardware has: another is a bad file, not a palette of 2^n colours)
                if vram.len() < 2 || !matches!(bpp, 4 | 8 | 15 | 16) {
                    continue;
                }
                placed.push(Placed {
                    file: file.to_string(),
                    index: img["index_map"].as_str().map(String::from),
                    source: file.split('/').next().unwrap_or_default().to_string(),
                    x: u(&vram[0]) * 16 / bpp,
                    y: u(&vram[1]),
                    w: u(&img["w"]),
                    h: u(&img["h"]),
                    bpp,
                    clut: img["clut"].as_array().filter(|c| c.len() == 2).map(|c| [u(&c[0]), u(&c[1])]),
                });
            }
        }
        let mut cluts: HashMap<String, HashMap<(u32, u32), Vec<u16>>> = HashMap::new();
        if let Some(sources) = read(&pictures.join("cluts.json"))["cluts"].as_object() {
            for (source, rows) in sources {
                for (xy, colours) in rows.as_object().into_iter().flatten() {
                    let Some((x, y)) = xy.split_once(',').and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?))) else { continue };
                    let row = colours.as_array().into_iter().flatten().map(|c| c.as_u64().unwrap_or(0) as u16).collect();
                    cluts.entry(source.clone()).or_default().insert((x, y), row);
                }
            }
        }
        let screens = read(&assets.join("gamedata/screens.json"));
        let fonts = screens["fonts"]["fonts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| Font {
                advance: u(&f["advance"]) as f32,
                w: u(&f["w"]) as f32,
                h: u(&f["h"]) as f32,
                page: [u(&f["page"][0]), u(&f["page"][1])],
                bpp: u(&f["bpp"]),
                clut_base: u(&f["clut_base"]),
                clut_origin: [u(&screens["fonts"]["clut_origin"][0]), u(&screens["fonts"]["clut_origin"][1])],
                glyphs: f["glyphs"].as_object().into_iter().flatten().filter_map(|(c, uv)| Some((c.chars().next()?, [u(&uv[0]), u(&uv[1])]))).collect(),
            })
            .collect();
        Self {
            ctx: ctx.clone(),
            dir: pictures.clone(),
            pics: RefCell::new(HashMap::new()),
            placed,
            cluts,
            recoloured: RefCell::new(HashMap::new()),
            index_sizes: RefCell::new(HashMap::new()),
            fonts,
            screens,
        }
    }

    /// A picture by file name relative to `pictures/` ("r009/029.png") and its size, loaded on first use. In a
    /// browser one not arrived yet is asked for and None till then (it shows a frame or two later).
    pub fn pic(&self, file: &str) -> Option<(TextureHandle, [usize; 2])> {
        if let Some(p) = self.pics.borrow().get(file) {
            return p.clone();
        }
        let path = self.dir.join(file);
        if !kumite_fs::ready(&[&path]) {
            kumite_fs::prefetch(&[&path]);
            return None;
        }
        let p = load_png(&path).ok().map(|im| {
            let size = im.size;
            (self.ctx.load_texture(file, im, egui::TextureOptions::NEAREST), size)
        });
        self.pics.borrow_mut().insert(file.to_string(), p.clone());
        p
    }

    /// The large portrait of character `id` (pictures/portraits/NN.png).
    pub fn portrait(&self, id: usize) -> Option<TextureHandle> {
        self.pic(&format!("portraits/{id:02}.png")).map(|p| p.0)
    }

    /// The select face of an extra fighter (roster.json `face`, in pictures/): the original's faces are VRAM
    /// pictures, these are plain ones.
    pub fn extra_face_tex(&self, roster: &crate::menu::Roster, id: usize) -> Option<TextureHandle> {
        roster.by_id(id)?.face.as_deref().and_then(|f| self.pic(f)).map(|p| p.0)
    }

    /// A plain picture (a texture of its own) at screen (x, y), w x h.
    #[allow(clippy::too_many_arguments)]
    pub fn image(painter: &egui::Painter, screen: &Screen, tex: &TextureHandle, x: f32, y: f32, w: f32, h: f32, tint: Color32) {
        let uv = egui::Rect::from_min_max(egui::Pos2::new(0.0, 0.0), egui::Pos2::new(1.0, 1.0));
        painter.image(tex.id(), screen.rect(x, y, w, h), uv, tint);
    }

    /// The face of an extra fighter, when it has one: true when drawn.
    #[allow(clippy::too_many_arguments)]
    pub fn extra_face(&self, painter: &egui::Painter, screen: &Screen, roster: &crate::menu::Roster, id: usize, x: f32, y: f32, h: f32) -> bool {
        let Some(t) = self.extra_face_tex(roster, id) else { return false };
        Self::image(painter, screen, &t, x, y, 32.0, h, Color32::WHITE);
        true
    }

    /// The CLUT row at VRAM `at` (n colours from there), from the last of `sources` that has it.
    fn clut(&self, sources: &[&str], at: [u32; 2], n: usize) -> Option<Vec<u16>> {
        for s in sources.iter().rev() {
            let Some(rows) = self.cluts.get(*s) else { continue };
            for ((x, y), row) in rows {
                if *y == at[1] && *x <= at[0] && ((at[0] - x) as usize) < row.len() {
                    let from = (at[0] - x) as usize;
                    let mut out: Vec<u16> = row[from..].iter().copied().take(n).collect();
                    out.resize(n, 0);
                    return Some(out);
                }
            }
        }
        None
    }

    /// The texture and texel offset of `w` x `h` texels at `t`: the picture of `sources` (later ones first) that
    /// holds them, recoloured when `t` asks for another CLUT than its own.
    fn texels(&self, sources: &[&str], t: &Tex, w: u32, h: u32) -> Option<(egui::TextureId, [usize; 2], [u32; 2])> {
        // (numbers from screens.json: a depth of 0 or a page past VRAM finds nothing, not a division by zero)
        let (px, py) = (t.page[0].checked_mul(16)?.checked_div(t.bpp)?.checked_add(t.u)?, t.page[1].checked_add(t.v)?);
        let (pxe, pye) = (px.checked_add(w)?, py.checked_add(h)?);
        // the picture holding the whole region; where uploads overlap in VRAM the later one wins
        let pic = sources.iter().rev().find_map(|s| self.placed.iter().rev().find(|p| p.source == *s && p.bpp == t.bpp && px >= p.x && py >= p.y && pxe <= p.x + p.w && pye <= p.y + p.h))?;
        let off = [px - pic.x, py - pic.y];
        let clut = match t.clut {
            Some(c) if Some(c) != pic.clut && pic.index.is_some() => c,
            _ => return self.pic(&pic.file).map(|(tex, size)| (tex.id(), size, off)),
        };
        let index = pic.index.clone()?;
        let (id, size) = self.recolour(&index, clut, t.bpp, sources)?;
        Some((id, size, off))
    }

    /// Index map `index` (relative to pictures/) coloured with the CLUT at VRAM `clut`, made once.
    fn recolour(&self, index: &str, clut: [u32; 2], bpp: u32, sources: &[&str]) -> Option<(egui::TextureId, [usize; 2])> {
        let key = (index.to_string(), clut);
        if let Some((tex, size)) = self.recoloured.borrow().get(&key) {
            return Some((tex.id(), *size));
        }
        let colours = self.clut(sources, clut, 1 << bpp)?;
        let gray = load_gray(&self.dir.join(index))?;
        let rgba: Vec<u8> = gray.1.iter().flat_map(|&i| ps_color(colours.get(i as usize).copied().unwrap_or(0))).collect();
        let img = egui::ColorImage::from_rgba_unmultiplied(gray.0, &rgba);
        let tex = self.ctx.load_texture(format!("{index}@{},{}", clut[0], clut[1]), img, egui::TextureOptions::NEAREST);
        let out = (tex.id(), gray.0);
        self.recoloured.borrow_mut().insert(key, (tex, gray.0));
        Some(out)
    }

    /// Size of a 4-bit index map (e.g. a fighter's name picture), loading it with its first CLUT use.
    pub fn indexed_size(&self, index: &str) -> Option<[usize; 2]> {
        // (one missing is asked for once: an extra fighter has none, and the HUD asks every frame)
        if let Some(size) = self.index_sizes.borrow().get(index) {
            return *size;
        }
        let size = load_gray(&self.dir.join(index)).map(|g| g.0);
        self.index_sizes.borrow_mut().insert(index.to_string(), size);
        size
    }

    /// A 4-bit index map drawn at its own size at screen (x, y) with the CLUT at VRAM `clut`.
    #[allow(clippy::too_many_arguments)]
    pub fn indexed(&self, painter: &egui::Painter, screen: &Screen, sources: &[&str], index: &str, clut: [u32; 2], x: f32, y: f32) {
        let Some((id, [w, h])) = self.recolour(index, clut, 4, sources) else { return };
        painter.image(id, screen.rect(x, y, w as f32, h as f32), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    }

    /// Rows `row0 .. row0 + rows` of a 4-bit index map at screen (x, y) (a sheet of several names).
    #[allow(clippy::too_many_arguments)]
    pub fn indexed_rows(&self, painter: &egui::Painter, screen: &Screen, sources: &[&str], index: &str, clut: [u32; 2], x: f32, y: f32, row0: usize, rows: usize) {
        let Some((id, [w, h])) = self.recolour(index, clut, 4, sources) else { return };
        let (v0, v1) = (row0 as f32 / h as f32, ((row0 + rows) as f32 / h as f32).min(1.0));
        painter.image(id, screen.rect(x, y, w as f32, rows as f32), Rect::from_min_max(Pos2::new(0.0, v0), Pos2::new(1.0, v1)), Color32::WHITE);
    }

    /// An original-hardware sprite: `w` x `h` texels at `t` drawn into the screen rectangle (x, y, dw, dh) (a negative
    /// `dw` mirrors), with per-corner colours (top-left, top-right, bottom-left, bottom-right).
    #[allow(clippy::too_many_arguments)]
    pub fn quad(&self, painter: &egui::Painter, screen: &Screen, sources: &[&str], t: &Tex, w: u32, h: u32, dst: [f32; 4], colours: [Color32; 4]) {
        let Some((id, size, off)) = self.texels(sources, t, w, h) else { return };
        let (sw, sh) = (size[0] as f32, size[1] as f32);
        let (u0, v0) = (off[0] as f32 / sw, off[1] as f32 / sh);
        let (u1, v1) = ((off[0] + w) as f32 / sw, (off[1] + h) as f32 / sh);
        let [x, y, dw, dh] = dst;
        let (xa, xb) = if dw < 0.0 { (x - dw, x) } else { (x, x + dw) };
        let mut mesh = egui::Mesh::with_texture(id);
        let corners = [(xa, y, u0, v0), (xb, y, u1, v0), (xa, y + dh, u0, v1), (xb, y + dh, u1, v1)];
        for (k, (cx, cy, cu, cv)) in corners.into_iter().enumerate() {
            mesh.vertices.push(egui::epaint::Vertex { pos: screen.pos(cx, cy), uv: Pos2::new(cu, cv), color: colours[k] });
        }
        mesh.indices.extend([0, 1, 2, 1, 3, 2]);
        painter.add(egui::Shape::mesh(mesh));
    }

    /// A sprite at its own size at screen (x, y).
    #[allow(clippy::too_many_arguments)]
    pub fn sprite(&self, painter: &egui::Painter, screen: &Screen, sources: &[&str], t: &Tex, w: u32, h: u32, x: f32, y: f32, tint: Color32) {
        self.quad(painter, screen, sources, t, w, h, [x, y, w as f32, h as f32], [tint; 4]);
    }

    /// `text` in font `font`, colour `colour` (the printf `%c`), top-left at screen (x, y).
    #[allow(clippy::too_many_arguments)]
    /// `text` at `scale` of the font's size (OpenKumite's own small lines: online's ping).
    #[allow(clippy::too_many_arguments)]
    pub fn text_scaled(&self, painter: &egui::Painter, screen: &Screen, font: usize, colour: u32, x: f32, y: f32, scale: f32, text: &str, tint: Color32) {
        let Some(f) = self.fonts.get(font) else { return };
        let clut = f.clut(colour);
        let mut cx = x;
        for ch in text.chars() {
            if let Some(&[u, v]) = f.glyphs.get(&ch) {
                let t = Tex { page: f.page, bpp: f.bpp, u, v, clut: Some(clut) };
                self.quad(painter, screen, &["exe"], &t, f.w as u32, f.h as u32, [cx, y, f.w * scale, f.h * scale], [tint; 4]);
            }
            cx += f.advance * scale;
        }
    }

    pub fn text(&self, painter: &egui::Painter, screen: &Screen, font: usize, colour: u32, x: f32, y: f32, text: &str, tint: Color32) {
        let Some(f) = self.fonts.get(font) else { return };
        let clut = f.clut(colour);
        let mut cx = x;
        for ch in text.chars() {
            if let Some(&[u, v]) = f.glyphs.get(&ch) {
                let t = Tex { page: f.page, bpp: f.bpp, u, v, clut: Some(clut) };
                self.sprite(painter, screen, &["exe"], &t, f.w as u32, f.h as u32, cx, y, tint);
            }
            cx += f.advance;
        }
    }

    /// Font `font`'s CLUT for `colour` (the printf `%c`).
    pub fn font_clut(&self, font: usize, colour: u32) -> [u32; 2] {
        self.fonts.get(font).map_or([0, 0], |f| f.clut(colour))
    }

    /// Width of `text` in font `font` (the advance of every character).
    pub fn text_w(&self, font: usize, text: &str) -> f32 {
        self.fonts.get(font).map_or(0.0, |f| f.advance * text.chars().count() as f32)
    }

    /// A flat quad with per-corner colours (top-left, top-right, bottom-left, bottom-right).
    pub fn shade(painter: &egui::Painter, screen: &Screen, dst: [f32; 4], colours: [Color32; 4]) {
        let [x, y, w, h] = dst;
        let mut mesh = egui::Mesh::default();
        for (k, (cx, cy)) in [(x, y), (x + w, y), (x, y + h), (x + w, y + h)].into_iter().enumerate() {
            mesh.colored_vertex(screen.pos(cx, cy), colours[k]);
        }
        mesh.indices.extend([0, 1, 2, 1, 3, 2]);
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// Decode a PNG (any colour type) to an egui image.
pub fn load_png(path: &Path) -> anyhow::Result<egui::ColorImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(kumite_fs::read(path)?));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or_else(|| anyhow::anyhow!("png too large"))?];
    let info = reader.next_frame(&mut buf)?;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => anyhow::bail!("indexed png not expanded"),
    };
    Ok(egui::ColorImage::from_rgba_unmultiplied([info.width as usize, info.height as usize], &rgba))
}

/// A grayscale PNG's values (the converter's index maps): size and one byte per pixel.
fn load_gray(path: &Path) -> Option<([usize; 2], Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(kumite_fs::read(path).ok()?));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    (info.color_type == png::ColorType::Grayscale).then(|| ([info.width as usize, info.height as usize], buf[..info.buffer_size()].to_vec()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_index_map_read_once() {
        let dir = std::env::temp_dir().join(format!("openkumite_art_test_{}", std::process::id()));
        let art = super::Art::load(&egui::Context::default(), &dir);
        assert_eq!(art.indexed_size("names/x.png"), None);
        // (arriving later is not seen: the miss is remembered)
        std::fs::create_dir_all(dir.join("pictures/names")).unwrap();
        let mut enc = png::Encoder::new(std::fs::File::create(dir.join("pictures/names/x.png")).unwrap(), 2, 1);
        enc.set_color(png::ColorType::Grayscale);
        enc.write_header().unwrap().write_image_data(&[0, 1]).unwrap();
        assert_eq!(art.indexed_size("names/x.png"), None);
        assert_eq!(super::Art::load(&egui::Context::default(), &dir).indexed_size("names/x.png"), Some([2, 1]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn printf_numbers() {
        assert_eq!(super::printf("A : %5D", &[300]), "A :   300");
        assert_eq!(super::printf("%02d'%02d\"%02d", &[1, 2, 34]), "01'02\"34");
        assert_eq!(super::printf("%1x %s", &[10]), "a %s");
        assert_eq!(super::printf("100%", &[]), "100%");
    }
}
