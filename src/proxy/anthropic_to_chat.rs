use serde_json::{json, Value};

pub fn convert_anthropic_to_chat(body: &[u8], upstream_model: &str) -> anyhow::Result<Vec<u8>> {
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
            messages.push(convert_message(item)?);
        }
    }

    if messages.is_empty() {
        anyhow::bail!("Anthropic 请求缺少 messages");
    }

    let requested_model = value
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(upstream_model);

    let mut chat = json!({
        "model": requested_model,
        "messages": messages,
        "max_tokens": value.get("max_tokens").cloned().unwrap_or(json!(4096)),
        "stream": value.get("stream").cloned().unwrap_or(Value::Bool(false)),
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

    Ok(serde_json::to_vec(&chat)?)
}

fn convert_message(item: &Value) -> anyhow::Result<Value> {
    let role = item
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("user");
    let content = item.get("content").cloned().unwrap_or(Value::Null);

    if role == "assistant" {
        return Ok(convert_assistant_message(&content));
    }
    if role == "user" {
        return Ok(json!({
            "role": "user",
            "content": content_to_string(&content),
        }));
    }
    Ok(json!({"role": role, "content": content_to_string(&content)}))
}

fn convert_assistant_message(content: &Value) -> Value {
    if content.is_string() {
        return json!({"role": "assistant", "content": content});
    }

    let mut text_parts = Vec::new();
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
                        text_parts.push(thinking);
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
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }
    message
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

fn content_to_string(content: &Value) -> Value {
    if content.is_string() || content.is_null() {
        return content.clone();
    }
    if let Some(blocks) = content.as_array() {
        let mut text_parts = Vec::new();
        let mut tool_results = Vec::new();
        for block in blocks {
            match block.get("type").and_then(|v| v.as_str()) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        text_parts.push(text);
                    }
                }
                Some("tool_result") => {
                    tool_results.push(json!({
                        "role": "tool",
                        "tool_call_id": block.get("tool_use_id").cloned().unwrap_or(Value::Null),
                        "content": block.get("content").cloned().unwrap_or(Value::Null),
                    }));
                }
                _ => {}
            }
        }
        if !tool_results.is_empty() {
            return Value::Array(tool_results);
        }
        return Value::String(text_parts.join("\n"));
    }
    content.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_basic_anthropic_request() {
        let body = br#"{"model":"claude-sonnet-4","max_tokens":100,"system":"hi","messages":[{"role":"user","content":"ping"}]}"#;
        let out = convert_anthropic_to_chat(body, "deepseek-v4-flash").unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["model"], "claude-sonnet-4");
        assert_eq!(v["messages"][0]["role"], "system");
        assert_eq!(v["messages"][1]["content"], "ping");
    }
}
