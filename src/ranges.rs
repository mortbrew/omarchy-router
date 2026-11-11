//! Turning `--range` / `--days` / `--from`+`--to` into concrete UTC windows.

use anyhow::{bail, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};

/// A resolved query window.
pub struct Window {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub label: String,
}

/// Format an instant the way the analytics API expects it.
pub fn iso(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Midnight today in the user's local timezone, expressed in UTC.
///
/// Day boundaries follow local midnight, not UTC: "today" should reset when the
/// user's day does, and the API accepts any UTC instant.
pub fn local_midnight() -> DateTime<Utc> {
    let local = Local::now();
    let naive = local.date_naive().and_hms_opt(0, 0, 0).expect("valid midnight");
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&naive))
}

/// Resolve the requested range.
pub fn resolve(
    range: Option<&str>,
    days: Option<i64>,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Window> {
    let now = Utc::now();

    if from.is_some() || to.is_some() {
        let (from, to) = match (from, to) {
            (Some(f), Some(t)) => (f, t),
            _ => bail!("--from and --to must be used together"),
        };
        let start = parse_date(from)?;
        let end = parse_date(to)?;
        if end <= start {
            bail!("--to must be after --from");
        }
        return Ok(Window {
            start,
            end,
            label: format!("{from} to {to}"),
        });
    }

    if let Some(n) = days {
        return Ok(Window {
            start: now - Duration::days(n),
            end: now,
            label: format!("last {n} day(s)"),
        });
    }

    let rng = range.unwrap_or("1w").to_lowercase();
    let today = local_midnight();

    let fixed = match rng.as_str() {
        "15m" => Some(Duration::minutes(15)),
        "30m" => Some(Duration::minutes(30)),
        "1h" => Some(Duration::hours(1)),
        "3h" => Some(Duration::hours(3)),
        "24h" => Some(Duration::hours(24)),
        "48h" => Some(Duration::hours(48)),
        "7d" => Some(Duration::days(7)),
        "30d" => Some(Duration::days(30)),
        "1w" => Some(Duration::weeks(1)),
        "1mo" => Some(Duration::days(30)),
        "1y" => Some(Duration::days(365)),
        _ => None,
    };
    if let Some(delta) = fixed {
        return Ok(Window {
            start: now - delta,
            end: now,
            label: format!("past {rng}"),
        });
    }

    let weekday = Local::now().weekday().num_days_from_monday() as i64;

    let (start, end, label) = match rng.as_str() {
        "today" => (today, now, "today".to_string()),
        "yesterday" => (today - Duration::days(1), today, "yesterday".to_string()),
        "this-week" => (today - Duration::days(weekday), now, "this week".to_string()),
        "prev-week" => {
            let start = today - Duration::days(weekday + 7);
            (start, start + Duration::days(7), "previous week".to_string())
        }
        "this-month" => (first_of_month(today), now, "this month".to_string()),
        "prev-month" => {
            let first = first_of_month(today);
            let prev_last = first - Duration::days(1);
            (first_of_month(prev_last), first, "previous month".to_string())
        }
        "this-year" => (first_of_year(today), now, "this year".to_string()),
        "prev-year" => {
            let first = first_of_year(today);
            let prev = first.with_year(first.year() - 1).unwrap_or(first);
            (prev, first, "previous year".to_string())
        }
        other => bail!(
            "unknown --range {other:?}. Try 15m, 1h, 24h, 1w, 1mo, today, this-week, this-month."
        ),
    };

    Ok(Window { start, end, label })
}

fn parse_date(text: &str) -> Result<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("--from/--to must be YYYY-MM-DD"))?;
    Ok(Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("valid midnight")))
}

fn first_of_month(dt: DateTime<Utc>) -> DateTime<Utc> {
    let naive = dt.date_naive();
    NaiveDate::from_ymd_opt(naive.year(), naive.month(), 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|d| Utc.from_utc_datetime(&d))
        .unwrap_or(dt)
}

fn first_of_year(dt: DateTime<Utc>) -> DateTime<Utc> {
    let naive = dt.date_naive();
    NaiveDate::from_ymd_opt(naive.year(), 1, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|d| Utc.from_utc_datetime(&d))
        .unwrap_or(dt)
}

/// Split a range so no window exceeds `max_days`.
pub fn chunk(start: DateTime<Utc>, end: DateTime<Utc>, max_days: i64) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut out = Vec::new();
    let mut cursor = start;
    while cursor < end {
        let next = (cursor + Duration::days(max_days)).min(end);
        out.push((cursor, next));
        cursor = next;
    }
    if out.is_empty() {
        out.push((start, end));
    }
    out
}
