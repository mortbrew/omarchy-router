//! OpenRouter API access: credential loading and the analytics query call.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

const API: &str = "https://openrouter.ai/api/v1";
const PLACEHOLDER: &str = "PASTE_YOUR_MANAGEMENT_KEY_HERE";
const TIMEOUT: Duration = Duration::from_secs(60);

/// Path to the credential file.
pub fn key_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/openrouter-cost/key")
}

/// First non-blank, non-comment line, so the file can be self-documenting.
fn read_key_file(path: &PathBuf) -> Result<String> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        return Ok(line.to_string());
    }
    Ok(String::new())
}

/// Resolve the management key from the environment, then the config file.
pub fn load_key() -> Result<String> {
    if let Ok(key) = env::var("OPENROUTER_MANAGEMENT_KEY") {
        let key = key.trim();
        if !key.is_empty() {
            return Ok(key.to_string());
        }
    }

    let path = key_path();
    if path.exists() {
        let key = read_key_file(&path)?;
        if !key.is_empty() && key != PLACEHOLDER {
            return Ok(key);
        }
        bail!(
            "no key pasted yet.\n  \
             Open {} and paste your management key on its own line\n  \
             at the bottom, below the commented {} line.\n  \
             Get one at https://openrouter.ai/settings/management-keys",
            path.display(),
            PLACEHOLDER
        );
    }

    bail!(
        "no management key found.\n  \
         Set OPENROUTER_MANAGEMENT_KEY, or write the key to:\n    {}   (then: chmod 600 {})\n  \
         Create a management key at https://openrouter.ai/settings/management-keys",
        path.display(),
        path.display()
    )
}

/// Perform a request against the OpenRouter API and decode the JSON body.
pub fn request(method: &str, path: &str, body: Option<&Value>, key: &str) -> Result<Value> {
    let url = format!("{API}{path}");
    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();

    let mut req = match method {
        "GET" => agent.get(&url),
        "POST" => agent.post(&url),
        other => bail!("unsupported method {other}"),
    };
    req = req
        .set("Authorization", &format!("Bearer {key}"))
        .set("Accept", "application/json");

    let result = match body {
        Some(payload) => req
            .set("Content-Type", "application/json")
            .send_string(&payload.to_string()),
        None => req.call(),
    };

    match result {
        Ok(resp) => resp
            .into_json::<Value>()
            .with_context(|| format!("invalid JSON from {path}")),
        Err(ureq::Error::Status(code, resp)) => {
            let detail = resp.into_string().unwrap_or_default();
            let detail: String = detail.chars().take(600).collect();
            if code == 401 {
                bail!(
                    "401 Unauthorized. Analytics needs a MANAGEMENT key \
                     (openrouter.ai/settings/management-keys), not a regular API key.\n  \
                     server said: {detail}"
                );
            }
            bail!("HTTP {code} from {path}: {detail}")
        }
        Err(ureq::Error::Transport(err)) => {
            Err(anyhow!("network error reaching {url}: {err}"))
        }
    }
}

/// Run one analytics query.
#[allow(clippy::too_many_arguments)]
pub fn run_query(
    key: &str,
    metrics: &[&str],
    dimensions: &[String],
    start: &str,
    end: &str,
    granularity: Option<&str>,
    filters: &[Value],
    limit: usize,
) -> Result<Value> {
    let mut body = json!({
        "metrics": metrics,
        "time_range": { "start": start, "end": end },
        "limit": limit,
    });

    let obj = body.as_object_mut().expect("object literal");
    if !dimensions.is_empty() {
        obj.insert("dimensions".into(), json!(dimensions));
    }
    if let Some(gran) = granularity {
        obj.insert("granularity".into(), json!(gran));
    }
    if !filters.is_empty() {
        obj.insert("filters".into(), json!(filters));
    }

    request("POST", "/analytics/query", Some(&body), key)
}

/// List the account's API keys.
pub fn list_keys(key: &str) -> Result<Vec<Value>> {
    let payload = request("GET", "/keys", None, key)?;
    if let Some(rows) = payload.get("data").and_then(Value::as_array) {
        return Ok(rows.clone());
    }
    if let Some(rows) = payload.as_array() {
        return Ok(rows.clone());
    }
    Ok(Vec::new())
}

/// The `/keys` endpoint calls it `hash`; older shapes used `key_hash`.
pub fn key_hash_of(entry: &Value) -> Option<String> {
    entry
        .get("hash")
        .or_else(|| entry.get("key_hash"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Map a key name (or hash) to the hash the analytics filter needs.
pub fn resolve_key(key: &str, wanted: &str) -> Result<String> {
    let keys = list_keys(key)?;
    if keys.is_empty() {
        bail!("could not list keys; pass --key-hash instead, or check the management key");
    }

    if keys.iter().any(|e| key_hash_of(e).as_deref() == Some(wanted)) {
        return Ok(wanted.to_string());
    }

    for entry in &keys {
        if entry.get("name").and_then(Value::as_str) == Some(wanted) {
            if let Some(hash) = key_hash_of(entry) {
                return Ok(hash);
            }
        }
    }

    let needle = wanted.to_lowercase();
    let matches: Vec<&Value> = keys
        .iter()
        .filter(|e| {
            e.get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase()
                .contains(&needle)
        })
        .collect();

    match matches.len() {
        1 => key_hash_of(matches[0]).ok_or_else(|| anyhow!("key has no hash")),
        0 => {
            let names: Vec<String> = keys
                .iter()
                .map(|e| format!("{:?}", e.get("name").and_then(Value::as_str).unwrap_or("")))
                .collect();
            bail!("no key matching {wanted:?}. Available: {}", names.join(", "))
        }
        _ => {
            let names: Vec<String> = matches
                .iter()
                .map(|e| format!("{:?}", e.get("name").and_then(Value::as_str).unwrap_or("")))
                .collect();
            bail!("ambiguous key {wanted:?} matches: {}", names.join(", "))
        }
    }
}
