//! 修复多轮 tool 历史，避免上游报 insufficient tool messages。

use std::collections::HashMap;

use serde_json::{json, Value};
use tracing::debug;

use crate::config::ProviderConfig;
use crate::provider::chat_reasoning::provider_needs_reasoning_content;

const REASONING_PLACEHOLDER: &str = "tool call";

#[derive(Debug, Clone, Copy, Default)]
pub struct RepairMessagesOptions {
    pub preserve_reasoning_content: bool,
    pub tool_output_max_chars: usize,
}

pub fn repair_options_for_provider(
    provider: Option<&ProviderConfig>,
    tool_output_max_chars: usize,
) -> RepairMessagesOptions {
    RepairMessagesOptions {
        preserve_reasoning_content: provider
            .map(provider_needs_reasoning_content)
            .unwrap_or(false),
        tool_output_max_chars,
    }
}

#[allow(dead_code)]
pub fn repair_messages_for_upstream(messages: &mut Vec<Value>) {
    repair_messages_for_upstream_with_options(messages, RepairMessagesOptions::default());
}

pub fn repair_messages_for_upstream_with_options(
    messages: &mut Vec<Value>,
    options: RepairMessagesOptions,
) {
    repair_chat_tool_message_sequence(messages);
    backfill_missing_tool_responses(messages);
    normalize_assistant_tool_call_content(messages);
    if options.preserve_reasoning_content {
        backfill_tool_call_reasoning_placeholders(messages);
    }
    if options.tool_output_max_chars > 0 {
        truncate_large_tool_outputs(messages, options.tool_output_max_chars);
    }
    let collapsed = collapse_system_messages_to_head(std::mem::take(messages));
    messages.extend(collapsed);
}

pub fn truncate_large_tool_outputs(messages: &mut [Value], max_chars: usize) {
    if max_chars == 0 {
        return;
    }
    for message in messages.iter_mut() {
        if message.get("role").and_then(|v| v.as_str()) != Some("tool") {
            continue;
        }
        let Some(obj) = message.as_object_mut() else {
            continue;
        };
        let content = tool_message_content_as_str(obj.get("content"));
        if content.is_empty() {
            continue;
        }
        let truncated = truncate_text_head_tail(&content, max_chars);
        if truncated != content {
            let original_chars = content.chars().count();
            let truncated_chars = truncated.chars().count();
            debug!(
                original_chars,
                truncated_chars,
                max_chars,
                "truncated upstream tool output"
            );
            obj.insert("content".into(), Value::String(truncated));
        }
    }
}

pub fn truncate_text_head_tail(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return text.to_string();
    }
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }

    const MARKER_RESERVE: usize = 48;
    let content_budget = max_chars.saturating_sub(MARKER_RESERVE).max(32);
    let head_chars = content_budget / 2;
    let tail_chars = content_budget - head_chars;
    let omitted = total_chars.saturating_sub(head_chars + tail_chars);
    let marker = format!("\n\n[... truncated {omitted} chars ...]\n\n");
    let head: String = text.chars().take(head_chars).collect();
    let tail: String = text.chars().skip(total_chars - tail_chars).collect();
    format!("{head}{marker}{tail}")
}

pub fn finalize_chat_request(chat: &mut Value, stream: bool) {
    let has_tools = chat
        .get("tools")
        .and_then(|v| v.as_array())
        .is_some_and(|tools| !tools.is_empty());
    if !has_tools {
        if let Some(obj) = chat.as_object_mut() {
            obj.remove("tool_choice");
            obj.remove("parallel_tool_calls");
        }
    }

    if stream {
        match chat.get_mut("stream_options") {
            Some(Value::Object(opts)) => {
                opts.insert("include_usage".into(), json!(true));
            }
            _ => {
                chat["stream_options"] = json!({ "include_usage": true });
            }
        }
    }
}

fn collapse_system_messages_to_head(messages: Vec<Value>) -> Vec<Value> {
    let mut system_chunks: Vec<String> = Vec::new();
    let mut rest: Vec<Value> = Vec::with_capacity(messages.len());

    for msg in messages {
        if msg.get("role").and_then(|v| v.as_str()) == Some("system") {
            if let Some(text) = msg.get("content").and_then(|v| v.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    system_chunks.push(text.to_string());
                }
                continue;
            }
        }
        rest.push(msg);
    }

    let mut out: Vec<Value> = Vec::with_capacity(rest.len() + 1);
    if !system_chunks.is_empty() {
        out.push(json!({
            "role": "system",
            "content": system_chunks.join("\n\n"),
        }));
    }
    out.extend(rest);
    out
}

fn normalize_assistant_tool_call_content(messages: &mut [Value]) {
    for message in messages.iter_mut() {
        if !assistant_has_tool_calls(message) {
            continue;
        }
        let Some(obj) = message.as_object_mut() else {
            continue;
        };
        let is_nullish = obj.get("content").is_none_or(|value| value.is_null());
        if is_nullish {
            obj.insert("content".into(), Value::String(String::new()));
        }
    }
}

fn repair_chat_tool_message_sequence(messages: &mut Vec<Value>) {
    let mut i = 0;
    while i < messages.len() {
        if !assistant_has_tool_calls(&messages[i]) {
            i += 1;
            continue;
        }

        let mut merged = tool_calls_from(&messages[i]);
        let mut j = i + 1;
        while j < messages.len() && assistant_has_tool_calls(&messages[j]) {
            merged.extend(tool_calls_from(&messages[j]));
            j += 1;
        }

        if j > i + 1 {
            let extra_content: Vec<String> = (i + 1..j)
                .filter_map(|k| messages[k].get("content").and_then(|v| v.as_str()))
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .collect();
            if let Some(obj) = messages[i].as_object_mut() {
                obj.insert("tool_calls".into(), Value::Array(merged));
                if !extra_content.is_empty() {
                    let base = obj
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let combined = if base.is_empty() {
                        extra_content.join("\n")
                    } else {
                        format!("{base}\n{}", extra_content.join("\n"))
                    };
                    obj.insert("content".into(), Value::String(combined));
                }
            }
            messages.drain(i + 1..j);
        }

        i += 1;
    }
}

fn tool_message_content_as_str(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(other) => serde_json::to_string(other).unwrap_or_default(),
        None => String::new(),
    }
}

fn assistant_has_tool_calls(message: &Value) -> bool {
    message.get("role").and_then(|v| v.as_str()) == Some("assistant")
        && message
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .is_some_and(|calls| !calls.is_empty())
}

fn tool_calls_from(message: &Value) -> Vec<Value> {
    message
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

fn tool_call_ids_from(message: &Value) -> Vec<String> {
    tool_calls_from(message)
        .iter()
        .filter_map(|call| call.get("id").and_then(|v| v.as_str()))
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

fn backfill_missing_tool_responses(messages: &mut Vec<Value>) {
    let mut i = 0;
    while i < messages.len() {
        if !assistant_has_tool_calls(&messages[i]) {
            i += 1;
            continue;
        }

        let expected_ids = tool_call_ids_from(&messages[i]);
        if expected_ids.is_empty() {
            i += 1;
            continue;
        }

        let tool_start = i + 1;
        let mut tool_end = tool_start;
        while tool_end < messages.len()
            && messages[tool_end].get("role").and_then(|v| v.as_str()) == Some("tool")
        {
            tool_end += 1;
        }

        let existing: HashMap<String, Value> = messages[tool_start..tool_end]
            .iter()
            .filter_map(|msg| {
                let id = msg.get("tool_call_id").and_then(|v| v.as_str())?;
                if id.is_empty() {
                    return None;
                }
                Some((id.to_string(), msg.clone()))
            })
            .collect();

        let rebuilt: Vec<Value> = expected_ids
            .iter()
            .map(|id| {
                existing.get(id).cloned().unwrap_or_else(|| {
                    json!({
                        "role": "tool",
                        "tool_call_id": id,
                        "content": "",
                    })
                })
            })
            .collect();

        let needs_rebuild = rebuilt.len() != tool_end - tool_start
            || rebuilt
                .iter()
                .zip(&messages[tool_start..tool_end])
                .any(|(expected, actual)| expected != actual);
        if needs_rebuild {
            messages.splice(tool_start..tool_end, rebuilt);
        }

        i = tool_start + expected_ids.len();
    }
}

fn backfill_tool_call_reasoning_placeholders(messages: &mut [Value]) {
    for index in 0..messages.len() {
        if !assistant_has_tool_calls(&messages[index]) {
            continue;
        }
        if message_has_reasoning_content(&messages[index]) {
            continue;
        }
        let reasoning = find_reasoning_for_tool_call_message(messages, index)
            .unwrap_or_else(|| REASONING_PLACEHOLDER.to_string());
        if let Some(obj) = messages[index].as_object_mut() {
            obj.insert("reasoning_content".into(), Value::String(reasoning));
        }
    }
}

fn message_has_reasoning_content(message: &Value) -> bool {
    message
        .get("reasoning_content")
        .and_then(|v| v.as_str())
        .is_some_and(|text| !text.trim().is_empty())
}

fn find_reasoning_for_tool_call_message(messages: &[Value], index: usize) -> Option<String> {
    for i in (0..index).rev() {
        if assistant_has_tool_calls(&messages[i]) {
            break;
        }
        if let Some(text) = messages[i]
            .get("reasoning_content")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(text.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_consecutive_assistant_tool_calls() {
        let mut messages = vec![
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "a", "arguments": "{}"}}]
            }),
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "call_2", "type": "function", "function": {"name": "b", "arguments": "{}"}}]
            }),
            json!({"role": "tool", "tool_call_id": "call_1", "content": "1"}),
            json!({"role": "tool", "tool_call_id": "call_2", "content": "2"}),
        ];
        repair_messages_for_upstream(&mut messages);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["tool_calls"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn backfills_missing_tool_responses() {
        let mut messages = vec![
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    {"id": "call_1", "type": "function", "function": {"name": "a", "arguments": "{}"}},
                    {"id": "call_2", "type": "function", "function": {"name": "b", "arguments": "{}"}}
                ]
            }),
            json!({"role": "tool", "tool_call_id": "call_1", "content": "ok"}),
        ];
        repair_messages_for_upstream(&mut messages);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2]["tool_call_id"], "call_2");
        assert_eq!(messages[2]["content"], "");
    }

    #[test]
    fn merges_assistant_content_when_collapsing_tool_calls() {
        let mut messages = vec![
            json!({
                "role": "assistant",
                "content": "first",
                "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "a", "arguments": "{}"}}]
            }),
            json!({
                "role": "assistant",
                "content": "second",
                "tool_calls": [{"id": "call_2", "type": "function", "function": {"name": "b", "arguments": "{}"}}]
            }),
            json!({"role": "tool", "tool_call_id": "call_1", "content": "1"}),
            json!({"role": "tool", "tool_call_id": "call_2", "content": "2"}),
        ];
        repair_messages_for_upstream(&mut messages);
        assert_eq!(messages[0]["content"], "first\nsecond");
        assert_eq!(messages[0]["tool_calls"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn truncates_non_string_tool_output_content() {
        let mut messages = vec![json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": {"stdout": "x".repeat(200)}
        })];
        truncate_large_tool_outputs(&mut messages, 80);
        let content = messages[0]["content"].as_str().unwrap();
        assert!(content.contains("truncated"));
        assert!(content.chars().count() <= 80);
    }

    #[test]
    fn backfill_inherits_reasoning_from_earlier_assistant_message() {
        let mut messages = vec![
            json!({"role": "assistant", "content": "", "reasoning_content": "Plan the patch."}),
            json!({"role": "user", "content": "apply it"}),
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "apply_patch", "arguments": "{}"}}]
            }),
            json!({"role": "tool", "tool_call_id": "call_1", "content": "ok"}),
        ];
        repair_messages_for_upstream_with_options(
            &mut messages,
            RepairMessagesOptions {
                preserve_reasoning_content: true,
                tool_output_max_chars: 0,
            },
        );
        assert_eq!(messages[2]["reasoning_content"], "Plan the patch.");
    }

    #[test]
    fn leaves_tool_output_intact_when_truncation_disabled() {
        let long_output = "a".repeat(500);
        let mut messages = vec![
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "grep", "arguments": "{}"}}]
            }),
            json!({"role": "tool", "tool_call_id": "call_1", "content": long_output.clone()}),
        ];
        repair_messages_for_upstream_with_options(
            &mut messages,
            RepairMessagesOptions {
                preserve_reasoning_content: false,
                tool_output_max_chars: 0,
            },
        );
        assert_eq!(messages[1]["content"], long_output);
    }

    #[test]
    fn backfills_tool_responses_when_assistant_tool_calls_trail_history() {
        let mut messages = vec![
            json!({"role": "user", "content": "run tools"}),
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    {"id": "call_1", "type": "function", "function": {"name": "a", "arguments": "{}"}},
                    {"id": "call_2", "type": "function", "function": {"name": "b", "arguments": "{}"}}
                ]
            }),
        ];
        repair_messages_for_upstream(&mut messages);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2]["tool_call_id"], "call_1");
        assert_eq!(messages[3]["tool_call_id"], "call_2");
    }

    #[test]
    fn injects_stream_options_include_usage_for_streaming_requests() {
        let mut chat = json!({"model": "qwen3.7-max", "messages": [], "stream": true});
        finalize_chat_request(&mut chat, true);
        assert_eq!(chat["stream_options"]["include_usage"], true);
    }

    #[test]
    fn strips_tool_choice_when_tools_are_absent() {
        let mut chat = json!({
            "model": "qwen3.7-max",
            "tool_choice": "auto",
            "parallel_tool_calls": true,
            "messages": [{"role": "user", "content": "hi"}]
        });
        finalize_chat_request(&mut chat, false);
        assert!(chat.get("tool_choice").is_none());
        assert!(chat.get("parallel_tool_calls").is_none());
    }

    #[test]
    fn truncates_large_string_tool_output_when_enabled() {
        let long_output = "HEAD".to_string() + &"x".repeat(200) + "TAIL";
        let mut messages = vec![
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "grep", "arguments": "{}"}}]
            }),
            json!({"role": "tool", "tool_call_id": "call_1", "content": long_output.clone()}),
        ];
        truncate_large_tool_outputs(&mut messages, 80);
        let content = messages[1]["content"].as_str().unwrap();
        assert!(content.contains("HEAD"));
        assert!(content.contains("TAIL"));
        assert!(content.contains("truncated"));
        assert!(content.chars().count() < long_output.chars().count());
    }

    #[test]
    fn collapses_mid_stream_system_messages_to_head() {
        let mut messages = vec![
            json!({"role": "system", "content": "base"}),
            json!({"role": "user", "content": "hi"}),
            json!({"role": "system", "content": "reminder"}),
            json!({"role": "assistant", "content": "ok"}),
        ];
        repair_messages_for_upstream(&mut messages);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["content"], "base\n\nreminder");
    }
}
