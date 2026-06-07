use serde_json::{json, Value};

use crate::config::ProviderConfig;

use super::message_repair::{
    finalize_chat_request, repair_messages_for_upstream_with_options, repair_options_for_provider,
};
use super::reasoning_options::apply_default_reasoning_effort;

pub struct ConvertOptions<'a> {
    pub provider: Option<&'a ProviderConfig>,
    pub model_reasoning_effort: &'a str,
    pub tool_output_max_chars: usize,
}

impl<'a> Default for ConvertOptions<'a> {
    fn default() -> Self {
        Self {
            provider: None,
            model_reasoning_effort: crate::config::DEFAULT_MODEL_REASONING_EFFORT,
            tool_output_max_chars: 0,
        }
    }
}

#[allow(dead_code)]
pub fn convert_anthropic_to_chat(body: &[u8], upstream_model: &str) -> anyhow::Result<Vec<u8>> {
    convert_anthropic_to_chat_with_options(body, upstream_model, ConvertOptions::default())
}

pub fn convert_anthropic_to_chat_with_options(
    body: &[u8],
    upstream_model: &str,
    options: ConvertOptions<'_>,
) -> anyhow::Result<Vec<u8>> {
    let value: Value = serde_json::from_slice(body)?;

    let mut messages = Vec::new();

    if let Some(system) = value.get("system") {
        let content = system_content_to_string(system);
        if !content.is_empty() {
            messages.push(json!({"role": "system", "content": content}));
        }
    }

    if let Some(items) = value.get("messages").and_then(|v| v.as_array()) {
        for item in items {
            messages.extend(convert_message(item)?);
        }
    }

    if messages.is_empty() {
        anyhow::bail!("Anthropic 请求缺少 messages");
    }

    repair_messages_for_upstream_with_options(
        &mut messages,
        repair_options_for_provider(options.provider, options.tool_output_max_chars),
    );

    let requested_model = value
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(upstream_model);

    let stream = value
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut chat = json!({
        "model": requested_model,
        "messages": messages,
        "max_tokens": value.get("max_tokens").cloned().unwrap_or(json!(4096)),
        "stream": stream,
    });

    if let Some(tools) = value.get("tools") {
        chat["tools"] = convert_tools(tools)?;
    }
    if let Some(tool_choice) = value.get("tool_choice") {
        chat["tool_choice"] = convert_tool_choice(tool_choice);
    }
    if let Some(temp) = value.get("temperature") {
        chat["temperature"] = temp.clone();
    }
    if let Some(top_p) = value.get("top_p") {
        chat["top_p"] = top_p.clone();
    }
    if let Some(stop) = value.get("stop_sequences") {
        chat["stop"] = stop.clone();
    }

    if let Some(provider) = options.provider {
        apply_default_reasoning_effort(&mut chat, options.model_reasoning_effort, provider);
    }
    finalize_chat_request(&mut chat, stream);

    Ok(serde_json::to_vec(&chat)?)
}

fn convert_message(item: &Value) -> anyhow::Result<Vec<Value>> {
    let role = item
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("user");
    let content = item.get("content").cloned().unwrap_or(Value::Null);

    if role == "assistant" {
        return Ok(vec![convert_assistant_message(&content)]);
    }
    if role == "user" {
        return Ok(expand_user_message(&content));
    }
    Ok(vec![json!({
        "role": role,
        "content": content_to_plain_string(&content),
    })])
}

fn convert_assistant_message(content: &Value) -> Value {
    if content.is_string() {
        return json!({"role": "assistant", "content": content});
    }

    let mut text_parts = Vec::new();
    let mut thinking_parts = Vec::new();
    let mut tool_calls = Vec::new();

    if let Some(blocks) = content.as_array() {
        for block in blocks {
            match block.get("type").and_then(|v| v.as_str()) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        text_parts.push(text);
                    }
                }
                Some("tool_use") => {
                    tool_calls.push(json!({
                        "id": block.get("id").cloned().unwrap_or(Value::Null),
                        "type": "function",
                        "function": {
                            "name": block.get("name").cloned().unwrap_or(Value::Null),
                            "arguments": serde_json::to_string(
                                &block.get("input").cloned().unwrap_or(json!({}))
                            ).unwrap_or_else(|_| "{}".into()),
                        }
                    }));
                }
                Some("thinking") => {
                    if let Some(thinking) = block.get("thinking").and_then(|v| v.as_str()) {
                        thinking_parts.push(thinking);
                    }
                }
                _ => {}
            }
        }
    }

    let mut message = json!({"role": "assistant"});
    let text = text_parts.join("\n");
    if !text.is_empty() {
        message["content"] = json!(text);
    }
    if !thinking_parts.is_empty() {
        message["reasoning_content"] = json!(thinking_parts.join("\n"));
    }
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }
    message
}

fn expand_user_message(content: &Value) -> Vec<Value> {
    if content.is_string() || content.is_null() {
        return vec![json!({"role": "user", "content": content})];
    }

    if let Some(blocks) = content.as_array() {
        let mut text_parts = Vec::new();
        let mut tool_messages = Vec::new();
        for block in blocks {
            match block.get("type").and_then(|v| v.as_str()) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        text_parts.push(text);
                    }
                }
                Some("tool_result") => {
                    tool_messages.push(json!({
                        "role": "tool",
                        "tool_call_id": block.get("tool_use_id").cloned().unwrap_or(Value::Null),
                        "content": tool_result_content(block.get("content")),
                    }));
                }
                _ => {}
            }
        }

        let mut out = Vec::new();
        out.extend(tool_messages);
        if !text_parts.is_empty() {
            out.push(json!({
                "role": "user",
                "content": text_parts.join("\n"),
            }));
        }
        if out.is_empty() {
            out.push(json!({"role": "user", "content": ""}));
        }
        return out;
    }

    vec![json!({"role": "user", "content": content})]
}

fn tool_result_content(content: Option<&Value>) -> Value {
    Value::String(tool_result_to_string(content))
}

fn tool_result_to_string(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => flatten_tool_result_blocks(blocks),
        Some(other) => serde_json::to_string(other).unwrap_or_default(),
        None => String::new(),
    }
}

fn flatten_tool_result_blocks(blocks: &[Value]) -> String {
    let mut parts = Vec::new();
    for block in blocks {
        match block.get("type").and_then(|v| v.as_str()) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    parts.push(text.to_string());
                }
            }
            Some("image") => parts.push("[image omitted]".to_string()),
            _ => {
                if let Ok(serialized) = serde_json::to_string(block) {
                    parts.push(serialized);
                }
            }
        }
    }
    parts.join("\n")
}

fn convert_tools(tools: &Value) -> anyhow::Result<Value> {
    let Some(items) = tools.as_array() else {
        return Ok(tools.clone());
    };

    let converted: Vec<Value> = items
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.get("name").cloned().unwrap_or(Value::Null),
                    "description": tool.get("description").cloned().unwrap_or(Value::Null),
                    "parameters": tool.get("input_schema").cloned().unwrap_or(json!({})),
                }
            })
        })
        .collect();
    Ok(Value::Array(converted))
}

fn convert_tool_choice(tool_choice: &Value) -> Value {
    match tool_choice.get("type").and_then(|v| v.as_str()) {
        Some("auto") => json!("auto"),
        Some("any") => json!("required"),
        Some("tool") => {
            if let Some(name) = tool_choice.get("name").and_then(|v| v.as_str()) {
                json!({"type": "function", "function": {"name": name}})
            } else {
                json!("auto")
            }
        }
        _ => tool_choice.clone(),
    }
}

fn system_content_to_string(system: &Value) -> String {
    if let Some(text) = system.as_str() {
        return text.to_string();
    }
    if let Some(blocks) = system.as_array() {
        return blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

fn content_to_plain_string(content: &Value) -> Value {
    if content.is_string() || content.is_null() {
        return content.clone();
    }
    if let Some(blocks) = content.as_array() {
        let text = blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        return Value::String(text);
    }
    content.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;

    fn qwen_provider() -> ProviderConfig {
        ProviderConfig {
            id: "qwen".into(),
            name: "千问".into(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            api_key_env: "DASHSCOPE_API_KEY".into(),
            default_model: "qwen3.7-max".into(),
            api_model: "qwen3.7-max".into(),
            wire_api: "chat".into(),
        }
    }

    #[test]
    fn converts_basic_anthropic_request() {
        let body = br#"{"model":"claude-sonnet-4","max_tokens":100,"system":"hi","messages":[{"role":"user","content":"ping"}]}"#;
        let out = convert_anthropic_to_chat(body, "deepseek-v4-flash").unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["model"], "claude-sonnet-4");
        assert_eq!(v["messages"][0]["role"], "system");
        assert_eq!(v["messages"][1]["content"], "ping");
    }

    #[test]
    fn expands_tool_results_to_separate_tool_messages() {
        let body = br#"{
            "model": "qwen3.7-max",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "tool_result", "tool_use_id": "call_1", "content": "ok"},
                    {"type": "text", "text": "continue"}
                ]
            }]
        }"#;
        let out = convert_anthropic_to_chat(body, "qwen3.7-max").unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        let msgs = v["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "tool");
        assert_eq!(msgs[0]["tool_call_id"], "call_1");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "continue");
    }

    #[test]
    fn maps_thinking_block_to_reasoning_content() {
        let body = br#"{
            "model": "deepseek-v4-pro",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "plan"},
                    {"type": "text", "text": "hi"}
                ]
            }]
        }"#;
        let out = convert_anthropic_to_chat(body, "deepseek-v4-pro").unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["messages"][0]["reasoning_content"], "plan");
        assert_eq!(v["messages"][0]["content"], "hi");
    }

    #[test]
    fn flattens_tool_result_array_content_to_string() {
        let body = br#"{
            "model": "qwen3.7-max",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "tool_result", "tool_use_id": "call_1", "content": [
                        {"type": "text", "text": "line1"},
                        {"type": "text", "text": "line2"}
                    ]}
                ]
            }]
        }"#;
        let out = convert_anthropic_to_chat(body, "qwen3.7-max").unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["messages"][0]["content"], "line1\nline2");
    }

    #[test]
    fn converts_assistant_tool_use_message() {
        let body = br#"{
            "model": "qwen3.7-max",
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "checking"},
                    {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {"path": "a.md"}}
                ]
            }]
        }"#;
        let out = convert_anthropic_to_chat(body, "qwen3.7-max").unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["messages"][0]["content"], "checking");
        assert_eq!(v["messages"][0]["tool_calls"][0]["function"]["name"], "read_file");
    }

    #[test]
    fn converts_tool_choice_any_to_required() {
        let body = br#"{
            "model": "qwen3.7-max",
            "tool_choice": {"type": "any"},
            "tools": [{"name": "read_file", "description": "read", "input_schema": {"type": "object"}}],
            "messages": [{"role": "user", "content": "hi"}]
        }"#;
        let out = convert_anthropic_to_chat(body, "qwen3.7-max").unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["tool_choice"], "required");
    }

    #[test]
    fn flattens_tool_result_image_block() {
        let body = br#"{
            "model": "qwen3.7-max",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "tool_result", "tool_use_id": "call_1", "content": [
                        {"type": "image"},
                        {"type": "text", "text": "done"}
                    ]}
                ]
            }]
        }"#;
        let out = convert_anthropic_to_chat(body, "qwen3.7-max").unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["messages"][0]["content"], "[image omitted]\ndone");
    }

    #[test]
    fn applies_default_reasoning_for_chat_provider() {
        let body = br#"{"model":"qwen3.7-max","messages":[{"role":"user","content":"hi"}]}"#;
        let out = convert_anthropic_to_chat_with_options(
            body,
            "qwen3.7-max",
            ConvertOptions {
                provider: Some(&qwen_provider()),
                model_reasoning_effort: "medium",
                tool_output_max_chars: 0,
            },
        )
        .unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["enable_thinking"], true);
    }
}
