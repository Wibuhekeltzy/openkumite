//! The graphics settings (OpenKumite's own): window or full screen, the resolution, the 3D render scale, vsync and
//! the texture filter. Kept in the save, changed in OPTION > GRAPHICS or on the command line:
//!
//! `--fullscreen` (the display's own mode at `--resolution`), `--borderless` (a window covering the screen),
//! `--windowed`, `--graphics recommended|default` (as the menu's rows: chosen for the screen and the GPU, or the
//! defaults; the other options apply over it), `--resolution WxH` (the window's size, or the full-screen video mode), `--render-scale S`
//! (0.25 .. 2: the 3D scene's resolution over the window's), `--vsync on|off`, `--textures sharp|smooth`,
//! `--stats` (the video statistics: FPS, frame times, resolutions, the GPU; F10 in the game).
//!
//! Only what the display and the GPU can show is offered ([`Display`]): the monitor's video modes for full screen,
//! window sizes that fit the screen, frames and 3D scenes within the GPU's largest texture. In a browser: a canvas
//! in the page (sizes that fit the page) or the browser's full screen; vsync is the browser's.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use winit::dpi::PhysicalSize;
use winit::window::{Fullscreen, Window, WindowAttributes};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMode {
    #[default]
    Window,
    /// A borderless window covering the display (at its own resolution).
    Borderless,
    /// The display switched to the chosen resolution (exclusive).
    Fullscreen,
}

impl DisplayMode {
    pub const ALL: [DisplayMode; 3] = [DisplayMode::Window, DisplayMode::Borderless, DisplayMode::Fullscreen];

    pub fn label(self) -> &'static str {
        match self {
            DisplayMode::Window => "WINDOW",
            // (in a browser the only full screen: the browser's)
            DisplayMode::Borderless if cfg!(target_arch = "wasm32") => "FULLSCREEN",
            DisplayMode::Borderless => "BORDERLESS",
            DisplayMode::Fullscreen => "FULLSCREEN",
        }
    }
}

/// The render scales offered (the 3D scene's resolution over the window's).
pub const SCALES: [f32; 6] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0];
/// Common window sizes (logical pixels), offered where they fit the screen and the GPU.
const WINDOW_SIZES: [[u32; 2]; 19] = [
    [640, 480],
    [800, 600],
    [960, 720],
    [1024, 768],
    [1280, 720],
    [1280, 800],
    [1280, 960],
    [1366, 768],
    [1440, 900],
    [1440, 1080],
    [1600, 900],
    [1600, 1200],
    [1680, 1050],
    [1920, 1080],
    [1920, 1200],
    [1920, 1440],
    [2560, 1440],
    [2560, 1600],
    [3840, 2160],
];
/// Room kept for the window's frame and the desktop's bars (logical pixels).
#[cfg(not(target_arch = "wasm32"))]
const FRAME_MARGIN: [u32; 2] = [0, 96];

/// What the display and the GPU allow: the screen, the room for a window, the video modes, the largest texture.
#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    /// The screen's size (physical pixels), when known.
    pub screen: Option<[u32; 2]>,
    /// The largest window (logical pixels): the screen less the frame; in a browser the page's view.
    pub room: Option<[u32; 2]>,
    /// Physical pixels per logical one.
    pub scale_factor: f64,
    /// The screen's video modes (physical pixels; exclusive full screen).
    pub modes: Vec<[u32; 2]>,
    /// The GPU's largest texture side: the frame and the 3D scene's target are at most this.
    pub max_texture: u32,
    /// The GPU's kind (for the recommended settings).
    pub gpu: Gpu,
}

/// A set of settings to start from: chosen for the screen and the GPU, or the defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Recommended,
    Default,
}

impl Preset {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "recommended" | "auto" => Ok(Preset::Recommended),
            "default" | "defaults" | "reset" => Ok(Preset::Default),
            _ => anyhow::bail!("--graphics recommended|default"),
        }
    }
}

/// The GPU's kind, for the recommended 3D scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gpu {
    Discrete,
    Integrated,
    /// Software rendering (a CPU).
    Software,
    /// Not told (a browser, a virtual machine).
    #[default]
    Unknown,
}

impl Gpu {
    pub fn of(t: kumite_render::wgpu::DeviceType) -> Self {
        use kumite_render::wgpu::DeviceType;
        match t {
            DeviceType::DiscreteGpu => Gpu::Discrete,
            DeviceType::IntegratedGpu => Gpu::Integrated,
            DeviceType::Cpu => Gpu::Software,
            DeviceType::VirtualGpu | DeviceType::Other => Gpu::Unknown,
        }
    }

    /// The 3D scene's pixels this kind of GPU draws comfortably at 60 frames a second (the game's scenes are
    /// light: the budget is in fill rate).
    fn pixel_budget(self) -> u64 {
        match self {
            Gpu::Discrete => 3840 * 2160 * 2,
            Gpu::Integrated => 3840 * 2160,
            Gpu::Unknown => 2560 * 1600,
            Gpu::Software => 640 * 480,
        }
    }
}

impl Display {
    /// Before the display is known: everything allowed.
    pub fn unknown() -> Self {
        Self { screen: None, room: None, scale_factor: 1.0, modes: Vec::new(), max_texture: u32::MAX, gpu: Gpu::Unknown }
    }

    /// The display a window is on (or would open on) and the GPU's texture limit (u32::MAX before the GPU is known).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn query(monitor: Option<winit::monitor::MonitorHandle>, max_texture: u32) -> Self {
        let Some(m) = monitor else {
            return Self { screen: None, room: None, scale_factor: 1.0, modes: Vec::new(), max_texture, gpu: Gpu::Unknown };
        };
        let sf = m.scale_factor().max(0.1);
        let size = m.size();
        let logical = [(size.width as f64 / sf) as u32, (size.height as f64 / sf) as u32];
        let mut modes: Vec<[u32; 2]> = m.video_modes().map(|v| [v.size().width, v.size().height]).collect();
        modes.sort();
        modes.dedup();
        Self {
            screen: Some([size.width, size.height]),
            room: Some([logical[0].saturating_sub(FRAME_MARGIN[0]), logical[1].saturating_sub(FRAME_MARGIN[1])]),
            scale_factor: sf,
            modes,
            max_texture,
            gpu: Gpu::Unknown,
        }
    }

    /// In a browser: the screen (full screen) and the page's view (the canvas's room); no video modes.
    #[cfg(target_arch = "wasm32")]
    pub fn query(_monitor: Option<winit::monitor::MonitorHandle>, max_texture: u32) -> Self {
        let w = web_sys::window();
        let sf = w.as_ref().map_or(1.0, |w| w.device_pixel_ratio()).max(0.1);
        let num = |v: Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>| v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
        let room = w.as_ref().map(|w| [num(w.inner_width()) as u32, num(w.inner_height()) as u32]).filter(|r| r[0] > 0 && r[1] > 0);
        let screen = w.as_ref().and_then(|w| w.screen().ok()).and_then(|s| Some([(s.width().ok()? as f64 * sf) as u32, (s.height().ok()? as f64 * sf) as u32]));
        Self { screen, room, scale_factor: sf, modes: Vec::new(), max_texture, gpu: Gpu::Unknown }
    }

    /// The settings recommended for this screen and GPU: full screen at the screen's own resolution (its video
    /// mode, else borderless; in a browser the largest canvas the page holds), the largest 3D scale the GPU draws
    /// comfortably (supersampling up to 2X), vsync on, the original's sharp textures.
    pub fn recommended(&self) -> Graphics {
        let mut g = Graphics::default();
        if cfg!(target_arch = "wasm32") {
            g.display = DisplayMode::Window;
            if let Some(&largest) = self.resolutions(DisplayMode::Window).iter().max_by_key(|r| r[0] as u64 * r[1] as u64) {
                g.resolution = largest;
            }
        } else if let Some(screen) = self.screen {
            if self.modes.contains(&screen) && self.fits_gpu(screen) {
                g.display = DisplayMode::Fullscreen;
                g.resolution = screen;
            } else {
                g.display = DisplayMode::Borderless;
            }
        }
        let g = self.fit(g);
        let f = self.frame(&g);
        let pixels = f[0] as u64 * f[1] as u64;
        let budget = self.gpu.pixel_budget();
        let scale = SCALES.iter().rev().copied().find(|&s| (pixels as f64 * (s * s) as f64) <= budget as f64 && self.scale_fits(&g, s)).unwrap_or(SCALES[0]);
        Graphics { scale, ..g }
    }

    pub fn preset(&self, p: Preset) -> Graphics {
        match p {
            Preset::Recommended => self.recommended(),
            Preset::Default => self.defaults(),
        }
    }

    /// A preset keeping `g`'s statistics overlay (a tool, not a picture setting).
    pub fn preset_over(&self, p: Preset, g: &Graphics) -> Graphics {
        Graphics { stats: g.stats, ..self.preset(p) }
    }

    /// The defaults (a 960x720 window at 1X), as this display can show them.
    pub fn defaults(&self) -> Graphics {
        self.fit(Graphics::default())
    }

    /// The display modes offered: in a browser a window in the page or full screen (the browser's: borderless).
    pub fn display_modes(&self) -> &'static [DisplayMode] {
        if cfg!(target_arch = "wasm32") || self.modes.is_empty() {
            &[DisplayMode::Window, DisplayMode::Borderless]
        } else {
            &DisplayMode::ALL
        }
    }

    fn fits_gpu(&self, physical: [u32; 2]) -> bool {
        physical[0] <= self.max_texture && physical[1] <= self.max_texture
    }

    /// The resolutions to choose from for a display mode (smallest first; none: the screen's own, borderless).
    pub fn resolutions(&self, mode: DisplayMode) -> Vec<[u32; 2]> {
        match mode {
            DisplayMode::Borderless => Vec::new(),
            DisplayMode::Fullscreen => self.modes.iter().copied().filter(|&m| self.fits_gpu(m)).collect(),
            DisplayMode::Window => {
                let sf = self.scale_factor;
                let physical = |r: [u32; 2]| [(r[0] as f64 * sf).round() as u32, (r[1] as f64 * sf).round() as u32];
                // the common sizes and the screen's own modes (in logical pixels)
                let mut out: Vec<[u32; 2]> = WINDOW_SIZES
                    .iter()
                    .copied()
                    .chain(self.modes.iter().map(|m| [(m[0] as f64 / sf) as u32, (m[1] as f64 / sf) as u32]))
                    .filter(|&r| r[0] >= 640 && r[1] >= 480 && self.fits_gpu(physical(r)))
                    .filter(|&r| self.room.is_none_or(|room| r[0] <= room[0] && r[1] <= room[1]))
                    .collect();
                out.sort();
                out.dedup();
                if out.is_empty() {
                    // a small screen or page: the largest 4:3 window in it
                    if let Some([w, h]) = self.room {
                        let h = h.min(w * 3 / 4).max(1);
                        out.push([h * 4 / 3, h]);
                    }
                }
                out
            }
        }
    }

    /// Settings this display can show: a display mode it offers, a resolution from its list (the nearest at most
    /// as large, else the smallest), the render scale within the GPU's texture limit.
    pub fn fit(&self, mut g: Graphics) -> Graphics {
        let modes = self.display_modes();
        if !modes.contains(&g.display) {
            g.display = if g.display == DisplayMode::Fullscreen { DisplayMode::Borderless } else { DisplayMode::Window };
        }
        let list = self.resolutions(g.display);
        if !list.is_empty() && !list.contains(&g.resolution) {
            let area = |r: &[u32; 2]| r[0] as u64 * r[1] as u64;
            g.resolution = list.iter().filter(|r| r[0] <= g.resolution[0] && r[1] <= g.resolution[1]).max_by_key(|r| area(r)).or_else(|| list.first()).copied().unwrap_or(g.resolution);
        }
        while g.scale > SCALES[0] && !self.scale_fits(&g, g.scale) {
            g.scale = SCALES.iter().rev().copied().find(|&s| s < g.scale).unwrap_or(SCALES[0]);
        }
        g
    }

    /// The frame's size in physical pixels for these settings.
    fn frame(&self, g: &Graphics) -> [u32; 2] {
        match g.display {
            DisplayMode::Borderless => self.screen.unwrap_or(g.resolution),
            DisplayMode::Fullscreen => g.resolution,
            DisplayMode::Window => [(g.resolution[0] as f64 * self.scale_factor) as u32, (g.resolution[1] as f64 * self.scale_factor) as u32],
        }
    }

    /// Whether the 3D scene at this render scale fits the GPU's textures.
    pub fn scale_fits(&self, g: &Graphics, scale: f32) -> bool {
        let f = self.frame(g);
        self.fits_gpu([(f[0] as f32 * scale).round() as u32, (f[1] as f32 * scale).round() as u32])
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Graphics {
    pub display: DisplayMode,
    /// The window's size (logical pixels), or the full-screen video mode.
    pub resolution: [u32; 2],
    pub scale: f32,
    pub vsync: bool,
    pub smooth_textures: bool,
    /// The video statistics in the corner (FPS, frame times, resolutions, the GPU).
    pub stats: bool,
}

impl Default for Graphics {
    fn default() -> Self {
        Self { display: DisplayMode::Window, resolution: [960, 720], scale: 1.0, vsync: true, smooth_textures: false, stats: false }
    }
}

/// A window's (or video mode's) smallest and largest side, in pixels.
pub const MIN_SIDE: u32 = 240;
pub const MAX_SIDE: u32 = 16384;

impl Graphics {
    /// In range whatever the save or the command line said: the render scale one of 0.25..2 (not a number: 1),
    /// the resolution's sides [`MIN_SIDE`]..[`MAX_SIDE`].
    pub fn sane(mut self) -> Self {
        self.scale = if self.scale.is_finite() { self.scale.clamp(SCALES[0], SCALES[SCALES.len() - 1]) } else { 1.0 };
        self.resolution = self.resolution.map(|d| d.clamp(MIN_SIDE, MAX_SIDE));
        self
    }

    pub fn render(&self) -> kumite_render::RenderSettings {
        kumite_render::RenderSettings { scale: self.scale, vsync: self.vsync, smooth_textures: self.smooth_textures }
    }

    /// The command line's settings over these (`arg`: an option's value, `flag`: whether it is given).
    pub fn with_args(mut self, arg: impl Fn(&str) -> Option<String>, flag: impl Fn(&str) -> bool) -> Result<Self> {
        if flag("--fullscreen") {
            self.display = DisplayMode::Fullscreen;
        }
        if flag("--borderless") {
            self.display = DisplayMode::Borderless;
        }
        if flag("--windowed") {
            self.display = DisplayMode::Window;
        }
        if let Some(r) = arg("--resolution") {
            let (w, h) = r.split_once(['x', 'X']).context("--resolution WxH")?;
            self.resolution = [w.trim().parse().context("--resolution WxH")?, h.trim().parse().context("--resolution WxH")?];
        }
        if let Some(s) = arg("--render-scale") {
            self.scale = s.trim_end_matches('%').parse::<f32>().context("--render-scale 0.25..2")?;
            if s.ends_with('%') {
                self.scale /= 100.0;
            }
            self.scale = self.scale.clamp(0.25, 2.0);
        }
        if let Some(v) = arg("--vsync") {
            self.vsync = matches!(v.as_str(), "on" | "1" | "true" | "yes");
        }
        if flag("--stats") {
            self.stats = true;
        }
        if let Some(t) = arg("--textures") {
            self.smooth_textures = matches!(t.as_str(), "smooth" | "linear" | "filtered");
        }
        Ok(self.sane())
    }

    /// The full-screen state: the exclusive video mode at the resolution (the best refresh rate), or a borderless one
    /// (in a browser: the canvas full screen).
    fn fullscreen(&self, monitor: Option<winit::monitor::MonitorHandle>) -> Option<Fullscreen> {
        match self.display {
            DisplayMode::Window => None,
            DisplayMode::Borderless => Some(Fullscreen::Borderless(None)),
            DisplayMode::Fullscreen => {
                let mode = monitor
                    .and_then(|m| m.video_modes().filter(|v| v.size() == PhysicalSize::new(self.resolution[0], self.resolution[1])).max_by_key(|v| (v.refresh_rate_millihertz(), v.bit_depth())));
                Some(mode.map_or(Fullscreen::Borderless(None), Fullscreen::Exclusive))
            }
        }
    }

    /// The new window's attributes (on `monitor`).
    pub fn window_attributes(&self, attrs: WindowAttributes, monitor: Option<winit::monitor::MonitorHandle>) -> WindowAttributes {
        let [w, h] = self.resolution;
        attrs.with_inner_size(winit::dpi::LogicalSize::new(w as f64, h as f64)).with_fullscreen(self.fullscreen(monitor))
    }

    /// Apply to a running window: full screen or not, and the window's size.
    pub fn apply(&self, window: &Window) {
        let fs = self.fullscreen(window.current_monitor());
        let windowed = fs.is_none();
        // (leaving full screen only when in it: a browser throws on leaving it otherwise)
        if !(windowed && window.fullscreen().is_none()) {
            window.set_fullscreen(fs);
        }
        if windowed {
            let [w, h] = self.resolution;
            let _ = window.request_inner_size(winit::dpi::LogicalSize::new(w as f64, h as f64));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2880x1800 Retina screen (1440x900 logical), its modes; a GPU's texture limit.
    fn retina(max_texture: u32) -> Display {
        Display {
            screen: Some([2880, 1800]),
            room: Some([1440, 900 - FRAME_MARGIN[1]]),
            scale_factor: 2.0,
            modes: vec![[1024, 640], [1440, 900], [1680, 1050], [2880, 1800]],
            max_texture,
            gpu: Gpu::Integrated,
        }
    }

    #[test]
    fn windows_fit_the_screen_and_the_gpu() {
        let d = retina(8192);
        let list = d.resolutions(DisplayMode::Window);
        assert!(list.iter().all(|r| r[0] <= 1440 && r[1] <= 900 - FRAME_MARGIN[1]), "{list:?}");
        assert!(list.contains(&[960, 720]) && !list.contains(&[1920, 1440]));
        // a saved size too large for this screen: the nearest that fits
        let g = d.fit(Graphics { resolution: [1920, 1440], ..Default::default() });
        assert_eq!(g.resolution, [1366, 768]);
        // a GPU with 2048 textures: windows at most 1024 logical on a 2x screen
        let small = retina(2048).resolutions(DisplayMode::Window);
        assert!(small.iter().all(|r| r[0] <= 1024 && r[1] <= 1024), "{small:?}");
    }

    #[test]
    fn full_screen_takes_the_monitors_modes() {
        let d = retina(2048);
        assert_eq!(d.resolutions(DisplayMode::Fullscreen), vec![[1024, 640], [1440, 900], [1680, 1050]]);
        let g = d.fit(Graphics { display: DisplayMode::Fullscreen, resolution: [1920, 1080], scale: 2.0, ..Default::default() });
        assert_eq!(g.resolution, [1680, 1050]);
        // the 3D scene within the GPU's textures: 1680 * 1 fits 2048, 1.5 does not
        assert_eq!(g.scale, 1.0);
        // no video modes known: no exclusive full screen (borderless instead)
        let none = Display { modes: Vec::new(), ..retina(8192) };
        assert_eq!(none.fit(Graphics { display: DisplayMode::Fullscreen, ..Default::default() }).display, DisplayMode::Borderless);
    }

    #[test]
    fn recommended_by_the_screen_and_the_gpu() {
        // the screen's own mode, full screen; 2880x1800 on an integrated GPU: 1X (1.5X would be 11.7M pixels)
        let d = retina(16384);
        let g = d.recommended();
        assert_eq!((g.display, g.resolution, g.scale), (DisplayMode::Fullscreen, [2880, 1800], 1.0));
        // a 1920x1200 screen: 1.5X on an integrated GPU, 2X on a discrete one, below 1X in software
        let hd = Display { screen: Some([1920, 1200]), modes: vec![[1920, 1200]], scale_factor: 1.0, ..retina(16384) };
        assert_eq!(hd.recommended().scale, 1.5);
        assert_eq!(Display { gpu: Gpu::Discrete, ..hd.clone() }.recommended().scale, 2.0);
        assert!(Display { gpu: Gpu::Software, ..hd.clone() }.recommended().scale < 1.0);
        // no such video mode: borderless
        assert_eq!(Display { modes: vec![[1280, 800]], ..hd.clone() }.recommended().display, DisplayMode::Borderless);
        assert_eq!(hd.defaults(), Graphics::default());
    }
}
