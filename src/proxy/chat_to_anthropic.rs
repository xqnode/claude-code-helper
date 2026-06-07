use std::collections::HashMap;

use serde_json::{json, Value};

pub fn convert_chat_json_to_anthropic(bytes: &[u8]) -> anyhow::Result<String> {
    let value: Value = serde_json::from_slice(bytes)?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("msg_helper");
    let model = value
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("claude-sonnet-4");
    let choice = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|choices| choices.first())
        .cloned()
        .unwrap_or(json!({}));
    let message = choice.get("message").cloned().unwrap_or(json!({}));
    let stop_reason = map_finish_reason(choice.get("finish_reason").and_then(|v| v.as_str()));

    let content = build_anthropic_content(&message);

    let usage = value.get("usage").cloned().unwrap_or_else(|| {
        json!({
            "input_tokens": 0,
            "output_tokens": 0,
        })
    });

    let response = json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {
            "input_tokens": usage.get("prompt_tokens").cloned().unwrap_or(json!(0)),
            "output_tokens": usage.get("completion_tokens").cloned().unwrap_or(json!(0)),
        }
    });
    Ok(serde_json::to_string(&response)?)
}

fn build_anthropic_content(message: &Value) -> Vec<Value> {
    let mut content = Vec::new();

    if let Some(reasoning) = message
        .get("reasoning_content")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        content.push(json!({
            "type": "thinking",
            "thinking": reasoning,
        }));
    }

    if let Some(text) = message.get("content").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
    }

    if let Some(tool_calls) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for call in tool_calls {
            let args = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let input: Value = serde_json::from_str(args).unwrap_or(json!({}));
            content.push(json!({
                "type": "tool_use",
                "id": call.get("id").cloned().unwrap_or(Value::Null),
                "name": call.get("function").and_then(|f| f.get("name")).cloned().unwrap_or(Value::Null),
                "input": input,
            }));
        }
    }

    content
}

fn map_finish_reason(finish_reason: Option<&str>) -> &'static str {
    match finish_reason {
        Some("tool_calls") => "tool_use",
        Some("length") => "max_tokens",
        _ => "end_turn",
    }
}

pub struct AnthropicSseTranslator {
    message_id: String,
    next_block_index: u32,
    thinking_block_index: Option<u32>,
    thinking_block_open: bool,
    thinking_block_stopped: bool,
    text_block_index: Option<u32>,
    text_block_open: bool,
    text_block_stopped: bool,
    tool_calls: HashMap<u32, ToolCallBuilder>,
    tool_block_indices: HashMap<u32, u32>,
    finish_emitted: bool,
}

#[derive(Debug, Default)]
struct ToolCallBuilder {
    id: String,
    name: String,
    arguments: String,
    block_started: bool,
    block_stopped: bool,
}

impl AnthropicSseTranslator {
    pub fn new(message_id: &str) -> Self {
        Self {
            message_id: message_id.to_string(),
            next_block_index: 0,
            thinking_block_index: None,
            thinking_block_open: false,
            thinking_block_stopped: false,
            text_block_index: None,
            text_block_open: false,
            text_block_stopped: false,
            tool_calls: HashMap::new(),
            tool_block_indices: HashMap::new(),
            finish_emitted: false,
        }
    }

    pub fn convert_event(&mut self, chunk: &str) -> Vec<String> {
        let data_line = match chunk
            .lines()
            .find_map(|line| super::sse::strip_sse_field(line, "data"))
            .map(str::trim)
        {
            Some(line) => line,
            None => return Vec::new(),
        };

        if data_line == "[DONE]" {
            return self.finish_stream();
        }

        let value: Value = match serde_json::from_str(data_line) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let choice = value
            .get("choices")
            .and_then(|v| v.as_array())
            .and_then(|choices| choices.first());

        let Some(choice) = choice else {
            return Vec::new();
        };

        let mut out = Vec::new();
        if let Some(delta) = choice.get("delta") {
            if let Some(reasoning) = delta
                .get("reasoning_content")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                out.extend(self.push_reasoning_delta(reasoning));
            }
            if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    out.extend(self.push_text_delta(text));
                }
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                for tool_call in tool_calls {
                    out.extend(self.push_tool_call_delta(tool_call));
                }
            }
        }

        if let Some(finish_reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
            out.extend(self.handle_finish_reason(finish_reason));
        }

        let _ = &self.message_id;
        out
    }

    fn push_reasoning_delta(&mut self, thinking: &str) -> Vec<String> {
        if thinking.is_empty() || self.thinking_block_stopped {
            return Vec::new();
        }
        let mut out = self.ensure_thinking_block();
        let idx = self.thinking_block_index.unwrap_or(0);
        let event = json!({
            "type": "content_block_delta",
            "index": idx,
            "delta": {"type": "thinking_delta", "thinking": thinking}
        });
        out.push(format!("event: content_block_delta\ndata: {event}\n\n"));
        out
    }

    fn push_text_delta(&mut self, text: &str) -> Vec<String> {
        if text.is_empty() || self.text_block_stopped {
            return Vec::new();
        }
        let mut out = self.ensure_text_block();
        let idx = self.text_block_index.unwrap_or(0);
        let event = json!({
            "type": "content_block_delta",
            "index": idx,
            "delta": {"type": "text_delta", "text": text}
        });
        out.push(format!("event: content_block_delta\ndata: {event}\n\n"));
        out
    }

    fn ensure_thinking_block(&mut self) -> Vec<String> {
        if self.thinking_block_open || self.thinking_block_stopped {
            return Vec::new();
        }
        let idx = self.next_block_index;
        self.next_block_index += 1;
        self.thinking_block_index = Some(idx);
        self.thinking_block_open = true;
        let block = json!({
            "type": "content_block_start",
            "index": idx,
            "content_block": {"type": "thinking", "thinking": ""}
        });
        vec![format!("event: content_block_start\ndata: {block}\n\n")]
    }

    fn ensure_text_block(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if self.thinking_block_open && !self.thinking_block_stopped {
            out.extend(self.stop_thinking_block());
        }
        if self.text_block_open || self.text_block_stopped {
            return out;
        }
        let idx = self.next_block_index;
        self.next_block_index += 1;
        self.text_block_index = Some(idx);
        self.text_block_open = true;
        let block = json!({
            "type": "content_block_start",
            "index": idx,
            "content_block": {"type": "text", "text": ""}
        });
        out.push(format!("event: content_block_start\ndata: {block}\n\n"));
        out
    }

    fn stop_thinking_block(&mut self) -> Vec<String> {
        if !self.thinking_block_open || self.thinking_block_stopped {
            return Vec::new();
        }
        self.thinking_block_stopped = true;
        self.thinking_block_open = false;
        let idx = self.thinking_block_index.unwrap_or(0);
        vec![format!(
            "event: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":{idx}}}\n\n"
        )]
    }

    fn stop_text_block(&mut self) -> Vec<String> {
        if !self.text_block_open || self.text_block_stopped {
            return Vec::new();
        }
        self.text_block_stopped = true;
        self.text_block_open = false;
        let idx = self.text_block_index.unwrap_or(0);
        vec![format!(
            "event: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":{idx}}}\n\n"
        )]
    }

    fn stop_content_blocks_before_tool(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if self.thinking_block_open && !self.thinking_block_stopped {
            out.extend(self.stop_thinking_block());
        }
        if self.text_block_open && !self.text_block_stopped {
            out.extend(self.stop_text_block());
        }
        out
    }

    fn push_tool_call_delta(&mut self, tool_call: &Value) -> Vec<String> {
        let index = tool_call
            .get("index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let mut out = Vec::new();
        let should_start = {
            let entry = self.tool_calls.entry(index).or_default();
            if let Some(id) = tool_call.get("id").and_then(|v| v.as_str()) {
                if !id.is_empty() {
                    entry.id = id.to_string();
                }
            }
            if let Some(name) = tool_call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
            {
                if !name.is_empty() {
                    entry.name = name.to_string();
                }
            }
            if let Some(args) = tool_call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
            {
                entry.arguments.push_str(args);
            }
            !entry.block_started && !entry.id.is_empty() && !entry.name.is_empty()
        };

        if should_start {
            out.extend(self.stop_content_blocks_before_tool());
            let (id, name) = {
                let entry = self.tool_calls.get(&index).unwrap();
                (entry.id.clone(), entry.name.clone())
            };
            let block_index = self.next_block_index;
            self.next_block_index += 1;
            self.tool_block_indices.insert(index, block_index);
            if let Some(entry) = self.tool_calls.get_mut(&index) {
                entry.block_started = true;
            }
            let block = json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": {
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": {}
                }
            });
            out.push(format!("event: content_block_start\ndata: {block}\n\n"));
        }

        if self
            .tool_calls
            .get(&index)
            .is_some_and(|entry| entry.block_started)
        {
            if let Some(args) = tool_call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
            {
                if !args.is_empty() {
                    let block_index = *self.tool_block_indices.get(&index).unwrap_or(&index);
                    let event = json!({
                        "type": "content_block_delta",
                        "index": block_index,
                        "delta": {"type": "input_json_delta", "partial_json": args}
                    });
                    out.push(format!("event: content_block_delta\ndata: {event}\n\n"));
                }
            }
        }

        out
    }

    fn handle_finish_reason(&mut self, finish_reason: &str) -> Vec<String> {
        if self.finish_emitted {
            return Vec::new();
        }
        let mut out = Vec::new();

        if self.thinking_block_open && !self.thinking_block_stopped {
            out.extend(self.stop_thinking_block());
        }
        if self.text_block_open && !self.text_block_stopped {
            out.extend(self.stop_text_block());
        }

        for (tool_index, builder) in self.tool_calls.iter_mut() {
            if builder.block_started && !builder.block_stopped {
                let block_index = self
                    .tool_block_indices
                    .get(tool_index)
                    .copied()
                    .unwrap_or(*tool_index);
                out.push(format!(
                    "event: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":{block_index}}}\n\n"
                ));
                builder.block_stopped = true;
            }
        }

        let stop_reason = map_finish_reason(Some(finish_reason));
        let event = json!({
            "type": "message_delta",
            "delta": {"stop_reason": stop_reason},
            "usage": {"output_tokens": 0}
        });
        out.push(format!("event: message_delta\ndata: {event}\n\n"));
        out.extend(self.finish_stream());
        out
    }

    fn finish_stream(&mut self) -> Vec<String> {
        if self.finish_emitted {
            return Vec::new();
        }
        self.finish_emitted = true;
        vec![format!(
            "event: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n"
        )]
    }
}

pub fn anthropic_stream_preamble(model: &str, message_id: &str) -> String {
    let message = json!({
        "type": "message_start",
        "message": {
            "id": message_id,
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": [],
            "stop_reason": null,
            "stop_sequence": null,
            "usage": {"input_tokens": 0, "output_tokens": 0}
        }
    });
    format!("event: message_start\ndata: {message}\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_chat_completion_json() {
        let body = br#"{"id":"chatcmpl_1","model":"deepseek-v4-flash","choices":[{"message":{"role":"assistant","content":"pong"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#;
        let out = convert_chat_json_to_anthropic(body).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["type"], "message");
        assert_eq!(v["content"][0]["text"], "pong");
    }

    #[test]
    fn converts_reasoning_content_to_thinking_block() {
        let body = br#"{"id":"chatcmpl_1","model":"deepseek-v4-pro","choices":[{"message":{"role":"assistant","reasoning_content":"plan","content":"done"},"finish_reason":"stop"}]}"#;
        let out = convert_chat_json_to_anthropic(body).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["content"][0]["type"], "thinking");
        assert_eq!(v["content"][1]["text"], "done");
    }

    #[test]
    fn streaming_reasoning_content_maps_to_thinking_delta() {
        let mut translator = AnthropicSseTranslator::new("msg_test");
        let chunk1 = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"plan\"}}]}\n\n";
        let chunk2 = "data: {\"choices\":[{\"delta\":{\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\n";
        let out1 = translator.convert_event(chunk1).join("");
        let out2 = translator.convert_event(chunk2).join("");
        assert!(out1.contains("\"type\":\"thinking\""));
        assert!(out1.contains("thinking_delta"));
        assert!(!out1.contains("text_delta"));
        assert!(out2.contains("text_delta"));
        assert!(out2.contains("message_stop"));
    }

    #[test]
    fn streaming_reasoning_text_then_tool_calls_sequence() {
        let mut translator = AnthropicSseTranslator::new("msg_test");
        let chunk1 = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"plan\"}}]}\n\n";
        let chunk2 = "data: {\"choices\":[{\"delta\":{\"content\":\"go\"}}]}\n\n";
        let chunk3 = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"read_file\"}}]}}]}\n\n";
        let chunk4 = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n";
        let out = [
            translator.convert_event(chunk1).join(""),
            translator.convert_event(chunk2).join(""),
            translator.convert_event(chunk3).join(""),
            translator.convert_event(chunk4).join(""),
        ]
        .join("");
        assert!(out.contains("thinking_delta"));
        assert!(out.contains("text_delta"));
        assert!(out.contains("tool_use"));
        assert!(out.contains("message_stop"));
    }

    #[test]
    fn streaming_tool_calls_translate_to_anthropic_events() {
        let mut translator = AnthropicSseTranslator::new("msg_test");
        let chunk1 = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"read_file\"}}]}}]}\n\n";
        let chunk2 = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\\\"a.md\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n";
        let out1 = translator.convert_event(chunk1).join("");
        let out2 = translator.convert_event(chunk2).join("");
        assert!(out1.contains("content_block_start"));
        assert!(out1.contains("tool_use"));
        assert!(out2.contains("input_json_delta"));
        assert!(out2.contains("tool_use"));
        assert!(out2.contains("message_stop"));
    }
}
