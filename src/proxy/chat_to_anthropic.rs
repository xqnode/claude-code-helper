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
    let stop_reason = match choice.get("finish_reason").and_then(|v| v.as_str()) {
        Some("tool_calls") => "tool_use",
        Some("length") => "max_tokens",
        _ => "end_turn",
    };

    let mut content = Vec::new();
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

pub fn wrap_chat_sse_as_anthropic_sse(chunk: &str, message_id: &str) -> Option<String> {
    let data_line = chunk
        .lines()
        .find(|line| line.starts_with("data: "))?
        .trim_start_matches("data: ")
        .trim();
    if data_line == "[DONE]" {
        return Some(format!(
            "event: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n"
        ));
    }
    let value: Value = serde_json::from_str(data_line).ok()?;
    let delta = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))?;

    if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            let event = json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": text}
            });
            return Some(format!("event: content_block_delta\ndata: {event}\n\n"));
        }
    }

    if delta.get("tool_calls").is_some() {
        return None;
    }

    if value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("finish_reason"))
        .is_some()
    {
        let event = json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {"output_tokens": 0}
        });
        return Some(format!(
            "event: message_delta\ndata: {event}\n\nevent: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n"
        ));
    }

    let _ = message_id;
    None
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
    let block = json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}});
    format!(
        "event: message_start\ndata: {message}\n\nevent: content_block_start\ndata: {block}\n\n"
    )
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
}
