//! omarchy-router — real dollar spend and usage from OpenRouter.
//!
//! Built for the Cursor + OpenRouter setup: Cursor's dashboard shows tokens but
//! not dollars, and OpenRouter holds the actual bill. This reads the bill.

mod api;
mod format;
mod metrics;
mod ranges;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::Parser;
use serde_json::{json, Value};

use metrics::{Kind, Metric};

/// Real dollar spend and usage from OpenRouter.
#[derive(Parser, Debug)]
#[command(
    name = "omarchy-router",
    version,
    about = "Report real OpenRouter spend and usage.",
    after_help = AFTER_HELP
)]
struct Cli {
    /// 15m,30m,1h,3h,24h,48h,1w,1mo,1y,today,yesterday,this-week,prev-week,
    /// this-month,prev-month,this-year,prev-year (default 1w)
    #[arg(long)]
    range: Option<String>,

    /// Rolling N-day window
    #[arg(long)]
    days: Option<i64>,

    /// Explicit start date (YYYY-MM-DD); requires --to
    #[arg(long, value_name = "YYYY-MM-DD")]
    from: Option<String>,

    /// Explicit end date (YYYY-MM-DD); requires --from
    #[arg(long, value_name = "YYYY-MM-DD")]
    to: Option<String>,

    /// Dimensions to group by, comma separated (default model).
    /// Max 2. Add day/hour/week/month for a time bucket.
    #[arg(long, default_value = "model")]
    by: String,

    /// Comma separated, or 'all' (default cost,tokens,requests)
    #[arg(long)]
    metrics: Option<String>,

    /// Show available metrics
    #[arg(long)]
    list_metrics: bool,

    /// Only this API key (by name)
    #[arg(long, value_name = "NAME")]
    key: Option<String>,

    /// Only this API key (by hash)
    #[arg(long, value_name = "HASH")]
    key_hash: Option<String>,

    /// List keys and exit
    #[arg(long)]
    list_keys: bool,

    /// Emit raw JSON
    #[arg(long)]
    json: bool,

    /// One line of totals for the bar: 1h, today, 7d, 30d
    #[arg(long)]
    brief: bool,

    /// Dump raw API responses
    #[arg(long)]
    debug: bool,

    /// Max rows (default 1000)
    #[arg(long, default_value_t = 1000)]
    limit: usize,
}

const AFTER_HELP: &str = "\
Auth:
  OPENROUTER_MANAGEMENT_KEY env var, or ~/.config/openrouter-cost/key (chmod 600).
  Analytics requires a MANAGEMENT key, not a regular API key.
  Create one at https://openrouter.ai/settings/management-keys

Why per-chat is not possible:
  Cursor reaches OpenRouter through the \"Override OpenAI Base URL\" setting
  (https://openrouter.ai/api/v1/cursor). It does not forward a chat or session
  identifier, so OpenRouter files all Cursor traffic under session_id \"none\".
  Per-key and per-model attribution is the finest granularity available.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    if cli.list_metrics {
        print_metric_list();
        return Ok(());
    }

    let key = api::load_key()?;

    if cli.brief {
        return print_brief(&key, &cli);
    }

    if cli.list_keys {
        return print_keys(&key, cli.json);
    }

    let dimensions: Vec<String> = cli
        .by
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    if dimensions.len() > 2 {
        bail!("at most 2 dimensions (OpenRouter caps this); got {}", dimensions.len());
    }

    let mut granularity: Option<String> = None;
    let mut query_dims: Vec<String> = Vec::new();
    for dim in &dimensions {
        if metrics::is_bucket(dim) {
            granularity = Some(dim.clone());
        } else {
            query_dims.push(dim.clone());
        }
    }

    let mut filters: Vec<Value> = Vec::new();
    if let Some(target) = key_filter(&key, &cli)? {
        filters.push(json!({ "field": "api_key_id", "operator": "eq", "value": target }));
    }

    let window = ranges::resolve(
        cli.range.as_deref(),
        cli.days,
        cli.from.as_deref(),
        cli.to.as_deref(),
    )?;

    let chosen = pick_metrics(cli.metrics.as_deref())?;

    // Error metrics must be queried separately from cost/volume metrics.
    let main_chosen: Vec<&Metric> = chosen
        .iter()
        .copied()
        .filter(|m| !metrics::is_error_metric(m.api))
        .collect();
    let err_chosen: Vec<&Metric> = chosen
        .iter()
        .copied()
        .filter(|m| metrics::is_error_metric(m.api))
        .collect();

    if !err_chosen.is_empty() {
        let bad: Vec<&str> = query_dims
            .iter()
            .map(String::as_str)
            .filter(|d| !metrics::ERROR_DIMS.contains(d))
            .collect();
        if !bad.is_empty() {
            bail!(
                "error metrics (--metrics errors) cannot be grouped by: {}\n  allowed: {}",
                bad.join(", "),
                metrics::ERROR_DIMS.join(", ")
            );
        }
    }

    let needs_chunk = query_dims.iter().any(|d| metrics::is_generations_only(d));
    let windows = if needs_chunk {
        ranges::chunk(window.start, window.end, 31)
    } else {
        vec![(window.start, window.end)]
    };

    let mut all_rows: Vec<Value> = Vec::new();
    let mut truncated = false;

    for group in [&main_chosen, &err_chosen] {
        if group.is_empty() {
            continue;
        }
        let api_names: Vec<&str> = group.iter().map(|m| m.api).collect();
        for (win_start, win_end) in &windows {
            let payload = api::run_query(
                &key,
                &api_names,
                &query_dims,
                &ranges::iso(*win_start),
                &ranges::iso(*win_end),
                granularity.as_deref(),
                &filters,
                cli.limit,
            )?;
            if cli.debug {
                eprintln!(
                    "--- window {} .. {} ---",
                    ranges::iso(*win_start),
                    ranges::iso(*win_end)
                );
                let text = serde_json::to_string_pretty(&payload).unwrap_or_default();
                eprintln!("{}", text.chars().take(2000).collect::<String>());
            }
            if payload
                .get("metadata")
                .and_then(|m| m.get("truncated"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                truncated = true;
            }
            all_rows.extend(format::rows_of(&payload));
        }
    }

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&all_rows)?);
        return Ok(());
    }

    if all_rows.is_empty() {
        println!("No usage found for {}.", window.label);
        if !filters.is_empty() {
            println!("(filtered to one API key — try without --key to confirm traffic exists)");
        }
        return Ok(());
    }

    print_table(&all_rows, &chosen, &dimensions, &window, &cli, truncated);
    Ok(())
}

/// Resolve `--key` / `--key-hash` into a single hash to filter on.
fn key_filter(key: &str, cli: &Cli) -> Result<Option<String>> {
    if let Some(hash) = &cli.key_hash {
        return Ok(Some(hash.clone()));
    }
    match &cli.key {
        Some(name) => Ok(Some(api::resolve_key(key, name)?)),
        None => Ok(None),
    }
}

/// Resolve `--metrics` into concrete metrics.
fn pick_metrics(spec: Option<&str>) -> Result<Vec<&'static Metric>> {
    let names: Vec<String> = match spec {
        None => metrics::DEFAULT_METRICS.iter().map(|s| s.to_string()).collect(),
        Some(s) if s.trim().eq_ignore_ascii_case("all") => {
            metrics::METRICS.iter().map(|m| m.name.to_string()).collect()
        }
        Some(s) => s
            .split(',')
            .map(|p| p.trim().to_lowercase())
            .filter(|p| !p.is_empty())
            .collect(),
    };

    let mut out = Vec::new();
    for name in names {
        match metrics::find(&name) {
            Some(metric) => out.push(metric),
            None => bail!("unknown metric {name:?}. Use --list-metrics to see options."),
        }
    }
    Ok(out)
}

fn print_metric_list() {
    println!("Available metrics (--metrics a,b,c or --metrics all):\n");
    let width = metrics::METRICS.iter().map(|m| m.name.len()).max().unwrap_or(0);
    for metric in metrics::METRICS {
        println!("  {:width$}  {}", metric.name, metric.api, width = width);
    }
    println!("\nTime buckets for --by: {}", metrics::BUCKETS.join(", "));
    let mut dims: Vec<&str> = metrics::GENERATIONS_ONLY
        .iter()
        .chain(metrics::PLAIN_DIMS.iter())
        .copied()
        .collect();
    dims.sort_unstable();
    dims.dedup();
    println!("Dimensions for --by: {}", dims.join(", "));
}

fn print_keys(key: &str, as_json: bool) -> Result<()> {
    let keys = api::list_keys(key)?;
    if keys.is_empty() {
        bail!("no keys returned");
    }
    if as_json {
        println!("{}", serde_json::to_string_pretty(&keys)?);
        return Ok(());
    }

    let name_of = |e: &Value| {
        e.get("name")
            .and_then(Value::as_str)
            .unwrap_or("(unnamed)")
            .to_string()
    };
    let width = keys.iter().map(|e| name_of(e).len()).max().unwrap_or(4).max(4);

    println!(
        "{:width$}  {:>10}  {:>10}  {:>10}  {:>10}",
        "NAME",
        "USAGE",
        "TODAY",
        "WEEK",
        "MONTH",
        width = width
    );
    for entry in &keys {
        let cell = |field: &str| format::money_grouped(format::num(entry.get(field)));
        println!(
            "{:width$}  {:>10}  {:>10}  {:>10}  {:>10}",
            name_of(entry),
            cell("usage"),
            cell("usage_daily"),
            cell("usage_weekly"),
            cell("usage_monthly"),
            width = width
        );
    }
    Ok(())
}

/// One line of totals for the bar widget: 1h, today, 7d, 30d.
///
/// "today" is midnight-to-now in local time, not a rolling 24 hours, so the
/// number resets at midnight the way a daily budget should.
fn print_brief(key: &str, cli: &Cli) -> Result<()> {
    let now = chrono::Utc::now();
    let midnight = ranges::local_midnight();

    let windows = [
        ("1h", now - chrono::Duration::hours(1), now),
        ("today", midnight, now),
        ("7d", now - chrono::Duration::days(7), now),
        ("30d", now - chrono::Duration::days(30), now),
    ];

    let mut filters: Vec<Value> = Vec::new();
    if let Some(target) = key_filter(key, cli)? {
        filters.push(json!({ "field": "api_key_id", "operator": "eq", "value": target }));
    }

    let mut parts = Vec::new();
    for (label, start, end) in windows {
        let payload = api::run_query(
            key,
            &["total_usage"],
            &[],
            &ranges::iso(start),
            &ranges::iso(end),
            None,
            &filters,
            1,
        )?;
        let total: f64 = format::rows_of(&payload)
            .iter()
            .map(|row| format::num(row.get("total_usage")))
            .sum();
        parts.push(format!("{label}={total:.4}"));
    }

    println!("{}", parts.join(" "));
    Ok(())
}

fn print_table(
    rows: &[Value],
    chosen: &[&Metric],
    dimensions: &[String],
    window: &ranges::Window,
    cli: &Cli,
    truncated: bool,
) {
    // Group rows by their dimension label, summing additive metrics and
    // averaging rates.
    let mut totals: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut counts: BTreeMap<String, Vec<usize>> = BTreeMap::new();

    for row in rows {
        let label = if dimensions.is_empty() {
            "total".to_string()
        } else {
            dimensions
                .iter()
                .map(|d| cell(row, d))
                .collect::<Vec<_>>()
                .join(" / ")
        };
        let sums = totals
            .entry(label.clone())
            .or_insert_with(|| vec![0.0; chosen.len()]);
        let seen = counts
            .entry(label)
            .or_insert_with(|| vec![0; chosen.len()]);

        for (idx, metric) in chosen.iter().enumerate() {
            let value = format::num(row.get(metric.api));
            sums[idx] += value;
            if !metric.additive && value != 0.0 {
                seen[idx] += 1;
            }
        }
    }

    // Convert accumulated sums into averages for non-additive metrics.
    for (label, sums) in totals.iter_mut() {
        let seen = &counts[label];
        for (idx, metric) in chosen.iter().enumerate() {
            if !metric.additive && seen[idx] > 0 {
                sums[idx] /= seen[idx] as f64;
            }
        }
    }

    let sort_idx = 0;
    let mut ordered: Vec<(&String, &Vec<f64>)> = totals.iter().collect();
    ordered.sort_by(|a, b| {
        b.1[sort_idx]
            .partial_cmp(&a.1[sort_idx])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let header_names: Vec<String> = chosen.iter().map(|m| m.name.to_uppercase()).collect();
    let widths: Vec<usize> = chosen
        .iter()
        .map(|m| if m.kind == Kind::Money { 12 } else { 14 })
        .collect();

    let label_width = ordered
        .iter()
        .map(|(k, _)| k.len())
        .max()
        .unwrap_or(5)
        .max(if dimensions.is_empty() { 5 } else { dimensions.join(" / ").len() });

    println!();
    println!("OpenRouter — {}, by {}", window.label, dimensions.join(", "));
    println!(
        "{}  ->  {}",
        window.start.format("%Y-%m-%d %H:%M UTC"),
        window.end.format("%Y-%m-%d %H:%M UTC")
    );
    if !cli.key.is_none() || !cli.key_hash.is_none() {
        let which = cli.key.clone().or_else(|| cli.key_hash.clone()).unwrap_or_default();
        println!("filtered to API key: {which}");
    }
    println!();

    let mut header = format!("{:<width$}", "GROUP", width = label_width);
    for (name, width) in header_names.iter().zip(&widths) {
        header.push_str(&format!("  {:>width$}", name, width = width));
    }
    println!("{header}");

    let mut rule = "-".repeat(label_width);
    for width in &widths {
        rule.push_str(&format!("  {}", "-".repeat(*width)));
    }
    println!("{rule}");

    for (label, values) in &ordered {
        let mut line = format!("{:<width$}", label, width = label_width);
        for (idx, metric) in chosen.iter().enumerate() {
            line.push_str(&format!(
                "  {:>width$}",
                format::format(metric.kind, values[idx]),
                width = widths[idx]
            ));
        }
        println!("{line}");
    }
    println!("{rule}");

    let mut total_line = format!("{:<width$}", "TOTAL", width = label_width);
    for (idx, metric) in chosen.iter().enumerate() {
        let aggregate = if metric.additive {
            totals.values().map(|v| v[idx]).sum()
        } else {
            let vals: Vec<f64> = totals.values().map(|v| v[idx]).filter(|v| *v != 0.0).collect();
            if vals.is_empty() {
                0.0
            } else {
                vals.iter().sum::<f64>() / vals.len() as f64
            }
        };
        total_line.push_str(&format!(
            "  {:>width$}",
            format::format(metric.kind, aggregate),
            width = widths[idx]
        ));
    }
    println!("{total_line}");
    println!();

    if truncated {
        println!("warning: result hit the row limit; totals are partial. Raise --limit or narrow the range.");
    }
    println!("Note: per-chat attribution is unavailable — Cursor does not forward a session id.");
}

/// Read a dimension value out of a row, falling back across date field names.
fn cell(row: &Value, dim: &str) -> String {
    if metrics::is_bucket(dim) {
        for field in [
            "date",
            "day",
            "date__day",
            "date__hour",
            "date__week",
            "date__month",
        ] {
            if let Some(value) = row.get(field) {
                if !value.is_null() {
                    return match value {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                }
            }
        }
        return "—".to_string();
    }
    match row.get(dim) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => "—".to_string(),
        Some(other) => other.to_string(),
    }
}
