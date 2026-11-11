# omarchy-router

Real dollar spend and usage from [OpenRouter](https://openrouter.ai), as a CLI and
as an [Omarchy](https://omarchy.org) bar widget.

Built for the Cursor + OpenRouter setup: Cursor's dashboard shows tokens but not
dollars, and OpenRouter holds the actual bill. This reads the bill.

```
$ omarchy-router --range today
OpenRouter — today, by model
2026-11-10 05:00 UTC  ->  2026-11-11 02:45 UTC

GROUP                                          COST          TOKENS        REQUESTS
-------------------------------------  ------------  --------------  --------------
deepseek/deepseek-v4.1-flash-20261017         $0.28      37,707,253             423
-------------------------------------  ------------  --------------  --------------
TOTAL                                         $0.28      37,707,253             423
```

## Install

Requires a Rust toolchain (`rustup` or `pacman -S rust`).

```bash
git clone https://github.com/mortbrew/omarchy-router
cd omarchy-router
cargo build --release
install -Dm755 target/release/omarchy-router ~/.local/bin/omarchy-router
```

## Credentials

Analytics requires an **OpenRouter Management Key**, not a regular API key.
Create one at <https://openrouter.ai/settings/management-keys>.

Then either export it:

```bash
export OPENROUTER_MANAGEMENT_KEY=sk-or-v1-...
```

or write it to `~/.config/openrouter-cost/key`:

```bash
mkdir -p ~/.config/openrouter-cost
cat > ~/.config/openrouter-cost/key <<'EOF'
# OpenRouter management key for omarchy-router.
# Create one at https://openrouter.ai/settings/management-keys
# PASTE_YOUR_MANAGEMENT_KEY_HERE
EOF
chmod 600 ~/.config/openrouter-cost/key
```

The file is parsed as "first non-blank, non-comment line", so you can paste the
key on its own line at the bottom and leave the comments in place.

## Usage

### Ranges

Mirrors the OpenRouter Activity page:

```bash
omarchy-router --range 1h
omarchy-router --range today          # local midnight -> now
omarchy-router --range this-week
omarchy-router --range prev-month
omarchy-router --days 14              # rolling window
omarchy-router --from 2026-11-01 --to 2026-11-10
```

Accepted: `15m`, `30m`, `1h`, `3h`, `24h`, `48h`, `7d`, `30d`, `1w`, `1mo`, `1y`,
`today`, `yesterday`, `this-week`, `prev-week`, `this-month`, `prev-month`,
`this-year`, `prev-year`.

Day boundaries follow **local midnight**, not UTC, so `today` resets when your
day does.

### Grouping

```bash
omarchy-router --by model             # default
omarchy-router --by api_key_id
omarchy-router --by day --metrics cost
omarchy-router --by model,day
```

Max 2 dimensions (an OpenRouter API cap). `minute`/`hour`/`day`/`week`/`month`
set the time bucket instead of a dimension.

### Metrics

```bash
omarchy-router --metrics cost,tokens,requests   # default
omarchy-router --metrics all
omarchy-router --list-metrics
```

`cost`, `tokens`, `prompt`, `completion`, `reasoning`, `cached`, `requests`,
`byok`, `credits`, `upstream`, `cache-cost`, `web`, `file`, `web-fetch`, `data`,
`cache-hit`, `latency`, `ttft`, `throughput`, `errors`, `blended`.

### Keys

```bash
omarchy-router --list-keys
omarchy-router --key cursor-omarchy --range today
```

### Other

```bash
omarchy-router --json      # raw rows
omarchy-router --debug     # dump raw API responses to stderr
omarchy-router --brief     # one line: 1h, today, 7d, 30d
omarchy-router --stats     # one line per horizon, full stat set
```

## Omarchy bar widget

`plugin/` holds a Quickshell bar widget that shows four horizons: the last hour,
today (from local midnight), the last 7 days, and the last 30 days.

```bash
mkdir -p ~/.config/omarchy/plugins/mortbrew.openrouter-cost
cp plugin/Cost.qml plugin/manifest.json ~/.config/omarchy/plugins/mortbrew.openrouter-cost/
```

Then add it to the bar in `~/.config/omarchy/shell.json`:

```json
{
  "id": "mortbrew.openrouter-cost",
  "intervalMinutes": 3
}
```

under `bar.layout.right`.

### Choosing which horizons show

**Left click** the widget to open a popup with the full breakdown per horizon —
total spend, requests, token volume, cache hit rate, and blended $/M tokens —
each with a toggle. The ticked ones are the ones the bar shows. The choice is
written back to `shell.json` as a `visible` setting, so it survives a restart:

```json
{
  "id": "mortbrew.openrouter-cost",
  "intervalMinutes": 3,
  "visible": "1h,today,7d,30d"
}
```

The last remaining horizon cannot be switched off, so the widget can never
render empty and strand the popup. An empty or unrecognised `visible` value
falls back to showing all four.

**Right click** refreshes immediately. The popup also carries a Refresh button
and shows the current interval.

The bar prefixes the numbers with DeepSeek's mark (`U+E911` from Omarchy's own
icon font), since the widget is built for a DeepSeek-only setup. Set
`"showIcon": false` to drop it, or `"showLabels": false` to hide the `1h` /
`today` / `7d` / `30d` labels.

### Refresh interval

The widget runs `omarchy-router --stats`, which issues **4 analytics queries**
(one per horizon). OpenRouter rate-limits the analytics endpoint at **64
requests/minute**, so even a 1-minute interval uses ~6% of the budget.

The real constraint is data freshness, not rate limits: OpenRouter's rollups lag
by tens of seconds, so polling faster than ~2 minutes mostly redraws identical
numbers. **2–5 minutes** is the sweet spot; the default is 3.

Analytics queries are read-only and consume no credits.

## Why per-chat cost is not possible

Cursor reaches OpenRouter through the "Override OpenAI Base URL" setting
(`https://openrouter.ai/api/v1/cursor`). It does not forward a chat or session
identifier, so OpenRouter files all Cursor traffic under `session_id: "none"`.

Per-key and per-model attribution is the finest granularity available.

## License

MIT
