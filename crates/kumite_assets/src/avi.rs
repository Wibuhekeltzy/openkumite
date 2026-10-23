//! AVI files (the movies): their frames' chunks, indexed as the file arrives.

use anyhow::{bail, Result};

/// An AVI's chunks as they arrive (RIFF 'AVI '): the microseconds per frame and the frame count from 'avih',
/// each '##dc' / '##db' chunk of the 'movi' list in order.
#[derive(Default)]
pub struct AviIndex {
    /// The lists being walked: (next chunk, end), innermost last; None before the header.
    stack: Option<Vec<(usize, usize)>>,
    pub micros: u32,
    /// The frame count the header gives.
    pub total: Option<usize>,
    /// Each frame's chunk: its body's offset and size.
    pub frames: Vec<(usize, usize)>,
}

impl AviIndex {
    /// Index the chunks `d` (the file so far) now holds whole.
    pub fn advance(&mut self, d: &[u8]) -> Result<()> {
        let u32_at = |o: usize| d.get(o..o.checked_add(4)?).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        if self.stack.is_none() {
            if d.len() < 12 {
                return Ok(());
            }
            if d.get(0..4) != Some(b"RIFF") || d.get(8..12) != Some(b"AVI ") {
                bail!("not an AVI");
            }
            self.micros = 50_000;
            self.stack = Some(vec![(12, (u32_at(4).unwrap_or(0) as usize).saturating_add(8))]);
        }
        let Some(stack) = self.stack.as_mut() else { return Ok(()) };
        while let Some(&(at, end)) = stack.last() {
            // (sizes come from the file: saturating, so a huge one ends its list instead of wrapping on 32 bits)
            let body = at.saturating_add(8);
            if body > end {
                stack.pop();
                continue;
            }
            let (Some(id), Some(size)) = (at.checked_add(4).and_then(|e| d.get(at..e)), at.checked_add(4).and_then(u32_at)) else { break };
            let size = size as usize;
            let next = body.saturating_add(size).saturating_add(size & 1);
            if id == b"LIST" {
                // the list's own chunks first, then the rest of this level
                if let Some(top) = stack.last_mut() {
                    top.0 = next;
                }
                stack.push((body.saturating_add(4), next.min(end)));
                continue;
            }
            if body.saturating_add(size) > d.len() {
                break;
            }
            if id == b"avih" {
                self.micros = u32_at(body).unwrap_or(self.micros);
                self.total = u32_at(body + 16).map(|n| n as usize).filter(|&n| n > 0);
            } else if matches!(id, [_, _, b'd', b'c' | b'b']) && size > 0 {
                self.frames.push((body, size));
            }
            if let Some(top) = stack.last_mut() {
                top.0 = next;
            }
        }
        Ok(())
    }
}

/// The frames of a whole AVI and its microseconds per frame.
pub fn parse_avi(d: &[u8]) -> Result<(Vec<(usize, usize)>, u32)> {
    let mut a = AviIndex::default();
    a.advance(d)?;
    Ok((a.frames, a.micros))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The index built a few bytes at a time (a download) is the whole file's.
    #[test]
    fn avi_frames_as_they_arrive() {
        let path = std::env::var_os("OPENKUMITE_ASSETS").map_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"), std::path::PathBuf::from).join("video/20.avi");
        let Ok(d) = std::fs::read(&path) else { return };
        let whole = parse_avi(&d).unwrap();
        let mut a = AviIndex::default();
        for n in (0..d.len()).step_by(77_777).chain([d.len()]) {
            a.advance(&d[..n]).unwrap();
        }
        assert_eq!((a.frames, a.micros), whole);
        assert!(a.total.is_some_and(|t| t >= whole.0.len()));
    }

    #[test]
    fn avi_frames() {
        // RIFF AVI with hdrl/avih (40000 us) and movi with two 00dc chunks (odd sizes padded)
        let chunk = |id: &[u8], body: &[u8]| {
            let mut c = id.to_vec();
            c.extend((body.len() as u32).to_le_bytes());
            c.extend(body);
            if body.len() & 1 == 1 {
                c.push(0);
            }
            c
        };
        let list = |kind: &[u8], items: Vec<u8>| {
            let mut b = kind.to_vec();
            b.extend(items);
            chunk(b"LIST", &b)
        };
        let mut avih = 40_000u32.to_le_bytes().to_vec();
        avih.resize(56, 0);
        let hdrl = list(b"hdrl", chunk(b"avih", &avih));
        let movi = list(b"movi", [chunk(b"00dc", b"abc"), chunk(b"01wb", b"zz"), chunk(b"00dc", b"defg")].concat());
        let mut body = b"AVI ".to_vec();
        body.extend(hdrl);
        body.extend(movi);
        let avi = chunk(b"RIFF", &body);
        let (frames, micros) = parse_avi(&avi).unwrap();
        assert_eq!(micros, 40_000);
        assert_eq!(frames.len(), 2);
        assert_eq!(&avi[frames[0].0..frames[0].0 + frames[0].1], b"abc");
        assert_eq!(&avi[frames[1].0..frames[1].0 + frames[1].1], b"defg");
    }
}
