//! The context window is read from what the server reports, not guessed from the model name.
use portcode_llm::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Answers every request by path: `/v1/models` gets `models_body`, anything else 404.
async fn serve(models_body: &'static str) -> String {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                return;
            };
            let mut buf = vec![0u8; 8192];
            let n = s.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let (status, body) = if req.starts_with("GET /v1/models") {
                (200, models_body)
            } else {
                (404, "{}")
            };
            let head = format!("HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len());
            let _ = s.write_all(head.as_bytes()).await;
            let _ = s.write_all(body.as_bytes()).await;
        }
    });
    format!("http://{addr}/v1")
}

fn client(url: String) -> Client {
    Client::new(ProviderConfig {
        name: "t".into(),
        base_url: url,
        api_key: None,
        api_key_env: None,
        headers: Default::default(),
    })
    .unwrap()
}

#[tokio::test]
async fn reads_vllm_and_openrouter_style_fields() {
    let u =
        serve(r#"{"data":[{"id":"a","max_model_len":131072},{"id":"b","context_length":65536}]}"#)
            .await;
    let c = client(u);
    assert_eq!(c.context_window("a").await, Some(131_072));
    assert_eq!(c.context_window("b").await, Some(65_536));
    assert_eq!(c.context_window("missing").await, None);
}

#[tokio::test]
async fn a_server_that_says_nothing_gives_none_and_absurd_numbers_are_ignored() {
    let u = serve(r#"{"data":[{"id":"a"},{"id":"z","context_length":12}]}"#).await;
    let c = client(u);
    assert_eq!(c.context_window("a").await, None);
    assert_eq!(c.context_window("z").await, None);
}
