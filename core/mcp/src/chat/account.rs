// SPDX-License-Identifier: MIT
//! Account failures are kept separately from ordinary failed tools. Only
//! protocol errors are passed here; successful assistant text is not scanned.

use serde_json::{Value, json};

pub(super) fn classify(details: &Value, text: &str) -> Option<Value> {
    let structured = details.to_string().to_lowercase();
    let lower = text.to_lowercase();
    let has = |words: &[&str]| {
        words
            .iter()
            .any(|word| lower.contains(word) || structured.contains(word))
    };
    let (code, reason) = if has(&[
        "organization has disabled",
        "organisation has disabled",
        "org_disabled",
        "organization_disabled",
    ]) {
        ("org-disabled", "your organization has disabled access")
    } else if has(&[
        "invalid_api_key",
        "invalid api key",
        "invalid_api-key",
        "incorrect api key",
        "missing api key",
        "api key is missing",
        "api key is invalid",
        "no api key",
        "api_key_missing",
    ]) {
        ("invalid-key", "the API key is missing or invalid")
    } else if has(&[
        "insufficient_quota",
        "billing_hard_limit",
        "credit balance",
        "out of credits",
        "usage limit",
        "quota exceeded",
        "rate_limit",
        "rate limit",
        "ratelimit",
        "too_many_requests",
        "usagelimitexceeded",
        "quotaexceeded",
    ]) || status(details, 429)
    {
        (
            "limit",
            "the account's credit or rate limit has been reached",
        )
    } else if has(&[
        "not signed in",
        "not logged in",
        "authentication_error",
        "authentication_failed",
        "unauthorized",
        "authentication failed",
        "login required",
        "sign in again",
        "session expired",
        "session has expired",
        "token expired",
        "token has expired",
        "unauthenticated",
        // The Agent Client Protocol's auth_required error (mitcad#1198).
        "authentication required",
        "auth_required",
    ]) || status(details, 401)
    {
        ("signed-out", "sign-in is missing or has expired")
    } else {
        return None;
    };
    Some(json!({"code": code, "reason": reason}))
}

fn status(value: &Value, wanted: i64) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| {
            (matches!(
                key.as_str(),
                "status" | "statusCode" | "status_code" | "httpStatusCode"
            ) && (value.as_i64() == Some(wanted)
                || value.as_str().is_some_and(|v| v == wanted.to_string())))
                || status(value, wanted)
        }),
        Value::Array(values) => values.iter().any(|value| status(value, wanted)),
        _ => false,
    }
}

/// Annotate an agent's failed protocol message, preserving the original
/// end event so the controller always finishes the running turn.
pub(super) fn events(line: &str, mut events: Vec<Value>) -> Vec<Value> {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return events;
    };
    let kind = message
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let failed = (kind == "result"
        && (message.get("is_error").and_then(Value::as_bool) == Some(true)
            || message
                .get("subtype")
                .and_then(Value::as_str)
                .is_some_and(|s| s != "success")))
        || (kind == "assistant" && message.get("error").is_some_and(|v| !v.is_null()))
        || method == "error"
        || (method == "turn/completed"
            && message
                .pointer("/params/turn/status")
                .and_then(Value::as_str)
                == Some("failed"))
        || (message.get("id").is_some() && message.get("error").is_some_and(|v| !v.is_null()));
    if !failed {
        return events;
    }
    // The serialized error retains subtypes and HTTP status fields even
    // when an agent's human-readable message is uninformative.
    let mut text = message
        .get("result")
        .and_then(Value::as_str)
        .or_else(|| {
            message
                .pointer("/params/error/message")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            message
                .pointer("/params/turn/error/message")
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_owned();
    if text.is_empty()
        && let Some(content) = message
            .pointer("/message/content")
            .and_then(Value::as_array)
    {
        text = content
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
    }
    if text.is_empty() {
        text = events
            .iter()
            .filter_map(|event| event.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
    }
    let Some(account) = classify(&message, &text) else {
        return events;
    };
    events.retain(|event| event.get("kind").and_then(Value::as_str) != Some("text"));
    for event in &mut events {
        if event.get("kind").and_then(Value::as_str) == Some("turn_end") {
            event["account_error"] = json!(true);
        }
    }
    events.insert(0, json!({"kind": "account_error", "account": account, "text": super::cut(&text, super::MAX_TEXT_CHARS)}));
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_errors_and_ordinary_errors() {
        for (text, code) in [
            (
                "Your organization has disabled Claude subscription access for Claude Code · Use an Anthropic API key instead, or ask your admin to enable access",
                "org-disabled",
            ),
            ("Not signed in. Run login", "signed-out"),
            ("Your session has expired", "signed-out"),
            ("Invalid API key", "invalid-key"),
            ("Missing API key", "invalid-key"),
            ("Rate limit reached", "limit"),
            ("Your credit balance is too low", "limit"),
        ] {
            assert_eq!(classify(&Value::Null, text).unwrap()["code"], code);
        }
        assert_eq!(
            classify(
                &json!({"codexErrorInfo":{"httpConnectionFailed":{"httpStatusCode":401}}}),
                "Request failed"
            )
            .unwrap()["code"],
            "signed-out"
        );
        assert_eq!(
            classify(&json!({"status":429}), "Request failed").unwrap()["code"],
            "limit"
        );
        assert!(classify(&Value::Null, "The fillet tool failed: edge not found").is_none());
        // Tool results and normal assistant discussion of account errors
        // must not change readiness.
        assert!(events(r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":"Invalid API key"}]}}"#, vec![]).is_empty());
        assert!(events(r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Rate limit"}]}}"#, vec![]).is_empty());
    }

    #[test]
    fn both_agents_preserve_turn_end_and_mark_account_errors() {
        for line in [
            r#"{"type":"result","is_error":true,"subtype":"error_during_execution","result":"Your organization has disabled Claude Code"}"#,
            r#"{"method":"turn/completed","params":{"turn":{"status":"failed","error":{"message":"Request failed","codexErrorInfo":{"httpConnectionFailed":{"httpStatusCode":401}}}}}}"#,
        ] {
            let found = events(line, vec![json!({"kind":"turn_end","error":true})]);
            assert_eq!(found[0]["kind"], "account_error");
            assert_eq!(found[1]["kind"], "turn_end");
            assert_eq!(found[1]["account_error"], true);
        }
    }
}
