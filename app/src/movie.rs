//! Movies (the opening, the prologue, the endings, the Theater): the converter's copy of each STR movie, its
//! frames as Motion-JPEG in an AVI (decoded here, natively and in a browser alike) and its soundtrack as a FLAC
//! the audio player plays. A frame is shown by the time since the start, at the movie's own rate.
//!
//! The AVI is read as it arrives (a browser downloads it while it plays): its chunks are indexed as they come, and
//! the movie starts (its soundtrack with it) once the rest will arrive before it is needed.

use anyhow::{Context, Result};
use kumite_assets::avi::AviIndex;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// A caption drawn into the picture over a span of frames (the US release's burned-in subtitles): the picture
/// file (under pictures/), its place in the movie picture, the first and one past the last frame.
#[derive(Clone, Debug)]
pub struct Caption {
    pub file: String,
    pub at: [f32; 4],
    pub frames: (usize, usize),
}

/// How the game plays a movie (theater.json `movies`): its file, the frames played, the picture size in the
/// 240-line screen, looping, the white clear, its captions.
#[derive(Clone, Debug, Default)]
pub struct Params {
    pub file: usize,
    pub frames: usize,
    pub size: (f32, f32),
    pub looped: bool,
    pub white: bool,
    pub captions: Vec<Caption>,
}

pub struct Movie {
    /// The AVI so far, and the rest arriving.
    data: Vec<u8>,
    stream: Option<kumite_fs::Stream>,
    /// Its chunks indexed so far: each frame's JPEG (offset and length in `data`), the frame rate.
    avi: AviIndex,
    /// Started: the rest will arrive in time (the soundtrack starts with it).
    pub playing: bool,
    /// 60 Hz steps since the start.
    pub step: u32,
    /// The soundtrack (None: silent).
    pub audio: Option<PathBuf>,
    /// The frames played (a Theater entry can play the first part of a movie; None: all).
    limit: Option<usize>,
    /// The frame shown and its texture.
    shown: RefCell<Option<(usize, egui::TextureHandle)>>,
    /// Played over and over until START (theater.json's looping endings).
    pub looped: bool,
    /// Cleared to white around it, not black.
    pub white: bool,
    /// The picture in the 240-line screen (w, h), centred; None: the whole frame.
    size: Option<(f32, f32)>,
    captions: Vec<Caption>,
    pictures: PathBuf,
    caption_tex: RefCell<std::collections::HashMap<String, egui::TextureHandle>>,
}

impl Movie {
    /// Movie `index` (video/NN.avi and NN.flac).
    pub fn load(assets: &Path, index: usize) -> Result<Self> {
        let dir = assets.join("video");
        let stream = kumite_fs::Stream::open(&dir.join(format!("{index:02}.avi"))).with_context(|| format!("movie {index:02}.avi"))?;
        // (.mp3: a conversion made before the converter wrote FLAC)
        let audio = ["flac", "mp3"].iter().map(|e| dir.join(format!("{index:02}.{e}"))).find(|p| kumite_fs::exists(p));
        let mut m = Self {
            data: Vec::new(),
            stream: Some(stream),
            avi: AviIndex::default(),
            playing: false,
            step: 0,
            audio,
            limit: None,
            shown: RefCell::new(None),
            looped: false,
            white: false,
            size: None,
            captions: Vec::new(),
            pictures: assets.join("pictures"),
            caption_tex: RefCell::new(Default::default()),
        };
        m.receive()?;
        Ok(m)
    }

    /// A movie as the game plays it (its first `frames` frames, looped or not, its captions).
    pub fn with(assets: &Path, p: &Params) -> Result<Self> {
        let mut m = Self::load(assets, p.file)?;
        m.limit = (p.frames > 0).then_some(p.frames);
        m.looped = p.looped;
        m.white = p.white;
        m.size = (p.size.0 > 0.0 && p.size.1 > 0.0).then_some(p.size);
        m.captions = p.captions.clone();
        Ok(m)
    }

    /// Take what arrived and index it.
    fn receive(&mut self) -> Result<()> {
        if let Some(st) = self.stream.as_mut() {
            st.poll(&mut self.data)?;
            if st.done() {
                self.stream = None;
            }
        }
        self.avi.advance(&self.data)
    }

    fn fps(&self) -> f64 {
        1e6 / self.avi.micros.max(1) as f64
    }

    /// One past the last frame played (as far as known while it arrives).
    fn end(&self) -> usize {
        let all = if self.stream.is_none() { self.avi.frames.len() } else { self.avi.total.unwrap_or(usize::MAX) };
        self.limit.map_or(all, |l| l.min(all))
    }

    /// The frame to show now.
    pub fn current(&self) -> usize {
        (self.step as f64 * self.fps() / 60.0) as usize
    }

    /// The rest arrives before it is needed (or all is here): seconds still to download against seconds to play,
    /// with a second to spare.
    fn ready(&self) -> bool {
        let Some(st) = self.stream.as_ref() else { return true };
        let (Some(total), Some(frames)) = (st.total(), self.avi.total) else { return false };
        if st.rate() <= 0.0 || self.avi.frames.is_empty() {
            return false;
        }
        let left = total.saturating_sub(self.data.len()) as f64 / st.rate();
        let play = self.limit.map_or(frames, |l| l.min(frames)) as f64 / self.fps();
        left + 1.0 < play
    }

    /// The download as a fraction (for a waiting bar), None when it plays.
    pub fn loading(&self) -> Option<f32> {
        if self.playing {
            return None;
        }
        let st = self.stream.as_ref()?;
        Some(st.total().map_or(0.0, |t| self.data.len() as f32 / t.max(1) as f32))
    }

    /// One 60 Hz step; true once the last frame has been shown. Waits (not started) until it can play through.
    pub fn tick(&mut self) -> bool {
        if let Err(e) = self.receive() {
            log::warn!("movie: {e:#}");
            return true;
        }
        if !self.playing {
            if !self.ready() {
                return false;
            }
            self.playing = true;
        }
        // a frame not arrived yet: the picture holds
        if self.current() + 1 < self.end() && self.current() + 1 >= self.avi.frames.len() && self.stream.is_some() {
            return false;
        }
        self.step += 1;
        let done = self.current() >= self.end();
        if done && self.looped {
            self.step = 0;
            return false;
        }
        done
    }

    /// Draw the current frame over `rect` (the 4:3 picture), black around it.
    pub fn draw(&self, ctx: &egui::Context, painter: &egui::Painter, rect: egui::Rect) {
        let clear = if self.white { egui::Color32::WHITE } else { egui::Color32::BLACK };
        painter.rect_filled(ctx.content_rect(), 0.0, clear);
        // still arriving: a thin bar of how much
        if let Some(f) = self.loading() {
            let w = rect.width() * 0.4;
            let bar = egui::Rect::from_min_size(rect.center_bottom() + egui::vec2(-w / 2.0, -rect.height() * 0.08), egui::vec2(w, rect.height() * 0.008));
            painter.rect_filled(bar, 0.0, egui::Color32::from_gray(48));
            painter.rect_filled(egui::Rect::from_min_size(bar.min, egui::vec2(w * f.clamp(0.0, 1.0), bar.height())), 0.0, egui::Color32::from_gray(200));
            return;
        }
        // the picture centred in the 240-line screen (letterboxed); the screen is 256 wide up to 256, else 320
        let picture = match self.size {
            Some((w, h)) => {
                let screen_w = if w <= 256.0 { 256.0 } else { 320.0 };
                let (sx, sy) = (rect.width() / screen_w, rect.height() / 240.0);
                let min = rect.min + egui::vec2((screen_w - w) / 2.0 * sx, (240.0 - h) / 2.0 * sy);
                painter.rect_filled(rect, 0.0, egui::Color32::BLACK);
                Some((egui::Rect::from_min_size(min, egui::vec2(w * sx, h * sy)), sx, sy))
            }
            None => None,
        };
        let rect = picture.map_or(rect, |p| p.0);
        let k = self.current().min(self.avi.frames.len().saturating_sub(1)).min(self.end().saturating_sub(1));
        if self.avi.frames.is_empty() {
            return;
        }
        let mut shown = self.shown.borrow_mut();
        if shown.as_ref().is_none_or(|(i, _)| *i != k) {
            match self.decode(k) {
                Ok(img) => match shown.as_mut() {
                    Some((i, tex)) => {
                        tex.set(img, egui::TextureOptions::LINEAR);
                        *i = k;
                    }
                    None => *shown = Some((k, ctx.load_texture("movie", img, egui::TextureOptions::LINEAR))),
                },
                Err(e) => log::warn!("movie frame {k}: {e:#}"),
            }
        }
        if let Some((_, tex)) = shown.as_ref() {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(tex.id(), rect, uv, egui::Color32::WHITE);
        }
        // the captions, inside the picture only (some lie below a short picture's lines and never show)
        let Some((_, sx, sy)) = picture else { return };
        let frame = self.current();
        let clip = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
        for c in self.captions.iter().filter(|c| (c.frames.0..c.frames.1).contains(&frame)) {
            let mut cache = self.caption_tex.borrow_mut();
            if !cache.contains_key(&c.file) {
                match crate::art::load_png(&self.pictures.join(&c.file)) {
                    Ok(img) => {
                        cache.insert(c.file.clone(), ctx.load_texture(c.file.as_str(), img, egui::TextureOptions::NEAREST));
                    }
                    Err(e) => {
                        log::warn!("caption {}: {e:#}", c.file);
                        continue;
                    }
                }
            }
            let [x, y, w, h] = c.at;
            let r = egui::Rect::from_min_size(rect.min + egui::vec2(x * sx, y * sy), egui::vec2(w * sx, h * sy));
            let Some(tex) = cache.get(&c.file) else { continue };
            clip.image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
    }

    fn decode(&self, k: usize) -> Result<egui::ColorImage> {
        use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
        let &(at, len) = self.avi.frames.get(k).context("no such frame")?;
        // (a frame bigger than any movie's is a bad file, not memory to ask for)
        let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA).set_max_width(2048).set_max_height(2048);
        let jpeg = at.checked_add(len).and_then(|end| self.data.get(at..end)).context("a frame past the file")?;
        let mut d = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(jpeg), options);
        let pixels = d.decode().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let (w, h) = d.dimensions().context("no size")?;
        Ok(egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every converted movie loads as the game loads it, with its frames, rate and soundtrack, each frame decoding.
    #[test]
    fn every_converted_movie_decodes() {
        let a = std::env::var_os("OPENKUMITE_ASSETS").map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets"), PathBuf::from);
        let Ok(d) = kumite_fs::read(&a.join("audio/index.json")) else { return };
        let index: serde_json::Value = serde_json::from_slice(&d).unwrap();
        for (k, v) in index["video"].as_array().into_iter().flatten().enumerate() {
            let m = Movie::load(&a, k).unwrap();
            assert_eq!(m.avi.frames.len() as u64, v["frames"].as_u64().unwrap(), "movie {k} frames");
            assert_eq!(m.fps().round() as u64, v["fps"].as_u64().unwrap(), "movie {k} fps");
            assert!(m.audio.is_some(), "movie {k} has its soundtrack");
            for f in 0..m.avi.frames.len() {
                m.decode(f).unwrap_or_else(|e| panic!("movie {k} frame {f}: {e}"));
            }
        }
    }
}
