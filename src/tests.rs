//! Unit tests for the pure helpers: parsing, formatting, and range math.

use serde_json::json;

use crate::format::{int_grouped, money_grouped, num, rows_of};
use crate::metrics::{find, is_bucket, is_error_metric, is_generations_only};
use crate::ranges::{chunk, iso, resolve};

#[test]
fn num_accepts_numbers_strings_and_junk() {
    assert_eq!(num(Some(&json!(1.5))), 1.5);
    assert_eq!(num(Some(&json!("2.25"))), 2.25);
    assert_eq!(num(Some(&json!("1,234.5"))), 1234.5);
    assert_eq!(num(Some(&json!(null))), 0.0);
    assert_eq!(num(Some(&json!(true))), 0.0);
    assert_eq!(num(Some(&json!("not a number"))), 0.0);
    assert_eq!(num(None), 0.0);
}

#[test]
fn rows_of_unwraps_every_response_shape() {
    let bare = json!([{ "a": 1 }]);
    assert_eq!(rows_of(&bare).len(), 1);

    let single = json!({ "data": [{ "a": 1 }, { "a": 2 }] });
    assert_eq!(rows_of(&single).len(), 2);

    // The shape OpenRouter actually returns.
    let double = json!({ "data": { "data": [{ "a": 1 }] } });
    assert_eq!(rows_of(&double).len(), 1);

    assert!(rows_of(&json!({ "data": {} })).is_empty());
    assert!(rows_of(&json!("nope")).is_empty());
}

#[test]
fn money_formatting_matches_the_python_cli() {
    assert_eq!(money_grouped(0.0), "$0.00");
    assert_eq!(money_grouped(0.0068), "$0.0068");
    assert_eq!(money_grouped(6.63), "$6.63");
    assert_eq!(money_grouped(1234.5), "$1,234.50");
    assert_eq!(money_grouped(-0.84), "-$0.84");
}

#[test]
fn int_formatting_groups_thousands() {
    assert_eq!(int_grouped(0.0), "0");
    assert_eq!(int_grouped(423.0), "423");
    assert_eq!(int_grouped(377075253.0), "377,075,253");
    assert_eq!(int_grouped(-1234.0), "-1,234");
}

#[test]
fn metric_table_lookups() {
    assert_eq!(find("cost").unwrap().api, "total_usage");
    assert_eq!(find("tokens").unwrap().api, "tokens_total");
    assert!(find("nope").is_none());

    // Rates must never be summed across rows.
    assert!(!find("latency").unwrap().additive);
    assert!(find("cost").unwrap().additive);

    assert!(is_error_metric("gateway_5xx_count"));
    assert!(!is_error_metric("total_usage"));
    assert!(is_generations_only("session_id"));
    assert!(!is_generations_only("model"));
    assert!(is_bucket("day"));
    assert!(!is_bucket("model"));
}

#[test]
fn iso_is_second_precision_utc() {
    let window = resolve(Some("1h"), None, None, None).unwrap();
    let text = iso(window.start);
    assert!(text.ends_with('Z'), "got {text}");
    assert_eq!(text.len(), 20, "got {text}");
}

#[test]
fn ranges_resolve_to_sane_windows() {
    for name in ["15m", "1h", "24h", "1w", "1mo", "today", "yesterday", "this-week", "prev-week", "this-month", "prev-month", "this-year", "prev-year"] {
        let window = resolve(Some(name), None, None, None).unwrap();
        assert!(window.start < window.end, "{name} produced an empty window");
    }

    let today = resolve(Some("today"), None, None, None).unwrap();
    assert_eq!(today.label, "today");

    let days = resolve(None, Some(3), None, None).unwrap();
    assert_eq!(days.label, "last 3 day(s)");
}

#[test]
fn explicit_dates_are_validated() {
    assert!(resolve(None, None, Some("2026-11-01"), None).is_err());
    assert!(resolve(None, None, Some("2026-11-01"), Some("2026-11-10")).is_ok());
    assert!(resolve(None, None, Some("2026-11-10"), Some("2026-11-01")).is_err());
    assert!(resolve(None, None, Some("nope"), Some("2026-11-10")).is_err());
    assert!(resolve(Some("bogus"), None, None, None).is_err());
}

#[test]
fn chunk_splits_long_windows() {
    let window = resolve(Some("1y"), None, None, None).unwrap();
    let parts = chunk(window.start, window.end, 31);
    assert!(parts.len() >= 12, "got {} parts", parts.len());
    for (start, end) in &parts {
        assert!(start < end);
        assert!((*end - *start).num_days() <= 31);
    }
    // No gaps between consecutive windows.
    for pair in parts.windows(2) {
        assert_eq!(pair[0].1, pair[1].0);
    }
}
