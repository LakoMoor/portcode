//! `portcode` — open-source coding and reverse-engineering agent for local and self-hosted LLMs.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
mod cmds;
mod remote;
mod term;
mod tui;

use clap::{Parser, Subcommand};
use portcode_agent::{Config, Runner, Status};
use std::sync::Arc;
use term::*;

#[derive(Parser)]
#[command(
    name = "portcode",
    version,
    about = "PortCode — port programs to Rust with evidence, using local or self-hosted LLMs (lakomoor.com/v1, Ollama, LM Studio, llama.cpp, vLLM…)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
    /// Model as provider/model, e.g. lakomoor/deepseek-chat or ollama/qwen3:14b
    #[arg(short, long, global = true)]
    model: Option<String>,
    /// Agent: build, port, reverse, plan
    #[arg(short, long, global = true)]
    agent: Option<String>,
    /// Show the model's reasoning stream
    #[arg(long, global = true)]
    show_reasoning: bool,
    /// Deep mode: may disassemble, decompile and emulate the original (you own it or may examine it)
    #[arg(long, global = true, conflicts_with = "clean_room")]
    deep: bool,
    /// Clean-room mode (default): observe the original from outside only
    #[arg(long, global = true)]
    clean_room: bool,
}

#[derive(Subcommand)]
enum RemoteCmd {
    /// Create a pairing key (shown once): scope `full` (default) or `view`
    New {
        #[arg(long, default_value = "full")]
        scope: String,
    },
    /// List keys (ids and scopes only)
    List,
    /// Revoke a key
    Revoke { id: String },
}

#[derive(Subcommand)]
enum EnginesCmd {
    /// Show what is installed
    List,
    /// Install engines: an id (unicorn, capstone, ghidra, imhex…), `core` (capstone+unicorn+lief) or `all`
    Install { ids: Vec<String> },
    /// Print the engines folder
    Path,
}

#[derive(Subcommand)]
enum Cmd {
    /// Interactive chat (default)
    Chat {
        #[arg(short = 'c', long)]
        r#continue: bool,
        /// Line-based chat instead of the full-screen UI
        #[arg(long)]
        plain: bool,
    },
    /// Render the full-screen UI headlessly to an SVG screenshot
    #[command(hide = true)]
    Snapshot {
        out: String,
        #[arg(long, default_value = "120x36")]
        size: String,
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long)]
        until_permission: bool,
        #[arg(long)]
        typed: Option<String>,
        #[arg(long)]
        cwd_label: Option<String>,
    },
    /// Run one prompt non-interactively
    Run {
        prompt: Vec<String>,
        /// Allow every permission prompt (use only in a sandbox / trusted project)
        #[arg(short = 'y', long)]
        yes: bool,
        /// Continue the latest session
        #[arg(short = 'c', long)]
        r#continue: bool,
        /// Emit JSON lines instead of text
        #[arg(long)]
        json: bool,
    },
    /// List models served by a provider
    Models {
        #[arg(short, long)]
        provider: Option<String>,
    },
    /// Show configured and built-in providers
    Providers,
    /// Check providers, toolchain and optional engines
    Doctor,
    /// Probe a model: chat, tool-calling, JSON validity, edit accuracy
    Eval,
    /// Create portcode.json in this directory
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Store an API key (mode 0600)
    Auth {
        #[command(subcommand)]
        sub: AuthCmd,
    },
    /// Print shell completions (bash, zsh, fish, powershell, elvish)
    Completions { shell: clap_complete::Shell },
    /// List agents
    Agents,
    /// Run a session you can drive from the browser (remote control); see docs/REMOTE.md
    Serve {
        /// Address to listen on; keep 127.0.0.1 and use a TLS tunnel for remote access
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        #[arg(long, default_value_t = 7777)]
        port: u16,
    },
    /// Manage remote-control keys
    Remote {
        #[command(subcommand)]
        sub: RemoteCmd,
    },
    /// Reverse-engineering engines (Capstone, Unicorn, Ghidra, ImHex…): list, install, path
    Engines {
        #[command(subcommand)]
        sub: EnginesCmd,
    },
    /// Skills: list, show, export
    Skill {
        #[command(subcommand)]
        sub: SkillCmd,
    },
    /// Porting workbench (survey, pin, parity, scaffold, shot, site, ledger)
    Port {
        #[command(subcommand)]
        sub: PortCmd,
    },
    /// Run any built-in tool directly with JSON arguments
    Tool {
        name: String,
        #[arg(default_value = "{}")]
        args: String,
    },
}

#[derive(Subcommand)]
enum AuthCmd {
    Set {
        provider: String,
        /// Read the key from this env var instead of prompting
        #[arg(long)]
        from_env: Option<String>,
    },
    List,
}
#[derive(Subcommand)]
enum SkillCmd {
    List,
    Show { name: String, file: Option<String> },
    Export { dir: String },
}
#[derive(Subcommand)]
enum PortCmd {
    Survey {
        path: Option<String>,
    },
    Pin {
        file: String,
        #[arg(long)]
        version: Option<String>,
    },
    Parity {
        golden: String,
        port: String,
        #[arg(long, default_value_t = 1e-5)]
        abs_tol: f64,
        #[arg(long, default_value_t = 1e-4)]
        rel_tol: f64,
    },
    Scaffold {
        name: String,
        #[arg(long)]
        dest: Option<String>,
    },
    /// Run a command and save its output as a terminal screenshot + evidence record
    Shot {
        name: String,
        #[arg(short, long)]
        subject: String,
        #[arg(long)]
        statement: Option<String>,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    Site {
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        out: Option<String>,
        #[arg(long)]
        repo_url: Option<String>,
    },
    /// Render text from stdin as a terminal screenshot (SVG), e.g. a saved log
    Render {
        out: String,
        #[arg(long, default_value = "portcode")]
        title: String,
        #[arg(long, default_value = "")]
        command: String,
    },
    /// Render a markdown file as a docs page with the PortCode layout
    Page {
        input: String,
        out: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "../style.css")]
        css: String,
        #[arg(long, default_value = "../")]
        home: String,
    },
    Ledger {
        #[arg(long)]
        subject: Option<String>,
        #[arg(long)]
        unknowns: bool,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let code = match real_main(cli).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {e}", red("error:"));
            1
        }
    };
    std::process::exit(code);
}

pub fn root() -> Result<std::path::PathBuf, String> {
    std::env::current_dir().map_err(|e| e.to_string())
}

async fn real_main(cli: Cli) -> Result<i32, String> {
    let (deep, clean) = (cli.deep, cli.clean_room);
    let cfg = move || {
        let mut c = Config::load(&root()?)?;
        if let Ok(m) = std::env::var("PORTCODE_MODE") {
            c.mode = Some(m);
        }
        if deep {
            c.mode = Some("deep".into());
        }
        if clean {
            c.mode = Some("clean-room".into());
        }
        Ok::<_, String>(c)
    };
    let serve_opts = tui_opts(&cli, false);
    match cli.cmd {
        None => {
            let tty = std::io::IsTerminal::is_terminal(&std::io::stdin())
                && std::io::IsTerminal::is_terminal(&std::io::stdout());
            if tty {
                tui::run(cfg()?, tui_opts(&cli, false)).await
            } else {
                cmds::chat(cfg()?, &cli.model, &cli.agent, cli.show_reasoning, false).await
            }
        }
        Some(Cmd::Chat { r#continue, plain }) => {
            if plain {
                cmds::chat(
                    cfg()?,
                    &cli.model,
                    &cli.agent,
                    cli.show_reasoning,
                    r#continue,
                )
                .await
            } else {
                tui::run(cfg()?, tui_opts(&cli, r#continue)).await
            }
        }
        Some(Cmd::Snapshot {
            ref out,
            ref size,
            ref prompt,
            until_permission,
            ref typed,
            ref cwd_label,
        }) => {
            let (w, h) = size
                .split_once('x')
                .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                .ok_or("--size must look like 120x36")?;
            let mut o = tui_opts(&cli, false);
            o.cwd_label = cwd_label.clone();
            tui::snapshot(
                cfg()?,
                o,
                tui::Snap {
                    out: out.clone(),
                    w,
                    h,
                    prompt: prompt.clone(),
                    until_permission,
                    typed: typed.clone(),
                },
            )
            .await
        }
        Some(Cmd::Run {
            prompt,
            yes,
            r#continue,
            json,
        }) => {
            let mut text = prompt.join(" ");
            if text.trim().is_empty() && !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
                    .map_err(|e| e.to_string())?;
            }
            if text.trim().is_empty() {
                return Err("no prompt: portcode run \"your request\"".into());
            }
            let ui: Arc<dyn portcode_agent::Ui> = if json {
                Arc::new(JsonUi)
            } else {
                Arc::new(Term::new(cli.show_reasoning))
            };
            let mut r = Runner::new(
                root()?,
                cfg()?,
                cli.model.as_deref(),
                cli.agent.as_deref().unwrap_or("build"),
                Arc::new(TermGate { auto: yes }),
                ui,
            )
            .await?;
            cmds::watch_ctrl_c(r.cancel_flag());
            if r#continue {
                if let Some(id) = r.latest_session() {
                    r.resume(&id)?;
                }
            }
            let out = r.send(&text).await?;
            if !json {
                println!(
                    "\n{}",
                    dim(&format!(
                        "— {} step(s) · {} in / {} out tokens · {:?}",
                        out.steps, out.usage.prompt_tokens, out.usage.completion_tokens, out.status
                    ))
                );
            }
            Ok(if out.status == Status::Done { 0 } else { 1 })
        }
        Some(Cmd::Models { provider }) => cmds::models(cfg()?, provider.or(cli.model)).await,
        Some(Cmd::Providers) => cmds::providers(cfg()?),
        Some(Cmd::Doctor) => cmds::doctor(cfg()?).await,
        Some(Cmd::Eval) => cmds::eval(cfg()?, cli.model.as_deref()).await,
        Some(Cmd::Init { force }) => cmds::init(force),
        Some(Cmd::Auth { sub }) => match sub {
            AuthCmd::Set { provider, from_env } => cmds::auth_set(&provider, from_env.as_deref()),
            AuthCmd::List => cmds::auth_list(),
        },
        Some(Cmd::Completions { shell }) => {
            clap_complete::generate(
                shell,
                &mut <Cli as clap::CommandFactory>::command(),
                "portcode",
                &mut std::io::stdout(),
            );
            Ok(0)
        }
        Some(Cmd::Agents) => {
            for a in portcode_agent::agents::all() {
                println!(
                    "{:<9} {} {}",
                    bold(a.name),
                    if a.primary { "      " } else { "(sub) " },
                    a.description
                );
            }
            Ok(0)
        }
        Some(Cmd::Serve { bind, port }) => {
            let ip: std::net::IpAddr = bind.parse().map_err(|_| format!("bad --bind '{bind}'"))?;
            tui::serve(cfg()?, serve_opts, (ip, port).into()).await
        }
        Some(Cmd::Remote { sub }) => cmds::remote_keys(match sub {
            RemoteCmd::New { scope } => ("new".into(), scope),
            RemoteCmd::List => ("list".into(), String::new()),
            RemoteCmd::Revoke { id } => ("revoke".into(), id),
        }),
        Some(Cmd::Engines { sub }) => match sub {
            EnginesCmd::List => cmds::engines_list(),
            EnginesCmd::Path => {
                println!("{}", portcode_agent::engines::dir().display());
                Ok(0)
            }
            EnginesCmd::Install { ids } => cmds::engines_install(&ids),
        },
        Some(Cmd::Skill { sub }) => cmds::skill(sub_skill(sub)),
        Some(Cmd::Port {
            sub:
                PortCmd::Render {
                    out,
                    title,
                    command,
                },
        }) => cmds::render(&out, &title, &command),
        Some(Cmd::Port {
            sub:
                PortCmd::Page {
                    input,
                    out,
                    title,
                    css,
                    home,
                },
        }) => cmds::page(&input, &out, &title, &css, &home),
        Some(Cmd::Port { sub }) => cmds::port(cfg()?, sub_port(sub)).await,
        Some(Cmd::Tool { name, args }) => cmds::run_tool(cfg()?, &name, &args).await,
    }
}

fn tui_opts(cli: &Cli, cont: bool) -> tui::Opts {
    tui::Opts {
        model: cli.model.clone(),
        agent: cli.agent.clone().unwrap_or_else(|| "build".into()),
        cont,
        show_reasoning: cli.show_reasoning,
        cwd_label: None,
    }
}

fn sub_skill(s: SkillCmd) -> cmds::SkillOp {
    match s {
        SkillCmd::List => cmds::SkillOp::List,
        SkillCmd::Show { name, file } => cmds::SkillOp::Show(name, file),
        SkillCmd::Export { dir } => cmds::SkillOp::Export(dir),
    }
}
fn sub_port(p: PortCmd) -> (String, serde_json::Value) {
    use serde_json::json;
    match p {
        PortCmd::Survey { path } => (
            "survey".into(),
            json!({"path": path.unwrap_or_else(|| ".".into())}),
        ),
        PortCmd::Pin { file, version } => ("pin".into(), json!({"path": file, "version": version})),
        PortCmd::Parity {
            golden,
            port,
            abs_tol,
            rel_tol,
        } => (
            "parity".into(),
            json!({"golden": golden, "port": port, "abs_tol": abs_tol, "rel_tol": rel_tol}),
        ),
        PortCmd::Scaffold { name, dest } => {
            ("scaffold".into(), json!({"name": name, "dest": dest}))
        }
        PortCmd::Shot {
            name,
            subject,
            statement,
            command,
        } => (
            "screenshot".into(),
            json!({"name": name, "subject": subject, "statement": statement, "command": command.join(" ")}),
        ),
        PortCmd::Site {
            title,
            out,
            repo_url,
        } => (
            "site_build".into(),
            json!({"title": title, "out": out, "repo_url": repo_url}),
        ),
        PortCmd::Render { .. } | PortCmd::Page { .. } => (String::new(), json!({})),
        PortCmd::Ledger { subject, unknowns } => (
            "evidence_list".into(),
            json!({"subject": subject, "unknowns_only": unknowns}),
        ),
    }
}
