use std::io::{self, Read, Write};

struct Scanner<I: Iterator<Item = u8>> {
    bytes: std::iter::Peekable<I>,
}
impl<I: Iterator<Item = u8>> Scanner<I> {
    fn new(bytes: I) -> Self {
        Self {
            bytes: bytes.peekable(),
        }
    }
    fn whitespace(&mut self) {
        while self
            .bytes
            .peek()
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12))
        {
            self.bytes.next();
        }
    }
    fn token(&mut self, width: usize) -> Option<Vec<u8>> {
        self.whitespace();
        let mut token = Vec::new();
        while token.len() < width {
            match self.bytes.peek() {
                None | Some(b' ' | b'\t' | b'\n' | b'\r' | 11 | 12) => break,
                Some(b) => {
                    token.push(*b);
                    self.bytes.next();
                }
            }
        }
        (!token.is_empty()).then_some(token)
    }
    // src: original/main.c:12; scanf accepts signed numeric prefixes.
    fn number(&mut self) -> Option<u32> {
        self.whitespace();
        let negative = self.bytes.peek() == Some(&b'-');
        if negative || self.bytes.peek() == Some(&b'+') {
            self.bytes.next();
        }
        let mut digits = 0;
        let mut n = 0u32;
        while let Some(b'0'..=b'9') = self.bytes.peek() {
            let digit = self.bytes.next()? - b'0';
            n = n.wrapping_mul(10).wrapping_add(digit as u32);
            digits += 1;
        }
        (digits > 0).then(|| if negative { n.wrapping_neg() } else { n })
    }
}

#[derive(Default)]
struct State {
    t: i32,
    x: i32,
    v: i32,
    rng: u32,
}
impl State {
    fn line(&self) -> String {
        format!("{} {} {} {}\n", self.t, self.x, self.v, self.rng)
    }
    fn tick(&mut self) {
        // src: original/main.c:10; uint32 arithmetic wraps even in debug builds.
        self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        self.x = self
            .x
            .wrapping_add(self.v)
            .wrapping_add((self.rng % 3) as i32 - 1);
        self.t = self.t.wrapping_add(1);
    }
}

fn run(input: impl Iterator<Item = u8>, out: &mut impl Write) -> i32 {
    let mut scan = Scanner::new(input);
    let mut state = State {
        rng: 12345,
        ..State::default()
    };
    while let Some(cmd) = scan.token(31) {
        // strcmp and fopen see only the bytes before an embedded NUL.
        let cmd = cmd.split(|b| *b == 0).next().unwrap_or_default();
        match cmd {
            b"left" => state.v = -2,
            b"right" => state.v = 2,
            b"stop" => state.v = 0,
            b"tick" => state.tick(),
            b"state" => {}
            b"save" | b"load" => {
                let Some(path) = scan.token(255) else {
                    return 2;
                };
                let path = path.split(|b| *b == 0).next().unwrap_or_default();
                #[cfg(unix)]
                let path = {
                    use std::os::unix::ffi::OsStrExt;
                    std::path::Path::new(std::ffi::OsStr::from_bytes(path))
                };
                #[cfg(not(unix))]
                let path = std::path::Path::new(std::str::from_utf8(path).unwrap_or(""));
                if cmd == b"save" {
                    let Ok(mut file) = std::fs::File::create(path) else {
                        return 3;
                    };
                    // Like C, only opening failure affects the exit status.
                    let _ = file.write_all(state.line().as_bytes());
                } else {
                    let Ok(mut file) = std::fs::File::open(path) else {
                        return 3;
                    };
                    let mut bytes = Vec::new();
                    let _ = file.read_to_end(&mut bytes);
                    let mut save = Scanner::new(bytes.into_iter());
                    let Some(t) = save.number() else {
                        return 4;
                    };
                    let Some(x) = save.number() else {
                        return 4;
                    };
                    let Some(v) = save.number() else {
                        return 4;
                    };
                    let Some(rng) = save.number() else {
                        return 4;
                    };
                    state = State {
                        t: t as i32,
                        x: x as i32,
                        v: v as i32,
                        rng,
                    };
                }
            }
            _ => return 5,
        }
        let _ = out.write_all(state.line().as_bytes());
    }
    0
}

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let code = run(
        io::BufReader::new(stdin.lock())
            .bytes()
            .map_while(Result::ok),
        &mut stdout,
    );
    let _ = stdout.flush();
    std::process::exit(code);
}
