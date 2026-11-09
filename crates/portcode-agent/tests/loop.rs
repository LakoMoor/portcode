//! End-to-end: the real agent loop + real tools against a scripted model.
use portcode_agent::*;
use portcode_testkit::{start, ScriptedCall, Turn};
use serde_json::json;
use std::sync::{Arc, Mutex};

struct Rec(Mutex<Vec<UiEvent>>);
impl Ui for Rec {
    fn event(&self, e: UiEvent) {
        self.0.lock().unwrap().push(e);
    }
}

fn call(n: &str, a: serde_json::Value) -> ScriptedCall {
    ScriptedCall {
        name: n.into(),
        arguments: a,
    }
}
fn cfg(url: &str) -> Config {
    let mut c = Config::default();
    c.provider.insert(
        "mock".into(),
        config::ProviderEntry {
            base_url: Some(url.into()),
            ..Default::default()
        },
    );
    c.model = Some("mock/mock-model".into());
    c
}
async fn runner(dir: &std::path::Path, url: &str, agent: &str, gate: Reply) -> (Runner, Arc<Rec>) {
    let ui = Arc::new(Rec(Mutex::new(vec![])));
    (
        Runner::new(
            dir.to_path_buf(),
            cfg(url),
            None,
            agent,
            Arc::new(AutoGate(gate)),
            ui.clone(),
        )
        .await
        .unwrap(),
        ui,
    )
}

#[tokio::test]
async fn reads_edits_and_reports() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.txt"), "hello world\n").unwrap();
    let m = start(
        vec![
            Turn {
                tool_calls: vec![call("read", json!({"path":"a.txt"}))],
                ..Default::default()
            },
            Turn {
                reasoning: "plan".into(),
                tool_calls: vec![call(
                    "edit",
                    json!({"path":"a.txt","old_string":"world","new_string":"rust"}),
                )],
                ..Default::default()
            },
            Turn {
                text: "Changed greeting.".into(),
                ..Default::default()
            },
        ],
        0,
    )
    .await
    .unwrap();
    let (mut r, ui) = runner(d.path(), &m.url, "build", Reply::Once).await;
    let out = r.send("change world to rust").await.unwrap();
    assert_eq!(
        (out.status, out.steps, out.text.as_str()),
        (Status::Done, 3, "Changed greeting.")
    );
    assert_eq!(
        std::fs::read_to_string(d.path().join("a.txt")).unwrap(),
        "hello rust\n"
    );
    assert!(out.usage.prompt_tokens > 0);
    // tools were advertised, system prompt carried the rules, results were fed back
    let reqs = m.requests.lock().unwrap().clone();
    assert!(reqs[0]["tools"].as_array().unwrap().len() >= 10);
    assert!(reqs[0]["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("Act, don't narrate"));
    assert!(reqs[2]["messages"].as_array().unwrap().iter().any(
        |m| m["role"] == "tool" && m["content"].as_str().unwrap_or("").contains("Edited a.txt")
    ));
    assert!(ui
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, UiEvent::ToolResult { name, ok: true, .. } if name == "edit")));
    // session was persisted and is resumable
    let id = r.session_id.clone();
    let (mut r2, _) = runner(d.path(), &m.url, "build", Reply::Once).await;
    assert!(r2.resume(&id).unwrap().1 >= 6);
}

#[tokio::test]
async fn bad_tool_use_is_corrected_not_fatal() {
    let d = tempfile::tempdir().unwrap();
    let m = start(
        vec![
            Turn {
                tool_calls: vec![call("nope", json!({}))],
                ..Default::default()
            },
            Turn {
                tool_calls: vec![call("read", json!({}))],
                ..Default::default()
            },
            Turn {
                text: "gave up".into(),
                ..Default::default()
            },
        ],
        0,
    )
    .await
    .unwrap();
    let (mut r, _) = runner(d.path(), &m.url, "build", Reply::Once).await;
    r.send("x").await.unwrap();
    let reqs = m.requests.lock().unwrap().clone();
    let msgs = reqs[2]["messages"].as_array().unwrap();
    let tools: Vec<&str> = msgs
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap())
        .collect();
    assert!(tools[0].contains("unknown tool 'nope'") && tools[0].contains("read"));
    assert!(tools[1].contains("missing required parameter 'path'") && tools[1].contains("Example"));
}

#[tokio::test]
async fn text_style_tool_calls_work_for_qwen_like_servers() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("x.rs"), "fn main() {}\n").unwrap();
    let m = start(
        vec![
            Turn {
                text: "Looking.".into(),
                as_text: true,
                tool_calls: vec![call("read", json!({"path":"x.rs"}))],
                ..Default::default()
            },
            Turn {
                text: "It has main.".into(),
                ..Default::default()
            },
        ],
        0,
    )
    .await
    .unwrap();
    let (mut r, _) = runner(d.path(), &m.url, "build", Reply::Once).await;
    let o = r.send("what is in x.rs").await.unwrap();
    assert_eq!(o.text, "It has main.");
    assert!(m.requests.lock().unwrap()[1]["messages"]
        .to_string()
        .contains("fn main"));
}

#[tokio::test]
async fn plan_agent_cannot_edit_and_hides_write_tools() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a"), "1").unwrap();
    let m = start(
        vec![
            Turn {
                tool_calls: vec![call("write", json!({"path":"a","content":"2"}))],
                ..Default::default()
            },
            Turn {
                text: "ok".into(),
                ..Default::default()
            },
        ],
        0,
    )
    .await
    .unwrap();
    let (mut r, _) = runner(d.path(), &m.url, "plan", Reply::Once).await;
    r.send("change a").await.unwrap();
    assert_eq!(std::fs::read_to_string(d.path().join("a")).unwrap(), "1");
    let names: Vec<String> = m.requests.lock().unwrap()[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap().to_string())
        .collect();
    assert!(!names.contains(&"write".to_string()) && names.contains(&"read".to_string()));
}

#[tokio::test]
async fn repeated_identical_calls_are_stopped() {
    let d = tempfile::tempdir().unwrap();
    let same = || Turn {
        tool_calls: vec![call("ls", json!({}))],
        ..Default::default()
    };
    let m = start((0..12).map(|_| same()).collect(), 0).await.unwrap();
    let (mut r, _) = runner(d.path(), &m.url, "build", Reply::Once).await;
    let o = r.send("loop").await.unwrap();
    assert_eq!(o.status, Status::Stuck);
    assert!(o.steps < 8);
}

#[tokio::test]
async fn subagent_runs_with_fresh_context() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("f.rs"), "// needle\n").unwrap();
    let m = start(
        vec![
            Turn {
                tool_calls: vec![call(
                    "task",
                    json!({"agent":"explore","prompt":"find needle"}),
                )],
                ..Default::default()
            },
            Turn {
                tool_calls: vec![call("grep", json!({"pattern":"needle"}))],
                ..Default::default()
            }, // sub-agent step 1
            Turn {
                text: "f.rs:1 — needle".into(),
                ..Default::default()
            }, // sub-agent final
            Turn {
                text: "Found it in f.rs.".into(),
                ..Default::default()
            }, // parent final
        ],
        0,
    )
    .await
    .unwrap();
    let (mut r, _) = runner(d.path(), &m.url, "build", Reply::Once).await;
    let o = r.send("where is needle?").await.unwrap();
    assert_eq!(o.text, "Found it in f.rs.");
    let reqs = m.requests.lock().unwrap().clone();
    assert_eq!(
        reqs[1]["messages"].as_array().unwrap().len(),
        2,
        "sub-agent sees only system + its prompt"
    );
    assert!(reqs[3]["messages"].to_string().contains("f.rs:1"));
}

#[tokio::test]
async fn port_workflow_tools_produce_ledger_and_site() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("orig.c"),
        "#include <SDL2/SDL.h>\nint main(){return 0;}\n",
    )
    .unwrap();
    let m = start(vec![
        Turn { tool_calls: vec![call("pin", json!({"path":"orig.c","version":"1.0"}))], ..Default::default() },
        Turn { tool_calls: vec![call("screenshot", json!({"command":"echo hello from original","name":"orig-run","subject":"cli.hello"}))], ..Default::default() },
        Turn { tool_calls: vec![call("evidence_add", json!({"kind":"unknown","subject":"save","statement":"checksum algorithm"}))], ..Default::default() },
        Turn { tool_calls: vec![call("site_build", json!({"title":"Demo port"}))], ..Default::default() },
        Turn { text: "done".into(), ..Default::default() },
    ], 0).await.unwrap();
    let (mut r, _) = runner(d.path(), &m.url, "port", Reply::Once).await;
    r.send("document it").await.unwrap();
    let ev = r.ctx.ledger.all().unwrap();
    assert_eq!(ev.len(), 3);
    assert_eq!(ev[1].attachments, vec!["screenshots/orig-run.svg"]);
    let html = std::fs::read_to_string(d.path().join("docs/site/index.html")).unwrap();
    assert!(
        html.contains("cli.hello")
            && html.contains("screenshots/orig-run.svg")
            && html.contains("checksum algorithm")
    );
    assert!(
        std::fs::read_to_string(d.path().join("docs/screenshots/orig-run.svg"))
            .unwrap()
            .contains("hello\u{a0}from\u{a0}original")
    );
}

#[tokio::test]
async fn port_agent_is_sent_back_until_it_is_really_done() {
    let d = tempfile::tempdir().unwrap();
    let m = start(
        vec![
            // claims it is done with no todos and no parity run
            Turn {
                text: "All done!".into(),
                ..Default::default()
            },
            // after the nudge it plans, but then tries to stop again with an open item
            Turn {
                tool_calls: vec![call(
                    "todowrite",
                    json!({"todos":[{"content":"--top option","status":"in_progress"}]}),
                )],
                ..Default::default()
            },
            Turn {
                text: "Finished.".into(),
                ..Default::default()
            },
            // only the user can decide this one
            Turn {
                tool_calls: vec![call(
                    "blocked",
                    json!({"reason":"need a golden for --top 0"}),
                )],
                ..Default::default()
            },
        ],
        0,
    )
    .await
    .unwrap();
    let (mut r, ui) = runner(d.path(), &m.url, "port", Reply::Once).await;
    let out = r.send("port it").await.unwrap();
    assert_eq!(
        out.steps, 4,
        "two premature stops must each be answered with another step"
    );
    assert!(
        out.text.starts_with("Blocked, needs you: need a golden"),
        "{}",
        out.text
    );
    let infos: Vec<String> =
        ui.0.lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                UiEvent::Info(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
    assert!(
        infos
            .iter()
            .any(|s| s.contains("not finished: no todo list")),
        "{infos:?}"
    );
    assert!(
        infos
            .iter()
            .any(|s| s.contains("todo not done: --top option")),
        "{infos:?}"
    );
}

#[tokio::test]
async fn build_agent_is_not_gated() {
    let d = tempfile::tempdir().unwrap();
    let m = start(
        vec![Turn {
            text: "ok".into(),
            ..Default::default()
        }],
        0,
    )
    .await
    .unwrap();
    let (mut r, _) = runner(d.path(), &m.url, "build", Reply::Once).await;
    assert_eq!(r.send("hi").await.unwrap().steps, 1);
}

#[tokio::test]
async fn a_tool_call_cut_by_the_output_limit_does_not_run() {
    let d = tempfile::tempdir().unwrap();
    let m = start(
        vec![
            Turn {
                tool_calls: vec![call(
                    "write",
                    json!({"path":"big.txt","content":"x".repeat(400)}),
                )],
                truncated: true,
                ..Default::default()
            },
            Turn {
                tool_calls: vec![call(
                    "write",
                    json!({"path":"big.txt","content":"part one\n"}),
                )],
                ..Default::default()
            },
            Turn {
                tool_calls: vec![call(
                    "write",
                    json!({"path":"big.txt","content":"part two\n","append":true}),
                )],
                ..Default::default()
            },
            Turn {
                text: "done".into(),
                ..Default::default()
            },
        ],
        0,
    )
    .await
    .unwrap();
    let (mut r, ui) = runner(d.path(), &m.url, "build", Reply::Once).await;
    let out = r.send("write a big file").await.unwrap();
    assert_eq!(out.text, "done");
    assert_eq!(
        std::fs::read_to_string(d.path().join("big.txt")).unwrap(),
        "part one\npart two\n",
        "the cut-off call must not have written a truncated file, and append must extend"
    );
    let reqs = m.requests.lock().unwrap();
    let second = serde_json::to_string(&reqs[1]["messages"]).unwrap();
    assert!(second.contains("cut off by the output limit"));
    assert!(ui
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, UiEvent::Info(t) if t.contains("cut off"))));
}

#[tokio::test]
async fn a_cut_off_answer_is_continued_and_joined() {
    let d = tempfile::tempdir().unwrap();
    let m = start(
        vec![
            Turn {
                text: "Hello wor".into(),
                truncated: true,
                ..Default::default()
            },
            Turn {
                text: "ld".into(),
                ..Default::default()
            },
        ],
        0,
    )
    .await
    .unwrap();
    let (mut r, _ui) = runner(d.path(), &m.url, "build", Reply::Once).await;
    let out = r.send("say hello").await.unwrap();
    assert_eq!(out.text, "Hello world");
}

#[tokio::test]
async fn a_small_window_limits_how_much_output_is_requested() {
    let d = tempfile::tempdir().unwrap();
    let m = start(
        vec![Turn {
            text: "ok".into(),
            ..Default::default()
        }],
        0,
    )
    .await
    .unwrap();
    let mut c = cfg(&m.url);
    c.models.insert(
        "mock-model".into(),
        json!({"context_window": 1536, "max_tokens": 4096}),
    );
    let ui = Arc::new(Rec(Mutex::new(vec![])));
    let mut r = Runner::new(
        d.path().to_path_buf(),
        c,
        None,
        "build",
        Arc::new(AutoGate(Reply::Once)),
        ui.clone(),
    )
    .await
    .unwrap();
    r.send("hi").await.unwrap();
    let reqs = m.requests.lock().unwrap();
    let max = reqs[0]["max_tokens"].as_u64().unwrap();
    assert!(
        max <= 2048,
        "asked for {max} output tokens in a 4096 window"
    );
    assert!(ui
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, UiEvent::Info(t) if t.contains("context window is small"))));
}

#[tokio::test]
async fn compact_replaces_the_conversation_with_a_summary_and_keeps_going() {
    let d = tempfile::tempdir().unwrap();
    let m = start(
        vec![
            Turn {
                text: "First answer.".into(),
                ..Default::default()
            },
            Turn {
                text: "GOAL: fix parser. Files: a.rs. Next: run tests.".into(),
                ..Default::default()
            },
            Turn {
                text: "Carrying on.".into(),
                ..Default::default()
            },
        ],
        0,
    )
    .await
    .unwrap();
    let (mut r, _) = runner(d.path(), &m.url, "build", Reply::Once).await;
    assert!(
        r.compact().await.is_err(),
        "nothing to compact in a fresh session"
    );
    r.send("fix the parser").await.unwrap();
    let msg = r.compact().await.unwrap();
    assert!(msg.contains("compacted"), "{msg}");
    assert_eq!(r.history().len(), 1);
    assert!(r.history()[0]
        .content
        .as_deref()
        .unwrap_or("")
        .contains("GOAL: fix parser"));
    r.send("continue").await.unwrap();
    let reqs = m.requests.lock().unwrap().clone();
    let last = reqs.last().unwrap()["messages"].as_array().unwrap().clone();
    assert!(last.iter().any(|x| x["content"]
        .as_str()
        .unwrap_or("")
        .contains("[Summary of the earlier conversation]")));
}

#[tokio::test]
async fn cancel_stops_a_model_request_that_never_answers() {
    // a server that accepts the connection and then says nothing
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", l.local_addr().unwrap());
    tokio::spawn(async move {
        let mut held = vec![];
        while let Ok((s, _)) = l.accept().await {
            held.push(s);
        }
    });
    let d = tempfile::tempdir().unwrap();
    let (mut r, _ui) = runner(d.path(), &url, "build", Reply::Once).await;
    let flag = r.cancel_flag();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let t = std::time::Instant::now();
    let out = tokio::time::timeout(std::time::Duration::from_secs(10), r.send("hello"))
        .await
        .expect("cancel must end the request")
        .unwrap();
    assert!(matches!(out.status, Status::Cancelled));
    assert!(t.elapsed() < std::time::Duration::from_secs(5));
}

#[tokio::test]
async fn a_dropped_connection_is_retried_and_the_turn_finishes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // the first request is cut off, the second is answered
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", l.local_addr().unwrap());
    tokio::spawn(async move {
        let mut n = 0;
        while let Ok((mut s, _)) = l.accept().await {
            n += 1;
            let mut buf = vec![0u8; 65536];
            let _ = s.read(&mut buf).await;
            if n == 1 {
                continue; // drop: connection closed with no answer
            }
            let body = "data: {\"choices\":[{\"delta\":{\"content\":\"hi there\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
            let head = format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len());
            let _ = s.write_all(head.as_bytes()).await;
            let _ = s.write_all(body.as_bytes()).await;
        }
    });
    let d = tempfile::tempdir().unwrap();
    let (mut r, ui) = runner(d.path(), &url, "build", Reply::Once).await;
    let out = tokio::time::timeout(std::time::Duration::from_secs(30), r.send("hello"))
        .await
        .expect("must not hang")
        .expect("the turn must survive one dropped connection");
    assert!(matches!(out.status, Status::Done));
    assert!(out.text.contains("hi there"), "{}", out.text);
    let seen = ui.0.lock().unwrap();
    assert!(seen
        .iter()
        .any(|e| matches!(e, UiEvent::Info(t) if t.contains("Reconnecting"))));
}

#[tokio::test]
async fn a_steering_message_joins_the_conversation_before_the_next_step() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.txt"), "x\n").unwrap();
    let m = start(
        vec![
            Turn {
                tool_calls: vec![call("read", json!({"path":"a.txt"}))],
                ..Default::default()
            },
            Turn {
                text: "ok".into(),
                ..Default::default()
            },
        ],
        0,
    )
    .await
    .unwrap();
    let (mut r, ui) = runner(d.path(), &m.url, "build", Reply::Once).await;
    r.steer_handle()
        .lock()
        .unwrap()
        .push("use the other file instead".into());
    let out = r.send("start").await.unwrap();
    assert_eq!(out.status, Status::Done);
    assert!(r.steer_handle().lock().unwrap().is_empty());
    assert!(r
        .history()
        .iter()
        .any(|m| m.content.as_deref() == Some("use the other file instead")));
    assert!(ui
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, UiEvent::Info(s) if s.starts_with("steering"))));
}
