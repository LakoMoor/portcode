//! L1 — OpenAI-compatible chat client built for self-hosted and open-weight models
//! (lakomoor.com/v1, Ollama, LM Studio, llama.cpp, vLLM, SGLang…).
//!
//! Open models are less uniform than hosted ones, so this crate absorbs the differences:
//! `reasoning_content` and inline `<think>` blocks, tool calls emitted as text (`<tool_call>` / fenced JSON),
//! half-valid tool-argument JSON, servers that reject optional sampling fields, and idle streams.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

mod client;
mod profile;
mod repair;
mod types;

pub use client::{is_cli, presets, Client, LlmError, ProviderConfig, StreamEvent};
pub use profile::{profile_for, ModelProfile};
pub use repair::{parse_text_tool_calls, repair_json, strip_think};
pub use types::{Assistant, FunctionCall, Message, Role, ToolCall, ToolDef, Usage};
