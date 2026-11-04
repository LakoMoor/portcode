use crate::term::*;
use portcode_agent::tool::Ctx;
use portcode_agent::{AutoGate, Config, Reply, Rules, Runner, Status};
use portcode_llm::{profile_for, Client, Message, ToolDef};
use std::io::{BufRead, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub enum SkillOp {
    List,
    Show(String, Option<String>),
    Export(String),
}

pub fn watch_ctrl_c(flag: Arc<AtomicBool>) {
    tokio::spawn(async move {
        loop {
            if tokio::signal::ctrl_c().await.is_err() {
                break;
            }
            if flag.swap(true, Ordering::Relaxed) {
                std::process::exit(130);
            } // second Ctrl-C quits
            eprintln!("\n{}", yellow("! cancelling… (Ctrl-C again to quit)"));
        }
    });
}

fn banner(r: &Runner) {
    println!(
        "{} {}  {}",
        accent("◆ PortCode"),
        dim(env!("CARGO_PKG_VERSION")),
        dim(&format!(
            "{}/{} · {} · {}",
            r.resolved.provider.name, r.resolved.model, r.agent.name, r.resolved.provider.base_url
        ))
    );
    println!(
        "{}",
        dim("  /help for commands · Ctrl-C cancels a turn · /exit quits")
    );
}

pub async fn chat(
    cfg: Config,
    model: &Option<String>,
    agent: &Option<String>,
    show_reasoning: bool,
    cont: bool,
) -> Result<i32, String> {
    let mut r = Runner::new(
        crate::root()?,
        cfg,
        model.as_deref(),
        agent.as_deref().unwrap_or("build"),
        Arc::new(TermGate { auto: false }),
        Arc::new(Term::new(show_reasoning)),
    )
    .await?;
    watch_ctrl_c(r.cancel_flag());
    if cont {
        if let Some(id) = r.latest_session() {
            let n = r.resume(&id)?;
            println!("{}", dim(&format!("resumed {id} ({n} messages)")));
        }
    }
    banner(&r);
    let stdin = std::io::stdin();
    loop {
        print!("\n{} ", accent(&format!("{}❯", r.agent.name)));
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        let mut input = String::new();
        loop {
            // trailing backslash continues the line
            line.clear();
            let n = tokio::task::spawn_blocking(|| {
                let mut s = String::new();
                std::io::stdin().lock().read_line(&mut s).map(|n| (n, s))
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
            if n.0 == 0 {
                println!();
                return Ok(0);
            }
            let t = n.1.trim_end_matches(['\n', '\r']).to_string();
            if let Some(h) = t.strip_suffix('\\') {
                input += h;
                input.push('\n');
                print!("{} ", dim("…"));
                let _ = std::io::stdout().flush();
            } else {
                input += &t;
                break;
            }
        }
        let _ = &stdin;
        let text = input.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(cmd) = text.strip_prefix('/') {
            let mut p = cmd.split_whitespace();
            match (p.next().unwrap_or(""), p.next()) {
                ("exit" | "quit" | "q", _) => return Ok(0),
                ("help" | "?", _) => println!("{}", dim("/agent <build|port|reverse|plan>  /model <provider/model>  /models  /tools  /todos  /ledger  /clear  /resume  /exit")),
                ("agent", Some(a)) => match r.set_agent(a) { Ok(()) => println!("{}", dim(&format!("agent → {a}"))), Err(e) => println!("{} {e}", red("!")) },
                ("agent", None) => for a in portcode_agent::agents::all().iter().filter(|a| a.primary) { println!("  {:<8} {}", bold(a.name), a.description); },
                ("model", Some(m)) => match r.set_model(m) { Ok(()) => println!("{}", dim(&format!("model → {}/{}", r.resolved.provider.name, r.resolved.model))), Err(e) => println!("{} {e}", red("!")) },
                ("model", None) => println!("{}/{}  profile: {:?}", r.resolved.provider.name, r.resolved.model, r.resolved.profile),
                ("models", _) => { let _ = models(r.cfg.clone(), Some(r.resolved.provider.name.clone())).await; }
                ("tools", _) => println!("{}", r.tool_names().join(", ")),
                ("todos", _) => for t in r.todos() { println!("  [{}] {}", t.status, t.content); },
                ("ledger", _) => { let _ = run_tool(r.cfg.clone(), "evidence_list", "{}").await; }
                ("clear", _) => { r.clear(); println!("{}", dim("context cleared")); }
                ("resume", _) => match r.latest_session() { Some(id) => match r.resume(&id) { Ok(n) => println!("{}", dim(&format!("resumed {id} ({n} messages)"))), Err(e) => println!("{} {e}", red("!")) }, None => println!("{}", dim("no saved sessions")) },
                (other, _) => println!("{} unknown command /{other}. Try /help", yellow("!")),
            }
            continue;
        }
        match r.send(text).await {
            Ok(o) => {
                println!(
                    "\n{}",
                    dim(&format!(
                        "— {} step(s) · {} in / {} out tokens{}",
                        o.steps,
                        o.usage.prompt_tokens,
                        o.usage.completion_tokens,
                        match o.status {
                            Status::Done => "",
                            Status::MaxSteps => " · step limit reached",
                            Status::Cancelled => " · cancelled",
                            Status::Stuck => " · stopped (repeating)",
                        }
                    ))
                );
            }
            Err(e) => println!("\n{} {e}", red("error:")),
        }
    }
}

pub async fn models(cfg: Config, which: Option<String>) -> Result<i32, String> {
    let reference = which.map(|w| if w.contains('/') { w } else { format!("{w}/x") });
    let r = cfg.resolve(reference.as_deref())?;
    let c = Client::new(r.provider.clone()).map_err(|e| e.to_string())?;
    let ids = c.list_models().await.map_err(|e| e.to_string())?;
    println!("{} {}", bold(&r.provider.name), dim(&r.provider.base_url));
    for id in &ids {
        let p = profile_for(id);
        println!(
            "  {id:<48} {}",
            dim(&format!("{} ctx {}", p.family, p.context_window))
        );
    }
    if ids.is_empty() {
        println!("  (no models reported)");
    }
    Ok(0)
}

pub fn providers(cfg: Config) -> Result<i32, String> {
    let mut seen = std::collections::BTreeSet::new();
    for p in portcode_llm::presets()
        .into_iter()
        .map(|p| p.name)
        .chain(cfg.provider.keys().cloned())
    {
        if !seen.insert(p.clone()) {
            continue;
        }
        let r = cfg.resolve(Some(&format!("{p}/x")));
        match r {
            Ok(r) => {
                let key = r.provider.api_key.is_some()
                    || r.provider
                        .api_key_env
                        .as_deref()
                        .map(|e| std::env::var(e).is_ok())
                        .unwrap_or(false);
                println!(
                    "{:<11} {:<36} {}",
                    bold(&p),
                    r.provider.base_url,
                    if key {
                        green("key set")
                    } else if r.provider.api_key_env.is_some() {
                        yellow(&format!(
                            "no key (${})",
                            r.provider.api_key_env.unwrap_or_default()
                        ))
                    } else {
                        dim("no key needed").to_string()
                    }
                );
            }
            Err(e) => println!("{p:<11} {}", red(&e)),
        }
    }
    Ok(0)
}

pub async fn doctor(cfg: Config) -> Result<i32, String> {
    let ok = |b: bool, s: &str, hint: &str| {
        println!(
            "  {} {s}{}",
            if b { green("✓") } else { red("✗") },
            if b || hint.is_empty() {
                String::new()
            } else {
                format!("  {}", dim(&format!("→ {hint}")))
            }
        )
    };
    println!("{}", bold("Providers"));
    let names: Vec<String> = portcode_llm::presets()
        .into_iter()
        .map(|p| p.name)
        .chain(cfg.provider.keys().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let checks = names.iter().map(|n| {
        let cfg = cfg.clone();
        let n = n.clone();
        async move {
            let r = cfg.resolve(Some(&format!("{n}/x"))).ok();
            let res = match r {
                Some(r) => match Client::new(r.provider.clone()) {
                    Ok(c) => {
                        tokio::time::timeout(std::time::Duration::from_secs(4), c.list_models())
                            .await
                            .map_err(|_| "timeout".to_string())
                            .and_then(|x| x.map_err(|e| e.to_string()))
                            .map(|m| (r.provider.base_url, m.len()))
                    }
                    Err(e) => Err(e.to_string()),
                },
                None => Err("not configured".into()),
            };
            (n, res)
        }
    });
    for (n, res) in futures_util::future::join_all(checks).await {
        match res {
            Ok((u, c)) => ok(true, &format!("{n:<11} {u}  {} models", c), ""),
            Err(e) => {
                let short: String = e.chars().take(60).collect();
                println!("  {} {n:<11} {}", dim("·"), dim(&short))
            }
        }
    }
    println!("{}", bold("Toolchain"));
    for (bin, arg, hint) in [
        ("cargo", "--version", "install Rust: https://rustup.rs"),
        ("rustc", "--version", ""),
        ("git", "--version", ""),
        ("node", "--version", "needed only for REA (rea.tools)"),
        ("npx", "--version", ""),
        (
            "analyzeHeadless",
            "-help",
            "optional: Ghidra headless for deep native analysis",
        ),
    ] {
        let out = std::process::Command::new(bin)
            .arg(arg)
            .stdin(std::process::Stdio::null())
            .output();
        match out {
            Ok(o) if o.status.success() || bin == "analyzeHeadless" => ok(
                true,
                &format!(
                    "{bin:<16} {}",
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                ),
                "",
            ),
            _ => ok(
                bin != "cargo" && bin != "rustc",
                &format!("{bin:<16} not found"),
                hint,
            ),
        }
    }
    println!("{}", bold("MCP engines"));
    if cfg.mcp.is_empty() {
        println!(
            "  {} none configured. For REA: see skills/rustport/references/rea.md",
            dim("·")
        );
    }
    for (n, e) in &cfg.mcp {
        ok(
            !e.command.is_empty(),
            &format!("{n}: {}", e.command.join(" ")),
            "set \"command\" in portcode.json",
        );
    }
    println!("{}", bold("Project"));
    let root = crate::root()?;
    ok(
        root.join("portcode.json").exists() || root.join(".portcode/portcode.json").exists(),
        "portcode.json",
        "run `portcode init`",
    );
    ok(
        root.join("docs/PORT.md").exists(),
        "docs/PORT.md (port plan)",
        "run `portcode port scaffold <name>` for a new port",
    );
    Ok(0)
}

pub async fn eval(cfg: Config, model: Option<&str>) -> Result<i32, String> {
    let r = cfg.resolve(model)?;
    let c = Client::new(r.provider.clone()).map_err(|e| e.to_string())?;
    println!(
        "{} {}/{}  {}",
        bold("eval"),
        r.provider.name,
        r.model,
        dim(&format!("{:?}", r.profile.family))
    );
    let mut failed = 0;
    let mut report = |name: &str, ok: bool, ms: u128, note: String| {
        if !ok {
            failed += 1;
        }
        println!(
            "  {} {name:<22} {:>6} ms  {}",
            if ok { green("✓") } else { red("✗") },
            ms,
            dim(&note)
        );
    };

    let t = std::time::Instant::now();
    let a = c
        .chat(
            &r.model,
            &r.profile,
            &[Message::user("Reply with exactly one word: pong")],
            &[],
            &mut |_| {},
        )
        .await;
    report(
        "chat",
        a.as_ref()
            .map(|a| a.text.to_lowercase().contains("pong"))
            .unwrap_or(false),
        t.elapsed().as_millis(),
        a.map(|a| a.text.chars().take(40).collect())
            .unwrap_or_else(|e| e.to_string().chars().take(80).collect()),
    );

    let tool = ToolDef::new(
        "read",
        "Read a file",
        serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
    );
    let t = std::time::Instant::now();
    let a = c
        .chat(
            &r.model,
            &r.profile,
            &[
                Message::system("You use tools. Never answer in text when a tool fits."),
                Message::user("Call the read tool for README.md."),
            ],
            &[tool],
            &mut |_| {},
        )
        .await;
    let (ok, note) = match a {
        Ok(a) => match a.tool_calls.first() {
            Some(tc) => {
                let v: Result<serde_json::Value, _> = serde_json::from_str(&tc.function.arguments);
                (
                    tc.function.name == "read"
                        && v.as_ref()
                            .map(|v| v["path"] == "README.md")
                            .unwrap_or(false),
                    format!("{}({})", tc.function.name, tc.function.arguments),
                )
            }
            None => (
                false,
                format!(
                    "no tool call; text: {}",
                    a.text.chars().take(50).collect::<String>()
                ),
            ),
        },
        Err(e) => (false, e.to_string().chars().take(80).collect()),
    };
    report("tool-call + JSON", ok, t.elapsed().as_millis(), note);

    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("notes.txt"), "status: old\n").map_err(|e| e.to_string())?;
    let t = std::time::Instant::now();
    let mut ag = Runner::new(
        dir.path().to_path_buf(),
        cfg.clone(),
        model,
        "build",
        Arc::new(AutoGate(Reply::Once)),
        Arc::new(portcode_agent::NullUi),
    )
    .await?;
    let o = ag
        .send("In notes.txt replace the word 'old' with 'new'. Then say done.")
        .await;
    let content = std::fs::read_to_string(dir.path().join("notes.txt")).unwrap_or_default();
    report(
        "agent edit (read→edit)",
        content == "status: new\n",
        t.elapsed().as_millis(),
        match o {
            Ok(o) => format!("{} steps, {:?}", o.steps, o.status),
            Err(e) => e.chars().take(80).collect(),
        },
    );
    println!(
        "{}",
        if failed == 0 {
            green("all checks passed")
        } else {
            red(&format!("{failed} check(s) failed — try a larger model, or set \"parallel_tool_calls\": false / adjust the profile under \"models\""))
        }
    );
    Ok(if failed == 0 { 0 } else { 1 })
}

pub fn init(force: bool) -> Result<i32, String> {
    let p = crate::root()?.join("portcode.json");
    if p.exists() && !force {
        return Err(format!("{} exists (use --force)", p.display()));
    }
    std::fs::write(&p, portcode_agent::config::INIT_TEMPLATE).map_err(|e| e.to_string())?;
    let gi = crate::root()?.join(".gitignore");
    let cur = std::fs::read_to_string(&gi).unwrap_or_default();
    if !cur.lines().any(|l| l.trim() == ".portcode/") {
        let _ = std::fs::write(
            &gi,
            format!(
                "{cur}{}.portcode/\n",
                if cur.is_empty() || cur.ends_with('\n') {
                    ""
                } else {
                    "\n"
                }
            ),
        );
    }
    println!("{} wrote {}\n  next: export LAKOMOOR_API_KEY=… (or `portcode auth set lakomoor`), then `portcode doctor` and `portcode`", green("✓"), p.display());
    Ok(0)
}

pub fn auth_set(provider: &str, from_env: Option<&str>) -> Result<i32, String> {
    let key = match from_env {
        Some(e) => std::env::var(e).map_err(|_| format!("${e} is not set"))?,
        None => {
            if std::io::stdin().is_terminal() {
                eprint!("API key for {provider} (input is visible; prefer --from-env): ");
                let _ = std::io::stderr().flush();
            }
            let mut s = String::new();
            std::io::stdin()
                .read_line(&mut s)
                .map_err(|e| e.to_string())?;
            s.trim().to_string()
        }
    };
    if key.is_empty() {
        return Err("empty key".into());
    }
    let p = portcode_agent::config::store_key(provider, &key)?;
    println!(
        "{} saved key for {provider} in {} (mode 600)",
        green("✓"),
        p.display()
    );
    Ok(0)
}
pub fn auth_list() -> Result<i32, String> {
    for p in portcode_llm::presets() {
        println!(
            "{:<11} {}",
            p.name,
            if portcode_agent::config::stored_key(&p.name).is_some() {
                green("stored")
            } else {
                dim("—")
            }
        );
    }
    Ok(0)
}

pub fn skill(op: SkillOp) -> Result<i32, String> {
    let skills = portcode_agent::skills::discover(&crate::root()?);
    match op {
        SkillOp::List => {
            for s in &skills {
                println!(
                    "{:<14} {}",
                    bold(&s.name),
                    s.description.chars().take(110).collect::<String>()
                );
            }
        }
        SkillOp::Show(n, f) => {
            let s = skills
                .iter()
                .find(|s| s.name == n)
                .ok_or_else(|| format!("no skill '{n}'"))?;
            print!("{}", s.file(f.as_deref())?);
        }
        SkillOp::Export(dir) => {
            let s = portcode_agent::skills::bundled();
            let base = std::path::Path::new(&dir).join(&s.name);
            for (p, t) in s.embedded {
                let f = base.join(p);
                if let Some(d) = f.parent() {
                    std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
                }
                std::fs::write(&f, t).map_err(|e| e.to_string())?;
            }
            println!("{} exported {} files to {}\n  Claude Code: ~/.claude/skills  ·  OpenCode: ~/.config/opencode/skills  ·  Codex: ~/.agents/skills", green("✓"), s.embedded.len(), base.display());
        }
    }
    Ok(0)
}

pub async fn run_tool(cfg: Config, name: &str, args: &str) -> Result<i32, String> {
    let root = crate::root()?;
    let reg = portcode_agent::tools::builtin(portcode_agent::skills::discover(&root));
    let tool = reg
        .get(name)
        .ok_or_else(|| format!("unknown tool '{name}'. Tools: {}", reg.names().join(", ")))?;
    let v: serde_json::Value = serde_json::from_str(args)
        .map_err(|e| format!("args must be JSON: {e}. Example: {}", tool.example()))?;
    // the user typed this command explicitly, so a second permission prompt would be noise
    let ctx = Ctx::new(
        root,
        Arc::new(TermGate { auto: true }),
        Rules::from_config(&cfg.permission),
    );
    match tool.run(&ctx, v).await {
        Ok(o) => {
            println!("{}", o.text);
            Ok(if o.title == "parity FAIL" { 1 } else { 0 })
        }
        Err(e) => Err(e.0),
    }
}

pub async fn port(cfg: Config, (tool, args): (String, serde_json::Value)) -> Result<i32, String> {
    run_tool(cfg, &tool, &args.to_string()).await
}

pub fn render(out: &str, title: &str, command: &str) -> Result<i32, String> {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).map_err(|e| e.to_string())?;
    let svg = portcode_port::shot::render_svg(title, command, &text, 110, 60);
    std::fs::write(out, svg).map_err(|e| format!("{out}: {e}"))?;
    println!("wrote {out}");
    Ok(0)
}

pub fn page(input: &str, out: &str, title: &str, css: &str, home: &str) -> Result<i32, String> {
    let md = std::fs::read_to_string(input).map_err(|e| format!("{input}: {e}"))?;
    if let Some(d) = std::path::Path::new(out).parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(out, portcode_port::site::doc_page(title, &md, css, home))
        .map_err(|e| format!("{out}: {e}"))?;
    println!("wrote {out}");
    Ok(0)
}
