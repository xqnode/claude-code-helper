use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::RwLock;
use uuid::Uuid;

mod db;
use db::RequestLogDb;

use crate::paths;

const MAX_ENTRIES: usize = 300;

#[derive(Clone, Debug, Serialize)]
pub struct RequestLogEntry {
    pub id: String,
    pub time_ms: i64,
    pub time_label: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub path: String,
    pub stream: bool,
    pub status: u16,
    pub duration_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_yuan: Option<f64>,
    pub cost_label: String,
    pub ok: bool,
}

#[derive(Clone, Debug)]
pub struct PendingRequest {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub path: String,
    pub stream: bool,
    pub started: Instant,
    pub status: u16,
}

#[derive(Clone, Debug, Default)]
pub struct UsageSnapshot {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Clone)]
pub struct RequestLogStore {
    inner: Arc<RwLock<VecDeque<RequestLogEntry>>>,
    db: Option<Arc<RequestLogDb>>,
}

impl RequestLogStore {
    pub fn new() -> Self {
        let db = open_request_log_db();
        let mut entries = VecDeque::with_capacity(64);
        if let Some(db) = &db {
            match db.load_recent(MAX_ENTRIES) {
                Ok(loaded) => {
                    tracing::info!("已加载 {} 条历史请求日志", loaded.len());
                    entries = loaded.into_iter().collect();
                }
                Err(err) => tracing::warn!("加载请求日志失败: {err:#}"),
            }
        }
        Self {
            inner: Arc::new(RwLock::new(entries)),
            db,
        }
    }

    pub async fn push(&self, entry: RequestLogEntry) {
        {
            let mut entries = self.inner.write().await;
            entries.push_back(entry.clone());
            while entries.len() > MAX_ENTRIES {
                entries.pop_front();
            }
        }
        if let Some(db) = &self.db {
            if let Err(err) = db.insert(&entry) {
                tracing::warn!("写入请求日志失败: {err:#}");
            } else if let Err(err) = db.trim(MAX_ENTRIES) {
                tracing::warn!("裁剪请求日志失败: {err:#}");
            }
        }
    }

    pub async fn list(&self) -> Vec<RequestLogEntry> {
        let entries = self.inner.read().await;
        entries.iter().rev().cloned().collect()
    }

    pub async fn clear(&self) {
        self.inner.write().await.clear();
        if let Some(db) = &self.db {
            if let Err(err) = db.clear() {
                tracing::warn!("清空请求日志文件失败: {err:#}");
            }
        }
    }

    pub async fn summary(&self) -> RequestLogSummary {
        let entries = self.inner.read().await;
        let mut total_in = 0u64;
        let mut total_out = 0u64;
        let mut total_cost = 0.0f64;
        let mut cost_known = 0usize;
        for entry in entries.iter() {
            total_in += entry.input_tokens;
            total_out += entry.output_tokens;
            if let Some(cost) = entry.cost_yuan {
                total_cost += cost;
                cost_known += 1;
            }
        }
        RequestLogSummary {
            count: entries.len(),
            total_input_tokens: total_in,
            total_output_tokens: total_out,
            total_cost_yuan: if cost_known > 0 {
                Some(total_cost)
            } else {
                None
            },
        }
    }

    pub fn finalize(&self, pending: PendingRequest, usage: Option<UsageSnapshot>) -> RequestLogEntry {
        let usage = usage.unwrap_or_default();
        let duration_ms = pending.started.elapsed().as_millis() as u64;
        let cost_yuan = estimate_cost_yuan(
            &pending.provider_id,
            &pending.model,
            usage.input_tokens,
            usage.output_tokens,
        );
        let cost_label = cost_yuan
            .map(format_cost_yuan)
            .unwrap_or_else(|| "—".into());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        RequestLogEntry {
            id: Uuid::new_v4().to_string(),
            time_ms: now,
            time_label: format_time_label(now),
            provider_id: pending.provider_id,
            provider_name: pending.provider_name,
            model: pending.model,
            path: pending.path,
            stream: pending.stream,
            status: pending.status,
            duration_ms,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
            cost_yuan,
            cost_label,
            ok: pending.status >= 200 && pending.status < 400,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RequestLogSummary {
    pub count: usize,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_yuan: Option<f64>,
}

pub fn parse_usage_from_json(value: &serde_json::Value) -> Option<UsageSnapshot> {
    let usage = value.get("usage")?;
    parse_usage_object(usage)
}

pub fn parse_usage_from_bytes(bytes: &[u8]) -> Option<UsageSnapshot> {
    let mut latest = None;
    for line in bytes.split(|b| *b == b'\n') {
        let line = line.strip_prefix(b"data: ").or_else(|| {
            line.strip_prefix(b"data:")
                .map(|rest| rest.strip_prefix(b" ").unwrap_or(rest))
        });
        let Some(line) = line else { continue };
        if line == b"[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(usage) = parse_usage_from_json(&value) {
            latest = Some(usage);
        }
    }
    latest
}

fn parse_usage_object(usage: &serde_json::Value) -> Option<UsageSnapshot> {
    if !usage.is_object() {
        return None;
    }
    let input_tokens = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(input_tokens + output_tokens);
    if input_tokens == 0 && output_tokens == 0 && total_tokens == 0 {
        return None;
    }
    Some(UsageSnapshot {
        input_tokens,
        output_tokens,
        total_tokens,
    })
}

fn open_request_log_db() -> Option<Arc<RequestLogDb>> {
    let path = paths::helper_request_log_path().ok()?;
    match RequestLogDb::open(&path) {
        Ok(db) => Some(Arc::new(db)),
        Err(err) => {
            tracing::warn!("无法打开请求日志数据库 {}: {err:#}", path.display());
            None
        }
    }
}

/// 公开价估算（元 / 百万 tokens），未知模型返回 None。
fn estimate_cost_yuan(
    provider_id: &str,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> Option<f64> {
    if input_tokens == 0 && output_tokens == 0 {
        return Some(0.0);
    }
    let (in_per_m, out_per_m) = pricing_per_million(provider_id, model)?;
    Some(
        (input_tokens as f64 * in_per_m + output_tokens as f64 * out_per_m) / 1_000_000.0,
    )
}

fn pricing_per_million(provider_id: &str, model: &str) -> Option<(f64, f64)> {
    let model = model.to_ascii_lowercase();
    match provider_id {
        "deepseek" if model.contains("flash") => Some((0.5, 2.0)),
        "deepseek" if model.contains("pro") => Some((2.0, 8.0)),
        "qwen" if model.contains("plus") => Some((0.8, 2.0)),
        "qwen" if model.contains("max") => Some((2.0, 6.0)),
        "zhipu" if model.contains("glm-5") => Some((1.0, 3.0)),
        "zhipu" if model.contains("4.7") => Some((0.5, 1.5)),
        "kimi" => Some((1.0, 3.0)),
        "minimax" => Some((1.0, 3.0)),
        "mimo" if model.contains("flash") => Some((0.3, 1.0)),
        "mimo" => Some((1.0, 3.0)),
        "custom" if model.contains("mini") => Some((0.5, 1.5)),
        "custom" if model.contains("5.5") || model.contains("5.4") => Some((2.0, 8.0)),
        _ => None,
    }
}

fn format_cost_yuan(yuan: f64) -> String {
    if yuan < 0.01 {
        format!("约 ¥{:.4}", yuan)
    } else {
        format!("约 ¥{:.3}", yuan)
    }
}

fn format_time_label(time_ms: i64) -> String {
    const BEIJING_OFFSET_SECS: i64 = 8 * 3600;
    let total_secs = time_ms.div_euclid(1000);
    let ms = time_ms.rem_euclid(1000).unsigned_abs() as u32;
    let beijing_secs = total_secs + BEIJING_OFFSET_SECS;
    let days = beijing_secs.div_euclid(86400);
    let tod = beijing_secs.rem_euclid(86400);
    let h = (tod / 3600) as u32;
    let m = ((tod % 3600) / 60) as u32;
    let s = (tod % 60) as u32;
    let (year, month, day) = civil_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02}.{ms:03}")
}

/// Unix 纪元日数 → 公历日期（用于北京时间展示）。
fn civil_from_unix_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    (y, m, d)
}

pub fn extract_model_from_body(body: &[u8], fallback: &str) -> String {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return fallback.to_string();
    };
    value
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usage_from_sse_chunk() {
        let chunk = br#"data: {"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}

"#;
        let usage = parse_usage_from_bytes(chunk).unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 5);
    }

    #[test]
    fn format_time_label_uses_beijing_time() {
        assert_eq!(format_time_label(0), "1970-01-01 08:00:00.000");
        assert_eq!(format_time_label(3_600_000), "1970-01-01 09:00:00.000");
        // 1970-01-01 23:00 UTC → 1970-01-02 07:00 北京时间
        assert_eq!(format_time_label(82_800_000), "1970-01-02 07:00:00.000");
    }
}
