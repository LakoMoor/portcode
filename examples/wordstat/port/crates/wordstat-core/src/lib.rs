//! Headless core of the wordstat port. Behaviour measured against the C original (see docs/PORT.md, E-ids).
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

use std::collections::BTreeMap;

/// Everything the program would print/exit with; the binary only forwards it.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

/// C `atoi`: optional whitespace and sign, then digits; stops at the first non-digit; 0 if none. Saturates instead of UB.
pub fn atoi(s: &str) -> i32 {
    let t = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (neg, digits) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let mut n: i64 = 0;
    for b in digits.bytes().take_while(u8::is_ascii_digit) {
        n = (n * 10 + i64::from(b - b'0')).min(i64::from(i32::MAX) + 1);
    }
    let n = if neg { -n } else { n };
    n.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// Words = maximal runs of ASCII letters/digits, lower-cased; the original keeps at most 255 bytes of one run.
pub fn count(data: &[u8], min_len: i32) -> (BTreeMap<String, u32>, u64, u64) {
    let (mut map, mut total, mut chars) = (BTreeMap::new(), 0u64, 0u64);
    let mut word = String::new();
    let mut len = 0usize;
    // iterate one past the end so the final word is flushed, like the original's EOF sentinel
    for b in data.iter().map(|b| Some(*b)).chain(std::iter::once(None)) {
        if let Some(b) = b.filter(u8::is_ascii_alphanumeric) {
            if len < 255 {
                word.push(b.to_ascii_lowercase() as char);
                len += 1;
            }
            continue;
        }
        if len > 0 && len as i64 >= i64::from(min_len) {
            *map.entry(std::mem::take(&mut word)).or_insert(0) += 1;
            total += 1;
            chars += len as u64;
        }
        word.clear();
        len = 0;
    }
    (map, total, chars)
}

/// Run with `args` (without argv[0]); `read` returns the file bytes or `None` if it cannot be opened.
pub fn run(args: &[String], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Outcome {
    let (mut top, mut min_len, mut path): (i32, i32, Option<&str>) = (10, 1, None);
    let mut i = 0;
    while let Some(a) = args.get(i) {
        match (a.as_str(), args.get(i + 1)) {
            ("--top", Some(v)) => {
                top = atoi(v);
                i += 1;
            }
            ("--min-len", Some(v)) => {
                min_len = atoi(v);
                i += 1;
            }
            _ => path = Some(a.as_str()),
        }
        i += 1;
    }
    let Some(path) = path else {
        return Outcome {
            stdout: String::new(),
            stderr: "usage: wordstat [--top N] [--min-len L] FILE\n".into(),
            code: 2,
        };
    };
    let Some(data) = read(path) else {
        return Outcome {
            stdout: String::new(),
            stderr: format!("wordstat: cannot open {path}\n"),
            code: 1,
        };
    };
    let (map, total, chars) = count(&data, min_len);
    let mut entries: Vec<(&String, &u32)> = map.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let shown = i64::from(top).min(entries.len() as i64);
    let avg = if total > 0 {
        chars as f64 / total as f64
    } else {
        0.0
    };
    let mut out = format!(
        "file: {path}\nwords: {total}\nunique: {}\navg word length: {avg:.3}\ntop {shown}:\n",
        map.len()
    );
    for (w, n) in entries.iter().take(shown.max(0) as usize) {
        out.push_str(&format!("  {w:<12} {n:>5}\n"));
    }
    Outcome {
        stdout: out,
        stderr: String::new(),
        code: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_str(args: &[&str], text: &str) -> Outcome {
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let t = text.as_bytes().to_vec();
        run(&a, &move |_| Some(t.clone()))
    }

    #[test]
    fn atoi_matches_c_semantics() {
        assert_eq!(
            (
                atoi("12abc"),
                atoi("  -3"),
                atoi("x"),
                atoi("+7"),
                atoi("99999999999")
            ),
            (12, -3, 0, 7, i32::MAX)
        );
    }
    #[test]
    fn counts_and_sorts_ties_alphabetically() {
        let o = run_str(&["f"], "b a b A c");
        assert!(o.stdout.contains("words: 5\nunique: 3\n"));
        let lines: Vec<&str> = o.stdout.lines().skip(5).collect();
        assert_eq!(
            lines,
            vec![
                "  a                2",
                "  b                2",
                "  c                1"
            ]
        );
    }
    #[test]
    fn quirks_of_the_original_are_kept() {
        // negative --top prints "top -1:" and no rows; value-less flag becomes the path
        assert!(run_str(&["--top", "-1", "f"], "a")
            .stdout
            .ends_with("top -1:\n"));
        assert_eq!(
            run_str(&["--top"], "a").stdout.lines().next(),
            Some("file: --top")
        );
        // a 300-letter word is counted once, as 255 bytes
        let o = run_str(&["f"], &"x".repeat(300));
        assert!(o.stdout.contains("avg word length: 255.000"));
        assert_eq!(run_str(&[], "").code, 2);
    }
    #[test]
    fn unopenable_file_exit_1() {
        let o = run(&["nope".to_string()], &|_| None);
        assert_eq!(
            (o.code, o.stderr.as_str()),
            (1, "wordstat: cannot open nope\n")
        );
    }
}
