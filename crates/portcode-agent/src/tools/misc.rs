use crate::permission::Decision;
use crate::tool::*;
use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

pub struct TodoWrite;
#[async_trait]
impl Tool for TodoWrite {
    fn name(&self) -> String {
        "todowrite".into()
    }
    fn description(&self) -> String {
        "Keep the task list for multi-step work. Send the FULL list each time. Exactly one item 'in_progress'. Statuses: pending, in_progress, completed.".into()
    }
    fn example(&self) -> String {
        r#"{"todos": [{"content": "survey original", "status": "completed"}, {"content": "port parser slice", "status": "in_progress"}]}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"todos": {"type":"array","items": obj(json!({"content":{"type":"string"},"status":{"type":"string","enum":["pending","in_progress","completed"]}}), &["content","status"])}}),
            &["todos"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let todos: Vec<Todo> = serde_json::from_value(
            args.get("todos").cloned().unwrap_or(Value::Null),
        )
        .map_err(|_| {
            ToolError(format!(
                "todowrite: 'todos' must be a list of {{content,status}}. Example: {}",
                self.example()
            ))
        })?;
        if let Some(b) = todos
            .iter()
            .find(|t| !matches!(t.status.as_str(), "pending" | "in_progress" | "completed"))
        {
            return err(format!("bad status '{}'", b.status));
        }
        let open = todos.iter().filter(|t| t.status != "completed").count();
        let text = todos
            .iter()
            .map(|t| {
                format!(
                    "[{}] {}",
                    match t.status.as_str() {
                        "completed" => "x",
                        "in_progress" => "~",
                        _ => " ",
                    },
                    t.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        lock(&ctx.state).todos = todos;
        Ok(Out::new(format!("{open} open"), text))
    }
}

pub struct WebFetch;
fn html_to_text(h: &str) -> String {
    let strip = |re: &str, s: &str, with: &str| {
        Regex::new(re)
            .map(|r| r.replace_all(s, with).to_string())
            .unwrap_or_else(|_| s.to_string())
    };
    let s = strip(r"(?is)<script[^>]*>.*?</script>", h, " ");
    let s = strip(r"(?is)<style[^>]*>.*?</style>", &s, " ");
    let s = strip(r"(?is)<noscript[^>]*>.*?</noscript>", &s, " ");
    let s = strip(r"(?i)<br\s*/?>|</(p|div|li|h\d|tr)>", &s, "\n");
    let s = strip(r"(?s)<[^>]+>", &s, "");
    let s = s
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");
    strip(r"\n\s*\n+", &strip(r"[ \t]+", &s, " "), "\n\n")
        .trim()
        .to_string()
}
#[async_trait]
impl Tool for WebFetch {
    fn name(&self) -> String {
        "webfetch".into()
    }
    fn description(&self) -> String {
        "Fetch a http(s) URL and return readable text (HTML tags removed). Docs, crate pages, specs. Max ~2 MB downloaded, output clipped.".into()
    }
    fn example(&self) -> String {
        r#"{"url": "https://docs.rs/wgpu/latest/wgpu/"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(json!({"url": {"type":"string"}}), &["url"])
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "webfetch",
            example: &self.example(),
        };
        let url = a.str("url")?;
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return err("only http:// and https:// URLs are supported");
        }
        ctx.permit("webfetch", &url, Decision::Ask, "network fetch")
            .await?;
        let c = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("PortCode/0.1")
            .build()?;
        let r = c
            .get(&url)
            .send()
            .await
            .map_err(|e| ToolError(format!("fetch failed: {e}")))?;
        let status = r.status();
        let ct = r
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = r.bytes().await.map_err(|e| ToolError(e.to_string()))?;
        let body = String::from_utf8_lossy(bytes.get(..bytes.len().min(2_000_000)).unwrap_or(&[]))
            .to_string();
        let text = if ct.contains("html") {
            html_to_text(&body)
        } else {
            body
        };
        Ok(Out::new(
            url,
            format!("HTTP {status}\n{}", clip(&text, 40_000)),
        ))
    }
}

pub struct SkillTool(pub Vec<crate::skills::Skill>);
#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> String {
        "skill".into()
    }
    fn description(&self) -> String {
        format!("Load a skill's instructions, or one of its reference files. Load the skill BEFORE starting its kind of task, then load only the reference the current phase needs.\nAvailable: {}", self.0.iter().map(|s| format!("{} — {}", s.name, s.description.chars().take(110).collect::<String>())).collect::<Vec<_>>().join(" | "))
    }
    fn example(&self) -> String {
        r#"{"name": "rustport", "file": "references/evidence.md"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"name": {"type":"string"}, "file": {"type":"string","description":"optional relative path, e.g. references/evidence.md"}}),
            &["name"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "skill",
            example: &self.example(),
        };
        let n = a.str("name")?;
        let s = self.0.iter().find(|s| s.name == n).ok_or_else(|| {
            ToolError(format!(
                "no skill '{n}'. Available: {}",
                self.0
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
        lock(&ctx.state).loaded_skills.insert(n.clone());
        let f = a.opt_str("file");
        let text = s.file(f.as_deref()).map_err(ToolError)?;
        Ok(Out::new(
            format!("{n}{}", f.map(|f| format!(":{f}")).unwrap_or_default()),
            text,
        ))
    }
}

pub struct Task;
#[async_trait]
impl Tool for Task {
    fn name(&self) -> String {
        "task".into()
    }
    fn description(&self) -> String {
        "Delegate a self-contained job to a sub-agent with a fresh context and get its final answer. agent=explore (read-only search/summaries, fast) or general (can edit). Give complete instructions: it sees nothing of this conversation.".into()
    }
    fn example(&self) -> String {
        r#"{"agent": "explore", "description": "find RNG use", "prompt": "List every place that seeds or calls a random number generator in src/, with file:line and the algorithm. Answer as a bullet list."}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"agent": {"type":"string","enum":["explore","general"]}, "description": {"type":"string"}, "prompt": {"type":"string"}}),
            &["prompt"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "task",
            example: &self.example(),
        };
        let prompt = a.str("prompt")?;
        let agent = a.opt_str("agent").unwrap_or_else(|| "explore".into());
        let runner: Option<Arc<dyn SubRunner>> = lock(&ctx.sub).clone();
        let r = runner
            .ok_or_else(|| ToolError("sub-agents are not available in this session".into()))?;
        let text = r.run(&agent, &prompt).await.map_err(ToolError)?;
        Ok(Out::new(a.opt_str("description").unwrap_or(agent), text))
    }
}

pub struct Blocked;
#[async_trait]
impl Tool for Blocked {
    fn name(&self) -> String {
        "blocked".into()
    }
    fn description(&self) -> String {
        "Stop because you need a decision or input only the user can give (licence, missing file, ambiguous requirement, permission refused). The port is NOT finished; say exactly what you need. Do not use this to avoid hard work.".into()
    }
    fn example(&self) -> String {
        r#"{"reason": "inputs/ has no case for --min-len 0; the original crashes. Should the port crash too or return an error?"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(json!({"reason": {"type":"string"}}), &["reason"])
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "blocked",
            example: &self.example(),
        };
        let r = a.str("reason")?;
        lock(&ctx.state).blocked = Some(r.clone());
        Ok(Out::new(
            "blocked",
            format!("Stopped, waiting for the user: {r}"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_is_reduced_to_text() {
        let t = html_to_text("<html><style>x{}</style><body><h1>Hi</h1><p>a &amp; b</p><script>evil()</script></body></html>");
        assert!(t.contains("Hi") && t.contains("a & b") && !t.contains("evil") && !t.contains("<"));
    }
}
