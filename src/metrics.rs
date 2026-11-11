//! Metric and dimension tables.
//!
//! Friendly CLI names map to the raw OpenRouter analytics metric names. The
//! `Kind` drives how a value is formatted in the table output.

/// How a metric value should be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Money,
    Int,
    Pct,
    Ms,
    Rate,
}

/// A metric the user can ask for.
#[derive(Debug, Clone, Copy)]
pub struct Metric {
    /// Name used on the command line.
    pub name: &'static str,
    /// Name the analytics API expects.
    pub api: &'static str,
    /// How to format it.
    pub kind: Kind,
    /// Rates and averages must be averaged across rows, never summed.
    pub additive: bool,
}

const fn m(name: &'static str, api: &'static str, kind: Kind) -> Metric {
    Metric { name, api, kind, additive: true }
}

const fn avg(name: &'static str, api: &'static str, kind: Kind) -> Metric {
    Metric { name, api, kind, additive: false }
}

/// Every metric the CLI understands, in display order.
pub const METRICS: &[Metric] = &[
    m("cost", "total_usage", Kind::Money),
    m("tokens", "tokens_total", Kind::Int),
    m("prompt", "tokens_prompt", Kind::Int),
    m("completion", "tokens_completion", Kind::Int),
    m("reasoning", "reasoning_tokens", Kind::Int),
    m("cached", "cached_tokens", Kind::Int),
    m("requests", "request_count", Kind::Int),
    m("byok", "byok_usage", Kind::Money),
    m("credits", "credits_usage", Kind::Money),
    m("upstream", "usage_upstream", Kind::Money),
    m("cache-cost", "usage_cache", Kind::Money),
    m("web", "usage_web", Kind::Money),
    m("file", "usage_file", Kind::Money),
    m("web-fetch", "usage_web_fetch", Kind::Money),
    m("data", "usage_data", Kind::Money),
    avg("cache-hit", "cache_hit_rate", Kind::Pct),
    avg("latency", "avg_latency", Kind::Ms),
    avg("ttft", "avg_total_time_to_first_token", Kind::Ms),
    avg("throughput", "avg_throughput", Kind::Rate),
    m("errors", "gateway_5xx_count", Kind::Int),
    avg("blended", "blended_cost_per_million_tokens", Kind::Money),
];

/// Metrics used when `--metrics` is not given.
pub const DEFAULT_METRICS: &[&str] = &["cost", "tokens", "requests"];

/// Error metrics can only be queried alongside each other, and only with a
/// restricted set of dimensions. Mixing them with cost metrics returns HTTP 400.
pub const ERROR_METRICS: &[&str] = &["total_request_count", "gateway_5xx_count", "provider_5xx_count"];

/// Dimensions that error metrics may be grouped by.
pub const ERROR_DIMS: &[&str] = &["model", "api_key_id", "provider", "workspace", "user"];

/// Dimensions that force the 31-day query cap (generations-only dimensions).
pub const GENERATIONS_ONLY: &[&str] = &[
    "generation_id",
    "session_id",
    "provider",
    "origin",
    "country",
    "data_region",
    "skin",
    "finish_reason",
    "external_user",
    "context_length_bucket",
    "streamed",
];

/// Dimensions that are not generations-only, for help text.
pub const PLAIN_DIMS: &[&str] = &["model", "variant", "api_key_id", "workspace", "app", "user"];

/// Time-bucket keywords accepted in `--by`, mapped to the API granularity.
pub const BUCKETS: &[&str] = &["minute", "hour", "day", "week", "month"];

/// Look up a metric by its friendly name.
pub fn find(name: &str) -> Option<&'static Metric> {
    METRICS.iter().find(|metric| metric.name == name)
}

/// True when this API metric name is an error metric.
pub fn is_error_metric(api: &str) -> bool {
    ERROR_METRICS.contains(&api)
}

/// True when this dimension forces the 31-day cap.
pub fn is_generations_only(dim: &str) -> bool {
    GENERATIONS_ONLY.contains(&dim)
}

/// True when this token is a time bucket rather than a dimension.
pub fn is_bucket(token: &str) -> bool {
    BUCKETS.contains(&token)
}
