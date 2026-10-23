//! Audio in a browser: the converted FLAC / WAV files played by HTML audio elements from the page's server, or from
//! a conversion kept in the browser (`kumite_fs::src`) (the same API as the native player). Browsers start sound only
//! after the first key press or touch; some (Safari on iPhone) only on elements first played in one, so the page
//! unlocks a music element and a pool for the sounds on the first touch (`window.__okaudio`), and those are the ones
//! played.

use std::path::{Path, PathBuf};
use wasm_bindgen::JsCast;
use web_sys::HtmlAudioElement;

/// The page's unlocked elements (`window.__okaudio = { music, sfx: [...] }`), if it has them.
fn page_elements() -> Option<(HtmlAudioElement, Vec<HtmlAudioElement>)> {
    let w = web_sys::window()?;
    let a = js_sys::Reflect::get(&w, &"__okaudio".into()).ok().filter(|v| v.is_object())?;
    let music = js_sys::Reflect::get(&a, &"music".into()).ok()?.dyn_into().ok()?;
    let sfx: js_sys::Array = js_sys::Reflect::get(&a, &"sfx".into()).ok()?.dyn_into().ok()?;
    Some((music, sfx.iter().filter_map(|e| e.dyn_into().ok()).collect()))
}

pub struct Audio {
    music: Option<HtmlAudioElement>,
    /// The page's sound elements, played in turn (none: a new element a sound).
    pool: Vec<HtmlAudioElement>,
    next: usize,
    /// The track asked for, looped or not, and whether it has started (it waits for the first key press).
    current: Option<PathBuf>,
    looped: bool,
    started: bool,
    pub muted: bool,
    pub music_volume: f32,
    pub sfx_volume: f32,
}

fn url(path: &Path) -> String {
    kumite_fs::src(path)
}

/// The page may play sound: the player has clicked, touched or pressed a key (browsers block sound before that):
/// `navigator.userActivation`, or where a browser has none, the page's own flag (`window.__okgesture`).
fn allowed() -> bool {
    let Some(w) = web_sys::window() else { return false };
    let nav = w.navigator();
    let has = js_sys::Reflect::get(&nav, &"userActivation".into()).is_ok_and(|v| v.is_object());
    if has {
        return nav.user_activation().has_been_active();
    }
    js_sys::Reflect::get(&w, &"__okgesture".into()).is_ok_and(|v| v.is_truthy())
}

impl Audio {
    pub fn new() -> Option<Self> {
        Some(Self { music: None, pool: Vec::new(), next: 0, current: None, looped: false, started: false, muted: false, music_volume: 0.55, sfx_volume: 0.9 })
    }

    /// Play `path` as music, looped or once; None stops the music.
    pub fn music(&mut self, track: Option<(&Path, bool)>) {
        let (path, looped) = (track.map(|t| t.0), track.is_some_and(|t| t.1));
        if self.current.as_deref() == path {
            return;
        }
        if let Some(m) = &self.music {
            let _ = m.pause();
        }
        self.current = path.map(Path::to_path_buf);
        self.looped = looped;
        self.started = false;
        self.resume();
    }

    /// Start the asked-for track once the page may play sound (called every frame).
    pub fn resume(&mut self) {
        if self.started || !allowed() {
            return;
        }
        self.started = true;
        let Some(path) = &self.current else { return };
        // the page's unlocked element (its source changed), else a new one
        let page = page_elements();
        if let Some((music, pool)) = page.filter(|_| self.pool.is_empty() || self.music.is_some()) {
            if self.pool.is_empty() {
                self.pool = pool;
            }
            self.music.get_or_insert(music);
        }
        let m = match self.music.clone() {
            Some(m) if page_elements().is_some() => {
                m.set_src(&url(path));
                m
            }
            _ => match HtmlAudioElement::new_with_src(&url(path)) {
                Ok(m) => m,
                Err(_) => return,
            },
        };
        m.set_loop(self.looped);
        m.set_volume(if self.muted { 0.0 } else { self.music_volume as f64 });
        quiet(m.play());
        self.music = Some(m);
    }

    /// Start a one-shot sound.
    pub fn sfx(&mut self, path: &Path, volume: f32) {
        if self.muted || !allowed() {
            return;
        }
        if self.pool.is_empty() {
            if let Some((_, pool)) = page_elements() {
                self.pool = pool;
            }
        }
        // the page's unlocked elements in turn, else a new one
        let s = match self.pool.get(self.next % self.pool.len().max(1)) {
            Some(e) => {
                self.next += 1;
                e.set_src(&url(path));
                e.clone()
            }
            None => match HtmlAudioElement::new_with_src(&url(path)) {
                Ok(e) => e,
                Err(_) => return,
            },
        };
        s.set_volume((volume * self.sfx_volume).clamp(0.0, 1.0) as f64);
        quiet(s.play());
    }

    pub fn toggle_mute(&mut self) -> bool {
        self.muted = !self.muted;
        if let Some(m) = &self.music {
            m.set_volume(if self.muted { 0.0 } else { self.music_volume as f64 });
        }
        self.muted
    }

    /// Music quieter while paused.
    pub fn duck(&mut self, ducked: bool) {
        if let Some(m) = &self.music {
            m.set_volume(if self.muted {
                0.0
            } else if ducked {
                self.music_volume as f64 * 0.3
            } else {
                self.music_volume as f64
            });
        }
    }
}

/// A `play()` interrupted (the music changed at once: nothing to report) or refused (the log says why; the page
/// shows unhandled rejections as errors).
fn quiet(p: Result<js_sys::Promise, wasm_bindgen::JsValue>) {
    thread_local! {
        static IGNORE: wasm_bindgen::closure::Closure<dyn FnMut(wasm_bindgen::JsValue)> = wasm_bindgen::closure::Closure::new(|e: wasm_bindgen::JsValue| {
            let name = js_sys::Reflect::get(&e, &"name".into()).ok().and_then(|n| n.as_string()).unwrap_or_default();
            if name != "AbortError" {
                log::warn!("sound refused: {name} {}", js_sys::Reflect::get(&e, &"message".into()).ok().and_then(|m| m.as_string()).unwrap_or_default());
            }
        });
    }
    if let Ok(p) = p {
        IGNORE.with(|f| {
            let _ = p.catch(f);
        });
    }
}
