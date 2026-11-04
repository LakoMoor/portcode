//! Per-family behaviour for open-weight models. Matched by substring on the model id; every field is
//! overridable from `portcode.json` (`"models": { "<id>": { … } }`). Defaults are conservative starting
//! points, not benchmark claims: measure your own model with `portcode eval` before trusting a profile.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelProfile {
    pub family: String,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub max_tokens: u32,
    /// Context window in tokens; drives auto-compaction. Local servers often run smaller than the model card.
    pub context_window: u32,
    /// Ask for several tool calls per turn. Small/fast models do better with one.
    pub parallel_tool_calls: bool,
    /// Send `reasoning_content` back in history. Hosted APIs differ here; default off.
    pub echo_reasoning: bool,
    /// Parse `<tool_call>…</tool_call>` / fenced JSON out of plain text when the server returned no tool_calls.
    pub text_tool_fallback: bool,
    /// Remove inline `<think>…</think>` from displayed and stored text.
    pub strip_think: bool,
    /// Prompt flavour: `compact` (short, rule-first, few-shot) or `full`.
    pub prompt_style: String,
    /// Max bytes of one tool result kept in context.
    pub tool_output_limit: usize,
}

impl Default for ModelProfile {
    fn default() -> Self {
        Self {
            family: "generic".into(),
            temperature: Some(0.2),
            top_p: None,
            top_k: None,
            max_tokens: 4096,
            context_window: 32_768,
            parallel_tool_calls: false,
            echo_reasoning: false,
            text_tool_fallback: true,
            strip_think: true,
            prompt_style: "compact".into(),
            tool_output_limit: 12_000,
        }
    }
}

/// Built-in profile for a model id such as `deepseek-chat`, `Qwen/Qwen3-Coder-30B`, `qwen3:14b`.
pub fn profile_for(model: &str) -> ModelProfile {
    let m = model.to_lowercase();
    let mut p = ModelProfile::default();
    if m.contains("deepseek") {
        p.family = "deepseek".into();
        p.temperature = Some(0.0); // reasoning variants ignore it; chat variants are steadier at 0 for tool use
        p.context_window = 65_536;
        p.max_tokens = 8192;
        p.tool_output_limit = 16_000;
    } else if m.contains("qwen") {
        p.family = "qwen".into();
        p.temperature = Some(0.6);
        p.top_p = Some(0.95);
        p.top_k = Some(20);
        p.context_window = 65_536;
        p.max_tokens = 8192;
        // "flash"/"mini"/small variants: shorter outputs, one tool at a time
        if m.contains("flash")
            || m.contains("mini")
            || m.contains("7b")
            || m.contains("8b")
            || m.contains("4b")
        {
            p.max_tokens = 4096;
            p.tool_output_limit = 8_000;
            p.context_window = 32_768;
        }
    } else if m.contains("llama")
        || m.contains("mistral")
        || m.contains("gemma")
        || m.contains("glm")
        || m.contains("kimi")
    {
        p.family = "open".into();
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn families_match_by_substring() {
        assert_eq!(profile_for("deepseek-chat").family, "deepseek");
        assert_eq!(profile_for("Qwen/Qwen3-Coder").top_k, Some(20));
        let flash = profile_for("qwen-flash-next");
        assert!(
            flash.tool_output_limit < profile_for("qwen3-coder").tool_output_limit
                && !flash.parallel_tool_calls
        );
        assert_eq!(profile_for("gpt-x").family, "generic");
    }
}
