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
    pub presence_penalty: Option<f32>,
    /// Qwen3.8-style thinking control sent as `chat_template_kwargs`: `xhigh`, `medium`, `low` or `none` (thinking off).
    pub reasoning_effort: Option<String>,
    /// Keep thinking blocks of earlier turns in the prompt (Qwen3.8 does by default; helps long agent runs).
    pub preserve_thinking: Option<bool>,
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
    /// Prompt flavour: `compact` (short, rule-first, few-shot), `full`, or `qwen38` (for Qwen3.8 thinking models).
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
            presence_penalty: None,
            reasoning_effort: None,
            preserve_thinking: None,
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

impl ModelProfile {
    /// Output tokens to ask for. Never more than half the window: with a 4k window and `max_tokens` 4096 there is no room for the prompt.
    pub fn out_tokens(&self) -> u32 {
        self.max_tokens.min(self.context_window / 2).max(128)
    }
    /// Prompt tokens to stay under before compacting, leaving room for the answer.
    pub fn input_budget(&self) -> u64 {
        let w = u64::from(self.context_window);
        let room = w.saturating_sub(u64::from(self.out_tokens()) + 256);
        room.min(w * 3 / 4).max(w / 4)
    }
    /// Bytes of one tool result kept in context: at most the configured limit and about an eighth of the window.
    pub fn result_chars(&self) -> usize {
        self.tool_output_limit
            .min(self.context_window as usize * 33 / 80)
            .max(600)
    }
}

/// Qwen3.8 family: `qwen3.8-27b`, `Qwen3.8-Flash-Next`, `qwen3_8-…`, `qwen38…`.
pub fn is_qwen38(model: &str) -> bool {
    let m = model.to_lowercase();
    m.contains("qwen3.8") || m.contains("qwen3_8") || m.contains("qwen38") || m.contains("qwen-3.8")
}

impl ModelProfile {
    /// Qwen3.8 non-thinking mode: the model card asks for different sampling when thinking is off.
    pub fn non_thinking(&mut self) {
        self.reasoning_effort = Some("none".into());
        self.temperature = Some(0.7);
        self.top_p = Some(0.8);
        self.presence_penalty = Some(1.5);
        self.max_tokens = self.max_tokens.min(4096);
    }
}

/// Built-in profile for a model id such as `deepseek-chat`, `Qwen/Qwen3-Coder-30B`, `qwen3:14b`.
pub fn profile_for(model: &str) -> ModelProfile {
    let m = model.to_lowercase();
    let mut p = ModelProfile::default();
    if is_qwen38(model) {
        // Sampling from the Qwen3.8 model cards (thinking mode). Thinking tokens count against `max_tokens`.
        p.family = "qwen38".into();
        p.prompt_style = "qwen38".into();
        p.temperature = Some(1.0);
        p.top_p = Some(0.95);
        p.top_k = Some(20);
        p.presence_penalty = Some(0.0);
        p.reasoning_effort = Some("medium".into());
        p.preserve_thinking = Some(true);
        p.parallel_tool_calls = true;
        p.context_window = 65_536;
        p.max_tokens = 16_384;
        p.tool_output_limit = 16_000;
        if m.contains("flash") {
            p.tool_output_limit = 12_000;
        }
    } else if m.contains("deepseek") {
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

    #[test]
    fn qwen38_gets_the_model_card_sampling_and_its_own_prompt() {
        for id in ["qwen3.8-27b", "Qwen/Qwen3.8-Flash-Next", "qwen38-27b-q4"] {
            let p = profile_for(id);
            assert_eq!(
                (
                    p.family.as_str(),
                    p.prompt_style.as_str(),
                    p.temperature,
                    p.top_p,
                    p.top_k
                ),
                ("qwen38", "qwen38", Some(1.0), Some(0.95), Some(20)),
                "{id}"
            );
            assert_eq!(p.presence_penalty, Some(0.0));
            assert_eq!(p.preserve_thinking, Some(true));
        }
        assert_eq!(profile_for("qwen3-coder").family, "qwen");
        let mut p = profile_for("qwen3.8-27b");
        p.non_thinking();
        assert_eq!(
            (p.temperature, p.top_p, p.presence_penalty),
            (Some(0.7), Some(0.8), Some(1.5))
        );
        assert_eq!(p.reasoning_effort.as_deref(), Some("none"));
    }

    #[test]
    fn small_windows_get_room_for_the_prompt() {
        let p = ModelProfile {
            context_window: 4096,
            max_tokens: 4096,
            ..Default::default()
        };
        assert_eq!(p.out_tokens(), 2048);
        assert!(p.input_budget() <= 4096 - 2048 && p.input_budget() >= 1024);
        assert!(p.result_chars() <= 1700 && p.result_chars() >= 600);
        let big = ModelProfile::default();
        assert_eq!(big.out_tokens(), 4096);
        assert_eq!(big.result_chars(), 12_000);
        assert_eq!(big.input_budget(), 24_576);
    }
}
