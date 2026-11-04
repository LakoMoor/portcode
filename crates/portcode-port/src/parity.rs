use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct Report {
    pub ok: u32,
    pub diff: Vec<String>,
    pub missing: Vec<String>,
    pub extra: Vec<String>,
    pub empty: bool,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.diff.is_empty() && self.missing.is_empty() && self.extra.is_empty() && !self.empty
    }
    pub fn render(&self) -> String {
        let mut o = String::new();
        for d in &self.diff {
            o += &format!("DIFF    {d}\n");
        }
        for m in &self.missing {
            o += &format!("MISSING {m}\n");
        }
        for e in &self.extra {
            o += &format!("EXTRA   {e}: unexpected port output\n");
        }
        if self.empty {
            o += "EMPTY golden directory: no tests were executed\n";
        }
        o + &format!(
            "parity: ok={} diff={} missing={} extra={}\n",
            self.ok,
            self.diff.len(),
            self.missing.len(),
            self.extra.len()
        )
    }
}

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p)
            } else {
                out.push(p)
            }
        }
    }
    out.sort();
    out
}

fn close(a: f64, b: f64, at: f64, rt: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan()) || (a - b).abs() <= at + rt * a.abs().max(b.abs())
}

/// Token-wise text comparison; numbers may differ within tolerance, everything else must match exactly.
pub fn cmp_text(a: &str, b: &str, at: f64, rt: f64) -> Option<String> {
    let split = |s: &str| -> Vec<String> {
        let mut t = Vec::new();
        let mut cur = String::new();
        for c in s.chars() {
            if c.is_whitespace() || ",;:=()[]{}\"".contains(c) {
                if !cur.is_empty() {
                    t.push(std::mem::take(&mut cur));
                }
                t.push(c.to_string());
            } else {
                cur.push(c);
            }
        }
        if !cur.is_empty() {
            t.push(cur);
        }
        t
    };
    let (ta, tb) = (split(a), split(b));
    if ta.len() != tb.len() {
        return Some(format!("token count {} vs {}", ta.len(), tb.len()));
    }
    for (i, (x, y)) in ta.iter().zip(tb.iter()).enumerate() {
        if x == y {
            continue;
        }
        if let (Ok(fx), Ok(fy)) = (x.parse::<f64>(), y.parse::<f64>()) {
            if close(fx, fy, at, rt) {
                continue;
            }
        }
        return Some(format!("token {i}: {x:?} vs {y:?}"));
    }
    None
}

fn cmp_png(a: &Path, b: &Path, max_channel: u8) -> Option<String> {
    let dec = |p: &Path| -> Option<(png::OutputInfo, Vec<u8>)> {
        let mut r = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).ok()?))
            .read_info()
            .ok()?;
        let mut buf = vec![0; r.output_buffer_size()?];
        let info = r.next_frame(&mut buf).ok()?;
        buf.truncate(info.buffer_size());
        Some((info, buf))
    };
    let (Some((ia, da)), Some((ib, db))) = (dec(a), dec(b)) else {
        return (std::fs::read(a).ok() != std::fs::read(b).ok())
            .then(|| "png undecodable and bytes differ".into());
    };
    if (ia.width, ia.height, ia.color_type) != (ib.width, ib.height, ib.color_type) {
        return Some(format!(
            "image {}x{} {:?} vs {}x{} {:?}",
            ia.width, ia.height, ia.color_type, ib.width, ib.height, ib.color_type
        ));
    }
    let m = da
        .iter()
        .zip(db.iter())
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0);
    (m > max_channel).then(|| format!("max channel diff {m} (allowed {max_channel})"))
}

/// Compare every file under `golden` with the same relative path under `port`.
pub fn compare(golden: &Path, port: &Path, at: f64, rt: f64) -> Result<Report, String> {
    if !golden.is_dir() || !port.is_dir() {
        return Err("both golden and port directories must exist".into());
    }
    let mut r = Report::default();
    let _ = Regex::new("");
    let gf = files(golden);
    r.empty = gf.is_empty();
    for fa in &gf {
        let rel = fa.strip_prefix(golden).unwrap_or(fa);
        let fb = port.join(rel);
        let rels = rel.display().to_string();
        if !fb.exists() {
            r.missing.push(rels);
            continue;
        }
        let is_png = rels.to_lowercase().ends_with(".png");
        let err = if is_png {
            cmp_png(fa, &fb, 2)
        } else {
            match (std::fs::read_to_string(fa), std::fs::read_to_string(&fb)) {
                (Ok(x), Ok(y)) => cmp_text(&x, &y, at, rt),
                _ => (std::fs::read(fa).ok() != std::fs::read(&fb).ok())
                    .then(|| "binary differs".into()),
            }
        };
        match err {
            Some(e) => r.diff.push(format!("{rels}: {e}")),
            None => r.ok += 1,
        }
    }
    for fb in files(port) {
        let rel = fb.strip_prefix(port).unwrap_or(&fb);
        if !golden.join(rel).is_file() {
            r.extra.push(rel.display().to_string());
        }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tolerance_applies_to_numbers_only() {
        assert!(cmp_text("x=1.000001 name", "x=1.000002 name", 1e-5, 1e-4).is_none());
        assert!(cmp_text("x=1.0", "x=1.1", 1e-5, 1e-4).is_some());
        assert!(cmp_text("a b", "a c", 1e-5, 1e-4).is_some());
    }
    #[test]
    fn missing_extra_and_empty_fail() {
        let g = tempfile::tempdir().unwrap();
        let p = tempfile::tempdir().unwrap();
        assert!(
            !compare(g.path(), p.path(), 1e-5, 1e-4).unwrap().passed(),
            "empty golden must not pass"
        );
        std::fs::write(g.path().join("a.txt"), "1").unwrap();
        std::fs::write(p.path().join("b.txt"), "1").unwrap();
        let r = compare(g.path(), p.path(), 1e-5, 1e-4).unwrap();
        assert_eq!((r.missing.len(), r.extra.len()), (1, 1));
        std::fs::write(p.path().join("a.txt"), "1").unwrap();
        std::fs::remove_file(p.path().join("b.txt")).unwrap();
        assert!(compare(g.path(), p.path(), 1e-5, 1e-4).unwrap().passed());
    }
}
