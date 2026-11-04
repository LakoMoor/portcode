use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FunctionCall {
    pub name: String,
    /// JSON text, always valid after the client's repair pass.
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "func")]
    pub kind: String,
    pub function: FunctionCall,
}
fn func() -> String {
    "function".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Kept in our session files for display; sent back to the server only if the model profile asks for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

impl Message {
    pub fn system(t: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Some(t.into()),
            tool_calls: vec![],
            tool_call_id: None,
            reasoning_content: None,
        }
    }
    pub fn user(t: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Some(t.into()),
            tool_calls: vec![],
            tool_call_id: None,
            reasoning_content: None,
        }
    }
    pub fn tool(id: impl Into<String>, t: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(t.into()),
            tool_calls: vec![],
            tool_call_id: Some(id.into()),
            reasoning_content: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: ToolFn,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolFn {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}
impl ToolDef {
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: Value) -> Self {
        Self {
            kind: "function",
            function: ToolFn {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

/// A finished assistant turn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Assistant {
    pub text: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
    pub usage: Option<Usage>,
}

impl Assistant {
    pub fn into_message(self) -> Message {
        Message {
            role: Role::Assistant,
            content: if self.text.is_empty() && !self.tool_calls.is_empty() {
                None
            } else {
                Some(self.text)
            },
            tool_calls: self.tool_calls,
            tool_call_id: None,
            reasoning_content: if self.reasoning.is_empty() {
                None
            } else {
                Some(self.reasoning)
            },
        }
    }
}
