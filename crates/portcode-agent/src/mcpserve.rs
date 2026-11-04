//! PortCode as an MCP server over stdio, so Claude Code and Codex (on the user's own subscription) can use the
//! evidence ledger, `survey`, `pin`, `parity`, `screenshot`, `scaffold`, `site_build` and, in deep mode, the
//! reverse-engineering tools. Newline-delimited JSON-RPC 2.0.
use crate::tool::{Ctx, Registry};
use serde_json::{json, Value};

/// Tools offered to the outside CLI. File and shell tools are left out on purpose: the CLI has its own.
pub const SHARED: &[&str] = &[
    "survey",
    "pin",
    "evidence_add",
    "evidence_list",
    "parity",
    "screenshot",
    "scaffold",
    "site_build",
];
pub const DEEP: &[&str] = &[
    "re_status",
    "disasm",
    "emulate",
    "decompile",
    "functions",
    "hexdump",
    "unity",
    "dotnet",
    "unity_assets",
];

fn allowed(name: &str, deep: bool) -> bool {
    SHARED.contains(&name) || (deep && DEEP.contains(&name))
}

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}
fn fail(id: &Value, code: i64, msg: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": msg}})
}

/// Answer one JSON-RPC message; `None` for notifications.
pub async fn handle(reg: &Registry, ctx: &Ctx, msg: &Value) -> Option<Value> {
    let method = msg.get("method")?.as_str()?;
    let id = msg.get("id")?.clone(); // no id: a notification
    let deep = ctx.is_deep();
    Some(match method {
        "initialize" => {
            let v = msg
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2024-11-05");
            reply(
                &id,
                json!({
                    "protocolVersion": v,
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "portcode", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": "PortCode: evidence ledger, pinning, parity checks and (deep mode) reverse-engineering tools. Record facts with evidence_add; never claim parity without a parity PASS."
                }),
            )
        }
        "ping" => reply(&id, json!({})),
        "tools/list" => {
            let tools: Vec<Value> = reg
                .names()
                .into_iter()
                .filter(|n| allowed(n, deep))
                .filter_map(|n| reg.get(&n))
                .map(|t| {
                    let ex = t.example();
                    let desc = if ex.is_empty() {
                        t.description()
                    } else {
                        format!("{}\nExample: {ex}", t.description())
                    };
                    json!({"name": t.name(), "description": desc, "inputSchema": t.schema()})
                })
                .collect();
            reply(&id, json!({ "tools": tools }))
        }
        "tools/call" => {
            let name = msg
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = msg
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match reg.get(name).filter(|_| allowed(name, deep)) {
                None => fail(&id, -32602, &format!("unknown tool '{name}'")),
                Some(t) => match t.run(ctx, args).await {
                    Ok(o) => reply(
                        &id,
                        json!({"content": [{"type": "text", "text": o.text}], "isError": false}),
                    ),
                    Err(e) => reply(
                        &id,
                        json!({"content": [{"type": "text", "text": e.0}], "isError": true}),
                    ),
                },
            }
        }
        other => fail(&id, -32601, &format!("method not found: {other}")),
    })
}

/// Serve on stdin/stdout until stdin closes.
pub async fn serve(reg: Registry, ctx: Ctx) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    while let Some(l) = lines.next_line().await.map_err(|e| e.to_string())? {
        let Ok(v) = serde_json::from_str::<Value>(&l) else {
            continue;
        };
        if let Some(r) = handle(&reg, &ctx, &v).await {
            let mut s = r.to_string();
            s.push('\n');
            out.write_all(s.as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            out.flush().await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::Rules;
    use std::sync::Arc;

    fn setup() -> (Registry, Ctx, tempfile::TempDir) {
        let d = tempfile::tempdir().unwrap();
        let ctx = Ctx::new(
            d.path().to_path_buf(),
            Arc::new(crate::run::DenyAsk),
            Rules::default(),
        );
        (crate::tools::builtin(vec![]), ctx, d)
    }

    #[tokio::test]
    async fn lists_only_the_shared_tools_and_deep_ones_when_deep() {
        let (reg, ctx, _d) = setup();
        let m = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
        let r = handle(&reg, &ctx, &m).await.unwrap();
        let names: Vec<&str> = r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(names.contains(&"evidence_add") && names.contains(&"parity"));
        assert!(
            !names.contains(&"bash") && !names.contains(&"write") && !names.contains(&"disasm")
        );
        ctx.deep.store(true, std::sync::atomic::Ordering::Relaxed);
        let r = handle(&reg, &ctx, &m).await.unwrap();
        assert!(r["result"]["tools"].to_string().contains("\"disasm\""));
    }

    #[tokio::test]
    async fn initialize_ping_and_notifications() {
        let (reg, ctx, _d) = setup();
        let r = handle(&reg, &ctx, &json!({"jsonrpc":"2.0","id":"a","method":"initialize","params":{"protocolVersion":"2025-03-26"}})).await.unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], "portcode");
        assert!(handle(
            &reg,
            &ctx,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .await
        .is_none());
        let r = handle(&reg, &ctx, &json!({"jsonrpc":"2.0","id":2,"method":"nope"}))
            .await
            .unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn calls_run_real_tools_and_refuse_hidden_ones() {
        let (reg, ctx, _d) = setup();
        let call = |name: &str, args: Value| json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":name,"arguments":args}});
        let r = handle(&reg, &ctx, &call("evidence_add", json!({"kind":"observation","statement":"prints hello","subject":"cli.hello","tool":"bash","args":"./app"}))).await.unwrap();
        assert_eq!(r["result"]["isError"], false, "{r}");
        let l = handle(&reg, &ctx, &call("evidence_list", json!({})))
            .await
            .unwrap();
        assert!(l["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("prints hello"));
        let h = handle(&reg, &ctx, &call("bash", json!({"command":"ls"})))
            .await
            .unwrap();
        assert!(h.get("error").is_some(), "bash is not exposed");
        let d = handle(&reg, &ctx, &call("disasm", json!({"file":"x"})))
            .await
            .unwrap();
        assert!(d.get("error").is_some(), "deep tools need --deep");
        let bad = handle(&reg, &ctx, &call("pin", json!({}))).await.unwrap();
        assert_eq!(
            bad["result"]["isError"], true,
            "tool errors come back as isError, not a crash"
        );
    }
}
