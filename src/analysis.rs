use anyhow::{bail, Context, Result};
use chrono::{Duration, NaiveDateTime, NaiveTime};
use litchi::iwa::Document;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Long,
    Short,
}

#[derive(Debug, Clone)]
struct Candle {
    time: NaiveDateTime,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitReason {
    Tp,
    Sl,
    Time,
}

#[derive(Debug, Serialize)]
pub struct TradeReport {
    pub entry_time: String,
    pub exit_time: String,
    pub direction: Direction,
    pub entry_price: f64,
    pub exit_price: f64,
    pub sl: f64,
    pub risk: f64,
    pub r_multiple: f64,
    pub exit_reason: ExitReason,
    pub x_time: String,
    pub x_direction: Direction,
    pub hold_minutes: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct Skipped {
    pub outside_window: usize,
    pub insufficient_history: usize,
    pub invalid_risk: usize,
    pub invalid_same_candle: usize,
    pub no_exit: usize,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub candles: usize,
    pub timeframe_minutes: Option<i64>,
    pub trades: usize,
    pub wins: usize,
    pub losses: usize,
    pub breakeven: usize,
    pub win_rate: f64,
    pub total_r: f64,
    pub avg_r: f64,
    pub avg_win_r: f64,
    pub avg_loss_r: f64,
    pub profit_factor: Option<f64>,
    pub max_drawdown_r: f64,
    pub max_consecutive_wins: usize,
    pub max_consecutive_losses: usize,
    pub tp_exits: usize,
    pub sl_exits: usize,
    pub time_exits: usize,
    pub avg_hold_minutes: f64,
    pub start_time: String,
    pub end_time: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub summary: Summary,
    pub skipped: Skipped,
    pub trades: Vec<TradeReport>,
}

pub fn analyze_input(bytes: &[u8], file_name: Option<&str>) -> Result<Report> {
    let csv_bytes = if is_numbers_file(file_name) {
        numbers_to_csv(bytes)?
    } else {
        bytes.to_vec()
    };

    analyze_csv(&csv_bytes)
}

pub fn analyze_csv(bytes: &[u8]) -> Result<Report> {
    let mut candles = parse_csv(bytes)?;
    if candles.is_empty() {
        bail!("No valid rows parsed from input.");
    }
    candles.sort_by_key(|c| c.time);

    let timeframe_minutes = infer_timeframe_minutes(&candles);
    let x_dirs = compute_x_dirs(&candles);

    let (mut trades, skipped) = simulate_trades(&candles, &x_dirs);
    trades.sort_by(|a, b| a.entry_time.cmp(&b.entry_time));

    let summary = summarize(&candles, timeframe_minutes, &trades);
    Ok(Report {
        summary,
        skipped,
        trades,
    })
}

fn parse_csv(bytes: &[u8]) -> Result<Vec<Candle>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(bytes);

    let mut candles = Vec::new();
    for result in rdr.records() {
        let record = match result {
            Ok(record) => record,
            Err(_) => continue,
        };
        if record.len() < 5 {
            continue;
        }

        let raw_time = record.get(0).unwrap_or("").trim();
        if raw_time.is_empty() || raw_time.starts_with("Downloaded from") {
            continue;
        }

        let time_str = raw_time.trim_matches('"');
        let parsed_time = match parse_time(time_str) {
            Some(t) => t,
            None => continue,
        };
        let time = parsed_time + Duration::hours(1);

        let open = match parse_f64(record.get(1)) {
            Some(v) => v,
            None => continue,
        };
        let high = match parse_f64(record.get(2)) {
            Some(v) => v,
            None => continue,
        };
        let low = match parse_f64(record.get(3)) {
            Some(v) => v,
            None => continue,
        };
        let close = match parse_f64(record.get(4)) {
            Some(v) => v,
            None => continue,
        };

        candles.push(Candle {
            time,
            open,
            high,
            low,
            close,
        });
    }

    Ok(candles)
}

fn parse_time(raw: &str) -> Option<NaiveDateTime> {
    let raw = raw.trim();
    let formats = [
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
    ];
    for fmt in formats {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(raw, fmt) {
            return Some(parsed);
        }
    }
    None
}

fn is_numbers_file(file_name: Option<&str>) -> bool {
    let Some(name) = file_name else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".numbers") || lower.ends_with(".zip")
}

fn numbers_to_csv(bytes: &[u8]) -> Result<Vec<u8>> {
    if !looks_like_zip(bytes) {
        bail!(
            "This .numbers file looks like a package directory. Export as CSV or compress it to .zip first."
        );
    }

    let doc = Document::from_bytes(bytes).context("failed to parse .numbers file")?;
    let structured = doc
        .extract_structured_data()
        .context("failed to extract tables from .numbers")?;

    if structured.tables.is_empty() {
        bail!("No tables found in .numbers file.");
    }

    let mut best_csv: Option<Vec<u8>> = None;
    let mut best_len = 0usize;

    for table in structured.tables {
        let csv = table.to_csv();
        let candles = parse_csv(csv.as_bytes()).unwrap_or_default();
        let len = candles.len();
        if len > best_len {
            best_len = len;
            best_csv = Some(csv.into_bytes());
        }
    }

    if let Some(csv) = best_csv {
        Ok(csv)
    } else {
        bail!("No valid table found in .numbers file.")
    }
}

fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

fn parse_f64(value: Option<&str>) -> Option<f64> {
    let raw = value?.trim().trim_matches('"');
    if raw.is_empty() {
        return None;
    }

    let cleaned = raw.replace(',', "");
    cleaned.parse::<f64>().ok()
}

fn compute_x_dirs(candles: &[Candle]) -> Vec<Option<Direction>> {
    let mut x_dirs = vec![None; candles.len()];
    for i in 1..candles.len() {
        let prev = &candles[i - 1];
        let curr = &candles[i];

        let prev_low = prev.open.min(prev.close);
        let prev_high = prev.open.max(prev.close);
        let curr_low = curr.open.min(curr.close);
        let curr_high = curr.open.max(curr.close);

        if curr_low < prev_low && curr_high > prev_high {
            if curr.close > curr.open {
                x_dirs[i] = Some(Direction::Long);
            } else if curr.close < curr.open {
                x_dirs[i] = Some(Direction::Short);
            }
        }
    }

    x_dirs
}

fn simulate_trades(
    candles: &[Candle],
    x_dirs: &[Option<Direction>],
) -> (Vec<TradeReport>, Skipped) {
    let mut trades = Vec::new();
    let mut skipped = Skipped::default();

    for i in 1..candles.len() {
        let Some(x_dir) = x_dirs[i] else {
            continue;
        };
        let entry_idx = i + 1;
        if entry_idx >= candles.len() {
            continue;
        }

        let entry_time = candles[entry_idx].time;
        if !is_entry_window(entry_time) {
            skipped.outside_window += 1;
            continue;
        }
        if entry_idx < 3 {
            skipped.insufficient_history += 1;
            continue;
        }

        let entry_price = candles[entry_idx].open;
        let (sl, risk) = match x_dir {
            Direction::Long => {
                let mut min_low = f64::INFINITY;
                for j in (entry_idx - 3)..entry_idx {
                    min_low = min_low.min(candles[j].low);
                }
                (min_low, entry_price - min_low)
            }
            Direction::Short => {
                let mut max_high = f64::NEG_INFINITY;
                for j in (entry_idx - 3)..entry_idx {
                    max_high = max_high.max(candles[j].high);
                }
                (max_high, max_high - entry_price)
            }
        };

        if risk <= 0.0 {
            skipped.invalid_risk += 1;
            continue;
        }

        let entry_date = entry_time.date();
        let target_time = NaiveDateTime::new(
            entry_date,
            NaiveTime::from_hms_opt(14, 0, 0).unwrap(),
        );
        let mut closed = false;
        let mut invalid = false;
        let mut exit_reason = ExitReason::Time;
        let mut exit_time = entry_time;
        let mut exit_price = entry_price;

        for j in entry_idx..candles.len() {
            let candle = &candles[j];
            if candle.time.date() > entry_date {
                break;
            }

            if candle.time >= target_time {
                exit_time = candle.time;
                exit_price = candle.open;
                exit_reason = ExitReason::Time;
                closed = true;
                break;
            }

            let sl_hit = match x_dir {
                Direction::Long => candle.low < sl,
                Direction::Short => candle.high > sl,
            };

            let opp_x = match x_dirs[j] {
                Some(dir) => dir != x_dir,
                None => false,
            };

            if sl_hit && opp_x {
                // Same candle: SL hit and opposite X close -> invalidate trade.
                skipped.invalid_same_candle += 1;
                invalid = true;
                closed = true;
                break;
            }

            if sl_hit {
                exit_time = candle.time;
                exit_price = sl;
                exit_reason = ExitReason::Sl;
                closed = true;
                break;
            }

            if opp_x {
                exit_time = candle.time;
                exit_price = candle.close;
                exit_reason = ExitReason::Tp;
                closed = true;
                break;
            }
        }

        if !closed {
            skipped.no_exit += 1;
            continue;
        }
        if invalid {
            continue;
        }

        let r_multiple = match x_dir {
            Direction::Long => (exit_price - entry_price) / risk,
            Direction::Short => (entry_price - exit_price) / risk,
        };

        let hold_minutes = (exit_time - entry_time).num_minutes();

        trades.push(TradeReport {
            entry_time: fmt_time(entry_time),
            exit_time: fmt_time(exit_time),
            direction: x_dir,
            entry_price,
            exit_price,
            sl,
            risk,
            r_multiple,
            exit_reason,
            x_time: fmt_time(candles[i].time),
            x_direction: x_dir,
            hold_minutes,
        });
    }

    (trades, skipped)
}

fn is_entry_window(time: NaiveDateTime) -> bool {
    let t = time.time();
    let start = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let end = NaiveTime::from_hms_opt(11, 30, 0).unwrap();
    t >= start && t <= end
}

fn summarize(candles: &[Candle], timeframe_minutes: Option<i64>, trades: &[TradeReport]) -> Summary {
    let mut wins = 0;
    let mut losses = 0;
    let mut breakeven = 0;
    let mut sum_wins = 0.0;
    let mut sum_losses = 0.0;
    let mut total_r = 0.0;

    let mut tp_exits = 0;
    let mut sl_exits = 0;
    let mut time_exits = 0;

    let mut consecutive_wins = 0;
    let mut consecutive_losses = 0;
    let mut max_consecutive_wins = 0;
    let mut max_consecutive_losses = 0;

    let mut equity = 0.0;
    let mut peak = 0.0;
    let mut max_drawdown = 0.0;

    let mut total_hold = 0.0;

    for trade in trades {
        let r = trade.r_multiple;
        total_r += r;

        match trade.exit_reason {
            ExitReason::Tp => tp_exits += 1,
            ExitReason::Sl => sl_exits += 1,
            ExitReason::Time => time_exits += 1,
        }

        if r > 0.0 {
            wins += 1;
            sum_wins += r;
            consecutive_wins += 1;
            consecutive_losses = 0;
            if consecutive_wins > max_consecutive_wins {
                max_consecutive_wins = consecutive_wins;
            }
        } else if r < 0.0 {
            losses += 1;
            sum_losses += r;
            consecutive_losses += 1;
            consecutive_wins = 0;
            if consecutive_losses > max_consecutive_losses {
                max_consecutive_losses = consecutive_losses;
            }
        } else {
            breakeven += 1;
            consecutive_wins = 0;
            consecutive_losses = 0;
        }

        equity += r;
        if equity > peak {
            peak = equity;
        }
        let drawdown = equity - peak;
        if drawdown < max_drawdown {
            max_drawdown = drawdown;
        }

        total_hold += trade.hold_minutes as f64;
    }

    let trades_count = trades.len();
    let win_rate = if trades_count > 0 {
        wins as f64 / trades_count as f64
    } else {
        0.0
    };
    let avg_r = if trades_count > 0 {
        total_r / trades_count as f64
    } else {
        0.0
    };

    let avg_win_r = if wins > 0 { sum_wins / wins as f64 } else { 0.0 };
    let avg_loss_r = if losses > 0 { sum_losses / losses as f64 } else { 0.0 };

    let profit_factor = if sum_losses.abs() > 0.0 {
        Some(sum_wins / sum_losses.abs())
    } else {
        None
    };

    let avg_hold_minutes = if trades_count > 0 {
        total_hold / trades_count as f64
    } else {
        0.0
    };

    let (start_time, end_time) = if candles.is_empty() {
        (String::new(), String::new())
    } else {
        (fmt_time(candles[0].time), fmt_time(candles[candles.len() - 1].time))
    };

    Summary {
        candles: candles.len(),
        timeframe_minutes,
        trades: trades_count,
        wins,
        losses,
        breakeven,
        win_rate,
        total_r,
        avg_r,
        avg_win_r,
        avg_loss_r,
        profit_factor,
        max_drawdown_r: max_drawdown,
        max_consecutive_wins,
        max_consecutive_losses,
        tp_exits,
        sl_exits,
        time_exits,
        avg_hold_minutes,
        start_time,
        end_time,
    }
}

fn infer_timeframe_minutes(candles: &[Candle]) -> Option<i64> {
    if candles.len() < 2 {
        return None;
    }

    let mut deltas = Vec::new();
    for i in 1..candles.len() {
        let delta = candles[i].time - candles[i - 1].time;
        let minutes = delta.num_minutes();
        if minutes > 0 {
            deltas.push(minutes);
        }
    }

    if deltas.is_empty() {
        return None;
    }

    deltas.sort_unstable();
    let mid = deltas.len() / 2;
    Some(deltas[mid])
}

fn fmt_time(dt: NaiveDateTime) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}
