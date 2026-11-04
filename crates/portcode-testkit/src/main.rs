//! `portcode-mock-llm <scenario.json> [port]` — serve a scripted conversation (JSON array of turns).
#[tokio::main]
async fn main() {
    let mut a = std::env::args().skip(1);
    let Some(path) = a.next() else {
        eprintln!("usage: portcode-mock-llm <scenario.json> [port]");
        std::process::exit(2)
    };
    let port: u16 = a.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let turns: Vec<portcode_testkit::Turn> = match std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
    {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{path}: {e}");
            std::process::exit(2)
        }
    };
    match portcode_testkit::start(turns, port).await {
        Ok(m) => {
            println!("{}", m.url);
            std::future::pending::<()>().await
        }
        Err(e) => {
            eprintln!("cannot bind: {e}");
            std::process::exit(1)
        }
    }
}
