//! Utility functions for the web IDE.
//!
//! Pure helpers for JSON formatting, value parsing, and API error display.

use serde_json::Value as JsonValue;
use tabula_core::Value as CoreValue;

use crate::api::ApiClientError;
use crate::models::{State, TransactionBatch};

pub(crate) fn parse_state(raw: &str) -> Result<State, String> {
    serde_json::from_str::<State>(raw).map_err(|e| e.to_string())
}

pub(crate) fn parse_batch(raw: &str) -> Result<TransactionBatch, String> {
    serde_json::from_str::<TransactionBatch>(raw).map_err(|e| e.to_string())
}

pub(crate) fn format_api_err(ctx: &str, err: &ApiClientError) -> String {
    let mut out = format!("{} failed\n- message: {}", ctx.to_uppercase(), err.message);
    if let Some(status) = err.status {
        out.push_str(&format!("\n- http_status: {status}"));
    }
    if let Some(code) = err.code.as_ref() {
        out.push_str(&format!("\n- code: {code}"));
    }
    if let Some(details) = err.details.as_ref() {
        out.push_str("\n- details:\n");
        out.push_str(&pretty_json_value(details));
    }
    out
}

pub(crate) fn pretty_json_value(value: &JsonValue) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

pub(crate) fn pretty_json_inline<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

pub(crate) fn opt_token(token: String) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn format_value(v: &CoreValue) -> String {
    match v {
        CoreValue::U64(n) => n.to_string(),
        CoreValue::I64(n) => n.to_string(),
        CoreValue::Bool(b) => b.to_string(),
        CoreValue::Bytes32(d) => format!("0x{}", hex::encode(d)),
    }
}

pub(crate) fn parse_value_input(raw: &str) -> Option<CoreValue> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return None;
    }

    // Try parsing as JSON value first.
    if let Ok(v) = serde_json::from_str::<CoreValue>(trimmed) {
        return Some(v);
    }

    // Try bare integer.
    if let Ok(n) = trimmed.parse::<u64>() {
        return Some(CoreValue::U64(n));
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return Some(CoreValue::I64(n));
    }

    // Try boolean.
    match trimmed {
        "true" => return Some(CoreValue::Bool(true)),
        "false" => return Some(CoreValue::Bool(false)),
        _ => {}
    }

    None
}

pub(crate) fn default_value_for_type(type_name: &str) -> CoreValue {
    match type_name {
        "U64" => CoreValue::U64(0),
        "I64" => CoreValue::I64(0),
        "Bool" => CoreValue::Bool(false),
        _ => CoreValue::U64(0),
    }
}
