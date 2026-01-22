# nuked

Rust web app that analyzes EURUSD 15m data using a body-only "X candle" strategy.
Users upload CSV or Apple Numbers files; the server converts UTC-6 to UTC-5,
simulates entries/exits, and returns an R-based report plus trade list.

## Inputs
- CSV: `Time,Open,High,Low,Latest,...` (Latest is treated as Close).
- Numbers: `.numbers` or `.zip` exported from Numbers (tables are auto-detected).
- Timezone: input is UTC-6, converted to UTC-5 for analysis.

## Strategy Summary
- X candle: current body strictly covers previous body (no equality).
- Entry: next candle open after X candle.
- Direction: X candle closes up = long, closes down = short.
- SL: last 3 candles before entry (wick included), must be strictly broken to trigger.
- TP: next opposite-direction X candle close (same-direction X does not close).
- Entry window: 00:00 to 11:30 (UTC-5).
- Time exit: if still open, close at 14:00 open (UTC-5).
- Invalid trade: same candle hits SL and opposite X close.

## Output
- Summary metrics in R (total R, win rate, drawdown, etc.).
- Trade list with entry/exit, SL, R multiple, hold time, exit reason.

## Run Locally
```bash
cargo run
```
Open `http://127.0.0.1:3000` and upload a file.

## Deploy (Railway)
- Uses `PORT` env var and binds `0.0.0.0`.
- `nixpacks.toml` installs protobuf for Numbers parsing.
