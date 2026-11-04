//! Thin shell around `wordstat_core`: real file I/O and process exit only.
use std::io::{Read, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // like the original, a directory opens fine and reads as empty; only a failed open is an error
    let read = |p: &str| {
        std::fs::File::open(p).ok().map(|mut f| {
            let mut buf = Vec::new();
            let _ = f.read_to_end(&mut buf);
            buf
        })
    };
    let o = wordstat_core::run(&args, &read);
    let _ = std::io::stdout().write_all(o.stdout.as_bytes());
    let _ = std::io::stderr().write_all(o.stderr.as_bytes());
    std::process::exit(o.code);
}
