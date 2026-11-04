use crate::types::{FunctionCall, ToolCall};
use serde_json::Value;

/// Remove `<think>…</think>` blocks (and an unclosed trailing `<think>`), trimming the result.
pub fn strip_think(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("<think>") {
        out.push_str(rest.get(..i).unwrap_or(""));
        match rest.get(i..).and_then(|r| r.find("</think>")) {
            Some(j) => rest = rest.get(i + j + "</think>".len()..).unwrap_or(""),
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Turn what an open model *meant* as JSON into valid JSON: strips code fences, drops trailing commas,
/// closes unbalanced strings/brackets, and maps empty input to `{}`. Returns `None` if it still won't parse.
pub fn repair_json(raw: &str) -> Option<String> {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return Some("{}".into());
    }
    if serde_json::from_str::<Value>(&s).is_ok() {
        return Some(s);
    }
    if let Some(r) = s.strip_prefix("```json").or_else(|| s.strip_prefix("```")) {
        s = r.trim_end_matches("```").trim().to_string();
    }
    // walk once to find open structures
    let (mut stack, mut in_str, mut esc) = (Vec::new(), false, false);
    for c in s.chars() {
        if in_str {
            if esc {
                esc = false
            } else if c == '\\' {
                esc = true
            } else if c == '"' {
                in_str = false
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' => {
                stack.pop();
            }
            _ => {}
        }
    }
    if in_str {
        s.push('"');
    }
    let mut t = s.trim_end().to_string();
    while t.ends_with(',') || t.ends_with(':') {
        if t.ends_with(':') {
            t.push_str("null");
            break;
        }
        t.pop();
        t = t.trim_end().to_string();
    }
    while let Some(c) = stack.pop() {
        t.push(c);
    }
    let t = t.replace(",}", "}").replace(",]", "]");
    serde_json::from_str::<Value>(&t).ok().map(|_| t)
}

/// Extract tool calls a model wrote as text: Hermes/Qwen `<tool_call>{"name":…,"arguments":{…}}</tool_call>`
/// and fenced ```json blocks with `name` + `arguments`. Returns (calls, text with those blocks removed).
pub fn parse_text_tool_calls(text: &str, known: &[&str]) -> (Vec<ToolCall>, String) {
    let mut calls = Vec::new();
    let mut remaining = String::new();
    let mut n = 0;
    let mut push = |v: &Value, calls: &mut Vec<ToolCall>| -> bool {
        let name = v.get("name").and_then(Value::as_str).unwrap_or("");
        if !known.contains(&name) {
            return false;
        }
        let args = v
            .get("arguments")
            .or_else(|| v.get("parameters"))
            .cloned()
            .unwrap_or(Value::Object(Default::default()));
        let args = match args {
            Value::String(s) => repair_json(&s).unwrap_or_else(|| "{}".into()),
            other => other.to_string(),
        };
        n += 1;
        calls.push(ToolCall {
            id: format!("text_call_{n}"),
            kind: "function".into(),
            function: FunctionCall {
                name: name.into(),
                arguments: args,
            },
        });
        true
    };
    for (open, close) in [("<tool_call>", "</tool_call>"), ("```json", "```")] {
        remaining.clear();
        let mut rest = text;
        let mut buf = String::new();
        while let Some(i) = rest.find(open) {
            buf.push_str(rest.get(..i).unwrap_or(""));
            let after = rest.get(i + open.len()..).unwrap_or("");
            let (body, tail) = match after.find(close) {
                Some(j) => (
                    after.get(..j).unwrap_or(""),
                    after.get(j + close.len()..).unwrap_or(""),
                ),
                None => (after, ""),
            };
            let parsed = repair_json(body).and_then(|j| serde_json::from_str::<Value>(&j).ok());
            let used = parsed
                .as_ref()
                .map(|v| push(v, &mut calls))
                .unwrap_or(false);
            if !used {
                buf.push_str(open);
                buf.push_str(body);
                buf.push_str(close);
            }
            rest = tail;
        }
        buf.push_str(rest);
        remaining = buf;
        if open == "<tool_call>" && !calls.is_empty() {
            break;
        }
    }
    (calls, remaining.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_think_blocks() {
        assert_eq!(strip_think("<think>hmm</think>\nanswer"), "answer");
        assert_eq!(strip_think("a <think>unclosed"), "a");
        assert_eq!(strip_think("plain"), "plain");
    }
    #[test]
    fn repairs_common_breakage() {
        assert_eq!(repair_json("").as_deref(), Some("{}"));
        assert_eq!(repair_json("{\"a\": 1,}").as_deref(), Some("{\"a\": 1}"));
        assert_eq!(
            repair_json("{\"path\": \"src/main.rs").as_deref(),
            Some("{\"path\": \"src/main.rs\"}")
        );
        assert_eq!(
            repair_json("```json\n{\"a\":[1,2\n```").as_deref(),
            Some("{\"a\":[1,2]}")
        );
        assert!(repair_json("not json at all").is_none());
    }
    #[test]
    fn parses_hermes_tool_calls() {
        let t = "I'll look.\n<tool_call>\n{\"name\": \"read\", \"arguments\": {\"path\": \"a.rs\"}}\n</tool_call>";
        let (c, rest) = parse_text_tool_calls(t, &["read"]);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].function.name, "read");
        assert_eq!(c[0].function.arguments, "{\"path\":\"a.rs\"}");
        assert_eq!(rest, "I'll look.");
    }
    #[test]
    fn ignores_unknown_tools_and_plain_code() {
        let t = "```json\n{\"name\": \"rm_rf\", \"arguments\": {}}\n```";
        let (c, rest) = parse_text_tool_calls(t, &["read"]);
        assert!(c.is_empty());
        assert!(rest.contains("rm_rf"));
    }
}
