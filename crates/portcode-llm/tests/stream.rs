//! End-to-end against an in-process fake OpenAI-compatible server (no network, no real model).
use portcode_llm::*;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn serve(responses: Vec<(u16, String)>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        for (status, body) in responses {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = vec![0u8; 65536];
            let _ = s.read(&mut buf).await;
            let head = format!("HTTP/1.1 {status} X\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len());
            s.write_all(head.as_bytes()).await.unwrap();
            s.write_all(body.as_bytes()).await.unwrap();
        }
    });
    format!("http://{addr}/v1")
}

fn sse(chunks: &[serde_json::Value]) -> String {
    chunks
        .iter()
        .map(|c| format!("data: {c}\n\n"))
        .collect::<String>()
        + "data: [DONE]\n\n"
}
fn client(url: String) -> Client {
    Client::new(ProviderConfig {
        name: "t".into(),
        base_url: url,
        api_key: Some("sk-test".into()),
        api_key_env: None,
        headers: Default::default(),
    })
    .unwrap()
}
fn tool() -> Vec<ToolDef> {
    vec![ToolDef::new(
        "read",
        "read a file",
        json!({"type":"object","properties":{"path":{"type":"string"}}}),
    )]
}

#[tokio::test]
async fn streams_text_reasoning_and_usage() {
    let body = sse(&[
        json!({"choices":[{"delta":{"reasoning_content":"think "}}]}),
        json!({"choices":[{"delta":{"content":"Hel"}}]}),
        json!({"choices":[{"delta":{"content":"lo"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":2}}),
    ]);
    let c = client(serve(vec![(200, body)]).await);
    let mut seen = vec![];
    let a = c
        .chat(
            "m",
            &profile_for("m"),
            &[Message::user("hi")],
            &[],
            &mut |e| seen.push(e),
        )
        .await
        .unwrap();
    assert_eq!((a.text.as_str(), a.reasoning.as_str()), ("Hello", "think "));
    assert_eq!(
        a.usage,
        Some(Usage {
            prompt_tokens: 5,
            completion_tokens: 2
        })
    );
    assert!(seen.contains(&StreamEvent::Text("Hel".into())));
}

#[tokio::test]
async fn assembles_fragmented_tool_calls_and_repairs_json() {
    let body = sse(&[
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"read","arguments":""}}]}}]}),
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\": \"a."}}]}}]}),
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"rs"}}]},"finish_reason":"tool_calls"}]}),
    ]);
    let c = client(serve(vec![(200, body)]).await);
    let a = c
        .chat(
            "m",
            &profile_for("m"),
            &[Message::user("x")],
            &tool(),
            &mut |_| {},
        )
        .await
        .unwrap();
    assert_eq!(a.tool_calls.len(), 1);
    assert_eq!(a.tool_calls[0].function.arguments, "{\"path\": \"a.rs\"}");
}

#[tokio::test]
async fn promotes_text_tool_calls_from_qwen_style_output() {
    let body = sse(&[
        json!({"choices":[{"delta":{"content":"<think>plan</think>ok <tool_call>{\"name\":\"read\",\"arguments\":{\"path\":\"b.rs\"}}</tool_call>"},"finish_reason":"stop"}]}),
    ]);
    let c = client(serve(vec![(200, body)]).await);
    let a = c
        .chat(
            "qwen3",
            &profile_for("qwen3"),
            &[Message::user("x")],
            &tool(),
            &mut |_| {},
        )
        .await
        .unwrap();
    assert_eq!(
        a.tool_calls.first().map(|t| t.function.name.as_str()),
        Some("read")
    );
    assert_eq!(a.text, "ok");
}

#[tokio::test]
async fn retries_plain_when_server_rejects_optional_fields() {
    let ok = sse(&[json!({"choices":[{"delta":{"content":"fine"},"finish_reason":"stop"}]})]);
    let c = client(
        serve(vec![
            (400, "{\"error\":\"unknown field stream_options\"}".into()),
            (200, ok),
        ])
        .await,
    );
    let a = c
        .chat(
            "m",
            &profile_for("m"),
            &[Message::user("x")],
            &[],
            &mut |_| {},
        )
        .await
        .unwrap();
    assert_eq!(a.text, "fine");
}

#[tokio::test]
async fn auth_error_names_the_fix() {
    let c = client(serve(vec![(401, "no".into())]).await);
    let e = c
        .chat(
            "m",
            &profile_for("m"),
            &[Message::user("x")],
            &[],
            &mut |_| {},
        )
        .await
        .unwrap_err();
    assert!(matches!(e, LlmError::Auth { .. }), "{e}");
}

#[tokio::test]
async fn connection_refused_is_an_error_not_a_panic() {
    let c = client("http://127.0.0.1:1/v1".into());
    assert!(matches!(
        c.chat(
            "m",
            &profile_for("m"),
            &[Message::user("x")],
            &[],
            &mut |_| {}
        )
        .await,
        Err(LlmError::Connect { .. })
    ));
}
