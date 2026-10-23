//! File reads for OpenKumite's converted assets: from the disk natively, and in a browser from the web server that
//! serves the page. Everything the engine loads goes through here, so the same code runs in both.
//!
//! In a browser the page keeps a cache (`window.__okfs`, web/index.html) the files arrive in without blocking:
//! [`prefetch`] asks for files ahead of their use (a fight's characters and stage while the VS screen shows), and
//! [`read`] takes them from there; a file not asked for ahead comes by a synchronous request (logged, as a file
//! to add to a prefetch). A [`Stream`] reads a file as it arrives (a movie plays while it downloads).

use std::io;
use std::path::{Path, PathBuf};

#[cfg(target_arch = "wasm32")]
pub use imp::src;

/// The whole file.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    if let Ok(mut r) = RECORDED.lock() {
        if let Some(list) = r.as_mut() {
            if !list.contains(&path.to_path_buf()) {
                list.push(path.to_path_buf());
            }
        }
    }
    imp::read(path)
}

/// The files read, in order, while recording (what a browser should fetch ahead: web/make_preload.sh).
static RECORDED: std::sync::Mutex<Option<Vec<PathBuf>>> = std::sync::Mutex::new(None);

/// Start recording the files read.
pub fn record() {
    if let Ok(mut r) = RECORDED.lock() {
        r.get_or_insert_with(Vec::new);
    }
}

/// The files read since [`record`].
pub fn recorded() -> Vec<PathBuf> {
    RECORDED.lock().ok().and_then(|r| r.clone()).unwrap_or_default()
}

/// The file as UTF-8 text.
pub fn read_to_string(path: &Path) -> io::Result<String> {
    String::from_utf8(read(path)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// The file is there.
pub fn exists(path: &Path) -> bool {
    imp::exists(path)
}

/// Start fetching these files in the background (a browser; natively nothing to do).
pub fn prefetch<P: AsRef<Path>>(paths: &[P]) {
    for p in paths {
        imp::prefetch(p.as_ref());
    }
}

/// These files have arrived (or failed: their reads will say so).
pub fn ready<P: AsRef<Path>>(paths: &[P]) -> bool {
    paths.iter().all(|p| imp::ready(p.as_ref()))
}

/// What is still arriving: bytes received and expected (0 when unknown), files.
pub fn progress() -> (u64, u64, usize) {
    imp::progress()
}

/// A file read as it arrives.
pub struct Stream {
    path: PathBuf,
    /// Bytes handed out so far.
    pos: usize,
    done: bool,
    total: Option<usize>,
    /// Bytes per second while arriving.
    rate: f64,
    #[cfg(not(target_arch = "wasm32"))]
    data: Option<Vec<u8>>,
}

impl Stream {
    pub fn open(path: &Path) -> io::Result<Self> {
        imp::stream_open(path)
    }

    /// Append what arrived since the last call to `out`. An error: the file is not there (or broke off).
    pub fn poll(&mut self, out: &mut Vec<u8>) -> io::Result<()> {
        imp::stream_poll(self, out)
    }

    /// All of it has arrived.
    pub fn done(&self) -> bool {
        self.done
    }

    /// Its whole size, when known.
    pub fn total(&self) -> Option<usize> {
        self.total
    }

    /// Bytes per second while arriving (0 before any; infinite natively).
    pub fn rate(&self) -> f64 {
        self.rate
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        imp::stream_close(self);
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::Stream;
    use std::io;
    use std::path::Path;

    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    pub fn exists(path: &Path) -> bool {
        path.exists()
    }

    pub fn prefetch(_: &Path) {}

    pub fn ready(_: &Path) -> bool {
        true
    }

    pub fn progress() -> (u64, u64, usize) {
        (0, 0, 0)
    }

    pub fn stream_open(path: &Path) -> io::Result<Stream> {
        let data = std::fs::read(path)?;
        Ok(Stream { path: path.to_path_buf(), pos: 0, done: true, total: Some(data.len()), rate: f64::INFINITY, data: Some(data) })
    }

    pub fn stream_poll(s: &mut Stream, out: &mut Vec<u8>) -> io::Result<()> {
        if let Some(d) = s.data.take() {
            s.pos = d.len();
            out.extend_from_slice(&d);
        }
        Ok(())
    }

    pub fn stream_close(_: &mut Stream) {}
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::Stream;
    use std::io;
    use std::path::Path;
    use wasm_bindgen::prelude::*;
    use web_sys::XmlHttpRequest;

    #[wasm_bindgen]
    extern "C" {
        // the page's cache (web/index.html); absent on another page: then everything comes synchronously
        #[wasm_bindgen(js_namespace = __okfs, js_name = get, catch)]
        fn js_get(url: &str) -> Result<JsValue, JsValue>;
        #[wasm_bindgen(js_namespace = __okfs, js_name = state, catch)]
        fn js_state(url: &str) -> Result<JsValue, JsValue>;
        #[wasm_bindgen(js_namespace = __okfs, js_name = prefetch, catch)]
        fn js_prefetch(url: &str) -> Result<JsValue, JsValue>;
        #[wasm_bindgen(js_namespace = __okfs, js_name = progress, catch)]
        fn js_progress() -> Result<JsValue, JsValue>;
        #[wasm_bindgen(js_namespace = __okfs, js_name = read, catch)]
        fn js_read(url: &str, from: f64) -> Result<JsValue, JsValue>;
        #[wasm_bindgen(js_namespace = __okfs, js_name = forget, catch)]
        fn js_forget(url: &str) -> Result<JsValue, JsValue>;
        #[wasm_bindgen(js_namespace = __okfs, js_name = put, catch)]
        fn js_put(url: &str, bytes: &js_sys::Uint8Array) -> Result<JsValue, JsValue>;
        // a conversion kept in the browser (OPFS): a file's blob: URL, and whether it is there; absent otherwise
        #[wasm_bindgen(js_namespace = __okfs, js_name = src, catch)]
        fn js_src(url: &str) -> Result<JsValue, JsValue>;
        #[wasm_bindgen(js_namespace = __okfs, js_name = has, catch)]
        fn js_has(url: &str) -> Result<JsValue, JsValue>;
    }

    /// The cache's states.
    const ABSENT: u32 = 0;
    const DONE: u32 = 2;
    const FAILED: u32 = 3;

    fn url(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    /// Where the browser reads the file: the page's blob: URL of it (a conversion kept in the browser), else its URL.
    pub fn src(path: &Path) -> String {
        let u = url(path);
        js_src(&u).ok().and_then(|v| v.as_string()).unwrap_or(u)
    }

    /// Whether the page's conversion has the file (None: no conversion in the browser, the server says).
    fn has(path: &Path) -> Option<bool> {
        js_has(&url(path)).ok().and_then(|v| v.as_bool())
    }

    fn state(path: &Path) -> Option<u32> {
        js_state(&url(path)).ok().and_then(|v| v.as_f64()).map(|v| v as u32)
    }

    fn request(method: &str, path: &Path) -> io::Result<XmlHttpRequest> {
        let err = |e: JsValue| io::Error::other(format!("{}: {e:?}", url(path)));
        let x = XmlHttpRequest::new().map_err(err)?;
        x.open_with_async(method, &src(path), false).map_err(err)?;
        // binary through a synchronous request: each byte arrives as one character
        x.override_mime_type("text/plain; charset=x-user-defined").map_err(err)?;
        x.send().map_err(err)?;
        match x.status().map_err(err)? {
            200 => Ok(x),
            s => Err(io::Error::new(io::ErrorKind::NotFound, format!("{}: HTTP {s}", url(path)))),
        }
    }

    fn not_found(path: &Path) -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, format!("{}: not found", url(path)))
    }

    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        if has(path) == Some(false) {
            return Err(not_found(path));
        }
        match state(path) {
            Some(DONE) => {
                if let Some(a) = js_get(&url(path)).ok().and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok()) {
                    return Ok(a.to_vec());
                }
            }
            Some(FAILED) => return Err(not_found(path)),
            Some(ABSENT) => web_sys::console::warn_1(&format!("kumite_fs: sync {}", url(path)).into()),
            _ => {}
        }
        let x = request("GET", path)?;
        let text = x.response_text().ok().flatten().unwrap_or_default();
        let data: Vec<u8> = text.encode_utf16().map(|c| c as u8).collect();
        // kept: the next read of it is at hand
        let _ = js_put(&url(path), &js_sys::Uint8Array::from(&data[..]));
        Ok(data)
    }

    pub fn exists(path: &Path) -> bool {
        match state(path) {
            Some(DONE) => true,
            Some(FAILED) => false,
            // (a blob: URL answers no HEAD: the page's map says)
            _ => has(path).unwrap_or_else(|| request("HEAD", path).is_ok()),
        }
    }

    pub fn prefetch(path: &Path) {
        let _ = js_prefetch(&url(path));
    }

    pub fn ready(path: &Path) -> bool {
        state(path).is_none_or(|s| s == DONE || s == FAILED)
    }

    pub fn progress() -> (u64, u64, usize) {
        let Ok(v) = js_progress() else { return (0, 0, 0) };
        let a = js_sys::Array::from(&v);
        let n = |k: u32| a.get(k).as_f64().unwrap_or(0.0);
        (n(0) as u64, n(1) as u64, n(2) as usize)
    }

    pub fn stream_open(path: &Path) -> io::Result<Stream> {
        let mut s = Stream { path: path.to_path_buf(), pos: 0, done: false, total: None, rate: 0.0 };
        if js_prefetch(&url(path)).is_err() {
            // no cache on this page: all at once, handed out by the first poll
            let data = read(path)?;
            (s.done, s.total, s.rate) = (true, Some(data.len()), f64::INFINITY);
            STASH.with(|m| m.borrow_mut().insert(url(path), data));
        }
        Ok(s)
    }

    thread_local! {
        /// Streams read whole on a page without the cache.
        static STASH: std::cell::RefCell<std::collections::HashMap<String, Vec<u8>>> = Default::default();
    }

    pub fn stream_poll(s: &mut Stream, out: &mut Vec<u8>) -> io::Result<()> {
        let u = url(&s.path);
        if let Some(d) = STASH.with(|m| m.borrow_mut().remove(&u)) {
            out.extend_from_slice(&d);
            s.pos = d.len();
            return Ok(());
        }
        let v = js_read(&u, s.pos as f64).map_err(|e| io::Error::other(format!("{u}: {e:?}")))?;
        let get = |k: &str| js_sys::Reflect::get(&v, &k.into()).unwrap_or(JsValue::UNDEFINED);
        if get("failed").is_truthy() {
            return Err(not_found(&s.path));
        }
        if let Ok(a) = get("bytes").dyn_into::<js_sys::Uint8Array>() {
            let n = a.length() as usize;
            if n > 0 {
                let at = out.len();
                out.resize(at + n, 0);
                a.copy_to(&mut out[at..]);
                s.pos += n;
            }
        }
        s.done = get("done").is_truthy();
        s.total = get("total").as_f64().filter(|t| *t > 0.0).map(|t| t as usize);
        s.rate = get("rate").as_f64().unwrap_or(0.0);
        Ok(())
    }

    pub fn stream_close(s: &mut Stream) {
        let u = url(&s.path);
        STASH.with(|m| m.borrow_mut().remove(&u));
        let _ = js_forget(&u);
    }
}
