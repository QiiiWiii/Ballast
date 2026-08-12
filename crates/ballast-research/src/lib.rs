#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use chrono_tz::America::New_York;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const BPS: Decimal = Decimal::from_parts(10_000, 0, 0, false, 0);
const WINDOW_BUCKETS: usize = 18;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinuteBar {
    pub timestamp: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub open: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub high: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub low: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub close: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub volume: Decimal,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub vwap: Option<Decimal>,
    pub trade_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "algorithm", rename_all = "snake_case")]
pub enum StrategyConfig {
    Immediate,
    Twap {
        slice_interval_seconds: u32,
    },
    Pov {
        #[serde(with = "rust_decimal::serde::str")]
        participation_rate: Decimal,
    },
    Vwap {
        lookback_sessions: u32,
        bucket_interval_seconds: u32,
    },
}

impl StrategyConfig {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Twap { .. } => "twap",
            Self::Pov { .. } => "pov",
            Self::Vwap { .. } => "vwap",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactScenario {
    Low,
    Base,
    High,
}

impl ImpactScenario {
    const fn multiplier(self) -> Decimal {
        match self {
            Self::Low => Decimal::from_parts(5, 0, 0, false, 1),
            Self::Base => Decimal::ONE,
            Self::High => Decimal::TWO,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationCase {
    pub symbol: String,
    pub side: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub target_notional_usd: Decimal,
    pub timezone: String,
    pub start_time: String,
    pub end_time: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub max_participation_rate: Decimal,
    pub warmup_sessions: usize,
    pub evaluation_sessions: usize,
}

impl ValidationCase {
    #[must_use]
    pub fn tsla_default() -> Self {
        Self {
            symbol: "TSLA".to_owned(),
            side: "sell".to_owned(),
            target_notional_usd: Decimal::from(10_000_000),
            timezone: "America/New_York".to_owned(),
            start_time: "10:00:00".to_owned(),
            end_time: "11:30:00".to_owned(),
            max_participation_rate: Decimal::from_parts(1, 0, 0, false, 1),
            warmup_sessions: 20,
            evaluation_sessions: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayPoint {
    pub bucket: usize,
    pub timestamp: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub market_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub market_volume: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub planned_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub filled_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub cumulative_quantity: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub scenario: ImpactScenario,
    #[serde(with = "rust_decimal::serde::str")]
    pub average_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub implementation_shortfall_bps: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub interval_vwap_slippage_bps: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateSessionResult {
    pub strategy: String,
    pub completed: bool,
    pub completion_bucket: Option<usize>,
    #[serde(with = "rust_decimal::serde::str")]
    pub target_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub filled_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub maximum_participation_rate: Decimal,
    pub constraint_violations: Vec<String>,
    pub scenarios: Vec<ScenarioResult>,
    pub replay: Vec<ReplayPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionResult {
    pub date: NaiveDate,
    #[serde(with = "rust_decimal::serde::str")]
    pub arrival_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub interval_vwap: Decimal,
    pub candidates: Vec<CandidateSessionResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateResult {
    pub strategy: String,
    pub evaluated_sessions: usize,
    pub completed_sessions: usize,
    pub violation_count: usize,
    #[serde(with = "rust_decimal::serde::str")]
    pub median_shortfall_bps: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub p75_shortfall_bps: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub p95_shortfall_bps: Decimal,
    pub eligible_for_validation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub provider: String,
    pub dataset: String,
    pub data_quality: String,
    pub case: ValidationCase,
    pub strategies: Vec<StrategyConfig>,
    pub invalid_sessions: BTreeMap<NaiveDate, String>,
    pub sessions: Vec<SessionResult>,
    pub aggregates: Vec<AggregateResult>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResearchError {
    #[error("validation case is invalid: {0}")]
    InvalidCase(&'static str),
    #[error("strategy configuration is invalid: {0}")]
    InvalidStrategy(&'static str),
    #[error("not enough valid sessions: required {required}, found {found}")]
    NotEnoughSessions { required: usize, found: usize },
    #[error("session {0} has no complete validation window")]
    IncompleteSession(NaiveDate),
}

#[derive(Debug, Clone)]
struct Bucket {
    timestamp: DateTime<Utc>,
    open: Decimal,
    close: Decimal,
    volume: Decimal,
    vwap: Decimal,
}

pub fn validate(
    case: ValidationCase,
    strategies: Vec<StrategyConfig>,
    bars: Vec<MinuteBar>,
) -> Result<ValidationReport, ResearchError> {
    validate_case(&case)?;
    for strategy in &strategies {
        validate_strategy(strategy)?;
    }
    if !strategies
        .iter()
        .any(|value| matches!(value, StrategyConfig::Immediate))
    {
        return Err(ResearchError::InvalidStrategy(
            "immediate benchmark is required",
        ));
    }

    let mut grouped: BTreeMap<NaiveDate, Vec<MinuteBar>> = BTreeMap::new();
    for bar in bars {
        let local = bar.timestamp.with_timezone(&New_York);
        grouped.entry(local.date_naive()).or_default().push(bar);
    }

    let mut valid = Vec::new();
    let mut invalid_sessions = BTreeMap::new();
    for (date, mut session_bars) in grouped {
        session_bars.sort_by_key(|bar| bar.timestamp);
        match build_window_buckets(date, &session_bars) {
            Ok(buckets) => valid.push((date, session_bars, buckets)),
            Err(error) => {
                invalid_sessions.insert(date, error.to_string());
            }
        }
    }

    let required = case.warmup_sessions + case.evaluation_sessions;
    if valid.len() < required {
        return Err(ResearchError::NotEnoughSessions {
            required,
            found: valid.len(),
        });
    }
    let valid = valid.split_off(valid.len() - required);
    let mut sessions = Vec::with_capacity(case.evaluation_sessions);
    for index in case.warmup_sessions..valid.len() {
        let history = &valid[index - case.warmup_sessions..index];
        sessions.push(simulate_session(
            &case,
            &strategies,
            history,
            &valid[index],
        )?);
    }
    let aggregates = aggregate(&sessions, &strategies);
    Ok(ValidationReport {
        provider: "alpaca".to_owned(),
        dataset: "iex_1min_bars".to_owned(),
        data_quality: "iex_proxy".to_owned(),
        case,
        strategies,
        invalid_sessions,
        sessions,
        aggregates,
    })
}

fn validate_case(case: &ValidationCase) -> Result<(), ResearchError> {
    if case.symbol.trim().is_empty() || case.side != "sell" {
        return Err(ResearchError::InvalidCase(
            "only a named sell case is supported",
        ));
    }
    if case.target_notional_usd <= Decimal::ZERO {
        return Err(ResearchError::InvalidCase(
            "target notional must be positive",
        ));
    }
    if case.max_participation_rate <= Decimal::ZERO || case.max_participation_rate > Decimal::ONE {
        return Err(ResearchError::InvalidCase(
            "participation rate must be in (0, 1]",
        ));
    }
    if case.timezone != "America/New_York"
        || case.start_time != "10:00:00"
        || case.end_time != "11:30:00"
    {
        return Err(ResearchError::InvalidCase(
            "research lab supports the fixed TSLA 10:00-11:30 ET window",
        ));
    }
    // Warmup stays fixed so the immutable VWAP lookback template remains valid.
    if case.warmup_sessions != 20 {
        return Err(ResearchError::InvalidCase(
            "research lab requires 20 warmup sessions",
        ));
    }
    if case.evaluation_sessions == 0 || case.evaluation_sessions > 200 {
        return Err(ResearchError::InvalidCase(
            "evaluation sessions must be in 1..=200",
        ));
    }
    Ok(())
}

fn validate_strategy(strategy: &StrategyConfig) -> Result<(), ResearchError> {
    match strategy {
        StrategyConfig::Immediate => Ok(()),
        StrategyConfig::Twap {
            slice_interval_seconds,
        } if *slice_interval_seconds == 300 => Ok(()),
        StrategyConfig::Pov { participation_rate }
            if *participation_rate > Decimal::ZERO && *participation_rate <= Decimal::ONE =>
        {
            Ok(())
        }
        StrategyConfig::Vwap {
            lookback_sessions,
            bucket_interval_seconds,
        } if *lookback_sessions == 20 && *bucket_interval_seconds == 300 => Ok(()),
        StrategyConfig::Twap { .. } => Err(ResearchError::InvalidStrategy(
            "v1 TWAP interval must be 5 minutes",
        )),
        StrategyConfig::Pov { .. } => {
            Err(ResearchError::InvalidStrategy("POV rate must be in (0, 1]"))
        }
        StrategyConfig::Vwap { .. } => Err(ResearchError::InvalidStrategy(
            "v1 VWAP requires 20 sessions and 5-minute buckets",
        )),
    }
}

fn build_window_buckets(date: NaiveDate, bars: &[MinuteBar]) -> Result<Vec<Bucket>, ResearchError> {
    let start = NaiveTime::from_hms_opt(10, 0, 0).expect("valid time");
    let end = NaiveTime::from_hms_opt(11, 30, 0).expect("valid time");
    let mut minute_bars = bars
        .iter()
        .filter(|bar| {
            let time = bar.timestamp.with_timezone(&New_York).time();
            time >= start && time < end
        })
        .collect::<Vec<_>>();
    minute_bars.sort_by_key(|bar| bar.timestamp);
    if minute_bars.len() != 90 {
        return Err(ResearchError::IncompleteSession(date));
    }
    let mut buckets = Vec::with_capacity(WINDOW_BUCKETS);
    for chunk in minute_bars.chunks_exact(5) {
        let volume = chunk.iter().map(|bar| bar.volume).sum::<Decimal>();
        if volume <= Decimal::ZERO {
            return Err(ResearchError::IncompleteSession(date));
        }
        let notional = chunk
            .iter()
            .map(|bar| bar.vwap.unwrap_or(bar.close) * bar.volume)
            .sum::<Decimal>();
        buckets.push(Bucket {
            timestamp: chunk[0].timestamp,
            open: chunk[0].open,
            close: chunk.last().expect("five bars").close,
            volume,
            vwap: notional / volume,
        });
    }
    Ok(buckets)
}

fn simulate_session(
    case: &ValidationCase,
    strategies: &[StrategyConfig],
    history: &[(NaiveDate, Vec<MinuteBar>, Vec<Bucket>)],
    current: &(NaiveDate, Vec<MinuteBar>, Vec<Bucket>),
) -> Result<SessionResult, ResearchError> {
    let arrival = current.2[0].open;
    let target = (case.target_notional_usd / arrival).floor();
    let interval_volume = current
        .2
        .iter()
        .map(|bucket| bucket.volume)
        .sum::<Decimal>();
    let interval_vwap = current
        .2
        .iter()
        .map(|bucket| bucket.vwap * bucket.volume)
        .sum::<Decimal>()
        / interval_volume;
    let adv20 = history
        .iter()
        .map(|(_, bars, _)| regular_session_volume(bars))
        .sum::<Decimal>()
        / Decimal::from(history.len());
    let sigma20 = history
        .iter()
        .map(|(_, _, buckets)| realized_volatility_bps(buckets))
        .sum::<Decimal>()
        / Decimal::from(history.len());
    let curve = volume_curve(history);
    let candidates = strategies
        .iter()
        .map(|strategy| {
            simulate_candidate(
                case,
                strategy,
                &current.2,
                &curve,
                target,
                arrival,
                interval_vwap,
                adv20,
                sigma20,
            )
        })
        .collect();
    Ok(SessionResult {
        date: current.0,
        arrival_price: arrival,
        interval_vwap,
        candidates,
    })
}

fn simulate_candidate(
    case: &ValidationCase,
    strategy: &StrategyConfig,
    buckets: &[Bucket],
    curve: &[Decimal],
    target: Decimal,
    arrival: Decimal,
    interval_vwap: Decimal,
    adv20: Decimal,
    sigma20: Decimal,
) -> CandidateSessionResult {
    let mut cumulative = Decimal::ZERO;
    let mut maximum_participation = Decimal::ZERO;
    let mut fills = Vec::with_capacity(WINDOW_BUCKETS);
    let mut replay = Vec::with_capacity(WINDOW_BUCKETS);
    for (index, bucket) in buckets.iter().enumerate() {
        let remaining = (target - cumulative).max(Decimal::ZERO);
        let planned = match strategy {
            StrategyConfig::Immediate => remaining,
            StrategyConfig::Twap { .. } => {
                let scheduled = target * Decimal::from(index + 1) / Decimal::from(WINDOW_BUCKETS);
                (scheduled.floor() - cumulative).max(Decimal::ZERO)
            }
            StrategyConfig::Pov { participation_rate } => bucket.volume * *participation_rate,
            StrategyConfig::Vwap { .. } => {
                let scheduled = target * curve[..=index].iter().copied().sum::<Decimal>();
                (scheduled.floor() - cumulative).max(Decimal::ZERO)
            }
        }
        .min(remaining);
        let cap = (bucket.volume * case.max_participation_rate).floor();
        let filled = planned.min(cap).floor();
        let participation = if bucket.volume > Decimal::ZERO {
            filled / bucket.volume
        } else {
            Decimal::ZERO
        };
        maximum_participation = maximum_participation.max(participation);
        cumulative += filled;
        fills.push((filled, bucket.vwap));
        replay.push(ReplayPoint {
            bucket: index,
            timestamp: bucket.timestamp,
            market_price: bucket.vwap,
            market_volume: bucket.volume,
            planned_quantity: planned,
            filled_quantity: filled,
            cumulative_quantity: cumulative,
        });
    }
    let mut violations = Vec::new();
    if maximum_participation > case.max_participation_rate {
        violations.push("participation_rate_exceeded".to_owned());
    }
    let completed = cumulative >= target;
    if !completed {
        violations.push("incomplete_at_deadline".to_owned());
    }
    let scenarios = [
        ImpactScenario::Low,
        ImpactScenario::Base,
        ImpactScenario::High,
    ]
    .into_iter()
    .map(|scenario| {
        scenario_result(
            scenario,
            &fills,
            cumulative,
            arrival,
            interval_vwap,
            adv20,
            sigma20,
        )
    })
    .collect();
    CandidateSessionResult {
        strategy: strategy.name().to_owned(),
        completed,
        completion_bucket: replay
            .iter()
            .position(|point| point.cumulative_quantity >= target),
        target_quantity: target,
        filled_quantity: cumulative,
        maximum_participation_rate: maximum_participation,
        constraint_violations: violations,
        scenarios,
        replay,
    }
}

fn scenario_result(
    scenario: ImpactScenario,
    fills: &[(Decimal, Decimal)],
    total: Decimal,
    arrival: Decimal,
    interval_vwap: Decimal,
    adv20: Decimal,
    sigma20: Decimal,
) -> ScenarioResult {
    if total <= Decimal::ZERO {
        return ScenarioResult {
            scenario,
            average_price: Decimal::ZERO,
            implementation_shortfall_bps: BPS,
            interval_vwap_slippage_bps: BPS,
        };
    }
    let notional = fills
        .iter()
        .map(|(quantity, market_price)| {
            let ratio = if adv20 > Decimal::ZERO {
                *quantity / adv20
            } else {
                Decimal::ZERO
            };
            let impact_bps = scenario.multiplier() * sigma20 * decimal_sqrt(ratio);
            let price = *market_price * (Decimal::ONE - impact_bps / BPS);
            price * *quantity
        })
        .sum::<Decimal>();
    let average_price = notional / total;
    ScenarioResult {
        scenario,
        average_price,
        implementation_shortfall_bps: (arrival - average_price) / arrival * BPS,
        interval_vwap_slippage_bps: (interval_vwap - average_price) / interval_vwap * BPS,
    }
}

fn regular_session_volume(bars: &[MinuteBar]) -> Decimal {
    let start = NaiveTime::from_hms_opt(9, 30, 0).expect("valid time");
    let end = NaiveTime::from_hms_opt(16, 0, 0).expect("valid time");
    bars.iter()
        .filter(|bar| {
            let time = bar.timestamp.with_timezone(&New_York).time();
            time >= start && time < end
        })
        .map(|bar| bar.volume)
        .sum()
}

fn realized_volatility_bps(buckets: &[Bucket]) -> Decimal {
    let sum_squares = buckets
        .windows(2)
        .map(|window| {
            let value = window[1].close / window[0].close - Decimal::ONE;
            value * value
        })
        .sum::<Decimal>();
    decimal_sqrt(sum_squares) * BPS
}

fn volume_curve(history: &[(NaiveDate, Vec<MinuteBar>, Vec<Bucket>)]) -> Vec<Decimal> {
    let mut totals = vec![Decimal::ZERO; WINDOW_BUCKETS];
    for (_, _, buckets) in history {
        let session_volume = buckets.iter().map(|bucket| bucket.volume).sum::<Decimal>();
        for (index, bucket) in buckets.iter().enumerate() {
            totals[index] += bucket.volume / session_volume;
        }
    }
    let divisor = Decimal::from(history.len());
    let mut curve = totals
        .into_iter()
        .map(|value| value / divisor)
        .collect::<Vec<_>>();
    let total = curve.iter().copied().sum::<Decimal>();
    for value in &mut curve {
        *value /= total;
    }
    curve
}

fn aggregate(sessions: &[SessionResult], strategies: &[StrategyConfig]) -> Vec<AggregateResult> {
    let immediate = collect_shortfalls(sessions, "immediate");
    let immediate_median = percentile(&immediate, 50);
    let immediate_p95 = percentile(&immediate, 95);
    strategies
        .iter()
        .map(|strategy| {
            let name = strategy.name();
            let results = sessions
                .iter()
                .filter_map(|session| {
                    session
                        .candidates
                        .iter()
                        .find(|candidate| candidate.strategy == name)
                })
                .collect::<Vec<_>>();
            let shortfalls = collect_shortfalls(sessions, name);
            let median = percentile(&shortfalls, 50);
            let p95 = percentile(&shortfalls, 95);
            let completed = results
                .iter()
                .filter(|candidate| candidate.completed)
                .count();
            let violations = results
                .iter()
                .map(|candidate| candidate.constraint_violations.len())
                .sum();
            AggregateResult {
                strategy: name.to_owned(),
                evaluated_sessions: results.len(),
                completed_sessions: completed,
                violation_count: violations,
                median_shortfall_bps: median,
                p75_shortfall_bps: percentile(&shortfalls, 75),
                p95_shortfall_bps: p95,
                eligible_for_validation: name != "immediate"
                    && completed == results.len()
                    && violations == 0
                    && median < immediate_median
                    && p95 <= immediate_p95 + Decimal::TEN,
            }
        })
        .collect()
}

fn collect_shortfalls(sessions: &[SessionResult], strategy: &str) -> Vec<Decimal> {
    sessions
        .iter()
        .filter_map(|session| {
            session
                .candidates
                .iter()
                .find(|candidate| candidate.strategy == strategy)
                .and_then(|candidate| {
                    candidate
                        .scenarios
                        .iter()
                        .find(|scenario| scenario.scenario == ImpactScenario::Base)
                })
                .map(|scenario| scenario.implementation_shortfall_bps)
        })
        .collect()
}

fn percentile(values: &[Decimal], percentile: usize) -> Decimal {
    if values.is_empty() {
        return Decimal::ZERO;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let index = ((sorted.len() - 1) * percentile).div_ceil(100);
    sorted[index]
}

fn decimal_sqrt(value: Decimal) -> Decimal {
    if value <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    let mut estimate = if value > Decimal::ONE {
        value / Decimal::TWO
    } else {
        Decimal::ONE
    };
    for _ in 0..32 {
        let next = (estimate + value / estimate) / Decimal::TWO;
        if (next - estimate).abs() < Decimal::from_parts(1, 0, 0, false, 18) {
            return next;
        }
        estimate = next;
    }
    estimate
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike, Duration, TimeZone};

    use super::*;

    fn sample_bars(sessions: usize) -> Vec<MinuteBar> {
        let mut bars = Vec::new();
        let mut date = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
        for session in 0..sessions {
            while date.weekday().number_from_monday() > 5 {
                date += Duration::days(1);
            }
            let local = New_York
                .with_ymd_and_hms(date.year(), date.month(), date.day(), 9, 30, 0)
                .unwrap();
            for minute in 0..390 {
                let price = Decimal::from(300)
                    + Decimal::from(session) / Decimal::TEN
                    + Decimal::from(minute) / Decimal::from(10_000);
                bars.push(MinuteBar {
                    timestamp: (local + Duration::minutes(minute)).with_timezone(&Utc),
                    open: price,
                    high: price + Decimal::ONE,
                    low: price - Decimal::ONE,
                    close: price + Decimal::from_parts(1, 0, 0, false, 2),
                    volume: Decimal::from(50_000),
                    vwap: Some(price),
                    trade_count: Some(100),
                });
            }
            date += Duration::days(1);
        }
        bars
    }

    #[test]
    fn replay_is_deterministic_and_walk_forward() {
        let mut case = ValidationCase::tsla_default();
        case.evaluation_sessions = 2;
        let strategies = vec![
            StrategyConfig::Immediate,
            StrategyConfig::Twap {
                slice_interval_seconds: 300,
            },
            StrategyConfig::Pov {
                participation_rate: Decimal::from_parts(1, 0, 0, false, 1),
            },
            StrategyConfig::Vwap {
                lookback_sessions: 20,
                bucket_interval_seconds: 300,
            },
        ];
        let first = validate(case.clone(), strategies.clone(), sample_bars(22)).unwrap();
        let second = validate(case, strategies, sample_bars(22)).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.sessions.len(), 2);
        assert_eq!(first.data_quality, "iex_proxy");
        assert!(
            first
                .sessions
                .iter()
                .all(|session| session.candidates.len() == 4)
        );
    }

    #[test]
    fn impact_scenarios_are_monotonic() {
        let low = scenario_result(
            ImpactScenario::Low,
            &[(Decimal::from(10_000), Decimal::from(300))],
            Decimal::from(10_000),
            Decimal::from(300),
            Decimal::from(300),
            Decimal::from(1_000_000),
            Decimal::from(200),
        );
        let high = scenario_result(
            ImpactScenario::High,
            &[(Decimal::from(10_000), Decimal::from(300))],
            Decimal::from(10_000),
            Decimal::from(300),
            Decimal::from(300),
            Decimal::from(1_000_000),
            Decimal::from(200),
        );
        assert!(high.average_price < low.average_price);
        assert!(high.implementation_shortfall_bps > low.implementation_shortfall_bps);
    }

    #[test]
    fn incomplete_session_is_rejected_instead_of_filled() {
        let mut case = ValidationCase::tsla_default();
        case.evaluation_sessions = 1;
        let mut bars = sample_bars(21);
        bars.remove(bars.len() - 300);
        let error = validate(case, vec![StrategyConfig::Immediate], bars).unwrap_err();
        assert!(matches!(error, ResearchError::NotEnoughSessions { .. }));
    }
}
