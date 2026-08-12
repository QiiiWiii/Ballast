#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use ballast_research::MinuteBar;
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::{Client, StatusCode};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

const DEFAULT_BASE_URL: &str = "https://data.alpaca.markets";

#[derive(Debug, Clone)]
pub struct AlpacaClient {
    http: Client,
    base_url: String,
    key_id: String,
    secret_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub provider: String,
    pub feed: String,
    pub symbol: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub available: bool,
    pub first_bar_at: Option<DateTime<Utc>>,
    pub limitation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyManifest {
    pub provider: String,
    pub feed: String,
    pub symbol: String,
    pub session_date: NaiveDate,
    pub schema: String,
    pub record_count: usize,
    pub sha256: String,
    pub path: PathBuf,
    pub first_bar_at: DateTime<Utc>,
    pub last_bar_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum AlpacaError {
    #[error("ALPACA_API_KEY_ID is not configured")]
    MissingKeyId,
    #[error("ALPACA_API_SECRET_KEY is not configured")]
    MissingSecretKey,
    #[error("alpaca request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("alpaca returned {status}: {message}")]
    Api { status: StatusCode, message: String },
    #[error("alpaca payload is invalid: {0}")]
    InvalidPayload(String),
    #[error("research data file failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("research data serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("download returned no bars")]
    EmptyDownload,
}

impl AlpacaClient {
    pub fn from_env() -> Result<Self, AlpacaError> {
        let key_id = std::env::var("ALPACA_API_KEY_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or(AlpacaError::MissingKeyId)?;
        let secret_key = std::env::var("ALPACA_API_SECRET_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or(AlpacaError::MissingSecretKey)?;
        Ok(Self::new(DEFAULT_BASE_URL, key_id, secret_key))
    }

    #[must_use]
    pub fn new(
        base_url: impl Into<String>,
        key_id: impl Into<String>,
        secret_key: impl Into<String>,
    ) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into(),
            key_id: key_id.into(),
            secret_key: secret_key.into(),
        }
    }

    pub async fn coverage(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Coverage, AlpacaError> {
        let bars = self.fetch_page(symbol, start, end, 1, None).await?;
        Ok(Coverage {
            provider: "alpaca".to_owned(),
            feed: "iex".to_owned(),
            symbol: symbol.to_owned(),
            start,
            end,
            available: !bars.bars.is_empty(),
            first_bar_at: bars.bars.first().map(|bar| bar.timestamp),
            limitation: "IEX-only proxy; not consolidated US market volume".to_owned(),
        })
    }

    pub async fn download_bars(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MinuteBar>, AlpacaError> {
        let mut result = Vec::new();
        let mut page_token = None;
        loop {
            let page = self
                .fetch_page(symbol, start, end, 10_000, page_token.as_deref())
                .await?;
            result.extend(page.bars);
            match page.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }
        if result.is_empty() {
            return Err(AlpacaError::EmptyDownload);
        }
        result.sort_by_key(|bar| bar.timestamp);
        result.dedup_by_key(|bar| bar.timestamp);
        Ok(result)
    }

    async fn fetch_page(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: usize,
        page_token: Option<&str>,
    ) -> Result<BarsPage, AlpacaError> {
        let url = format!(
            "{}/v2/stocks/{}/bars",
            self.base_url.trim_end_matches('/'),
            symbol
        );
        let mut query = vec![
            ("timeframe", "1Min".to_owned()),
            ("start", start.to_rfc3339()),
            ("end", end.to_rfc3339()),
            ("adjustment", "raw".to_owned()),
            ("feed", "iex".to_owned()),
            ("sort", "asc".to_owned()),
            ("limit", limit.to_string()),
        ];
        if let Some(token) = page_token {
            query.push(("page_token", token.to_owned()));
        }
        let response = self
            .http
            .get(url)
            .headers(self.headers()?)
            .query(&query)
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            let message = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or(body);
            return Err(AlpacaError::Api { status, message });
        }
        parse_bars_page(&body)
    }

    fn headers(&self) -> Result<reqwest::header::HeaderMap, AlpacaError> {
        use reqwest::header::{HeaderMap, HeaderValue};
        let mut headers = HeaderMap::new();
        headers.insert(
            "APCA-API-KEY-ID",
            HeaderValue::from_str(&self.key_id)
                .map_err(|error| AlpacaError::InvalidPayload(error.to_string()))?,
        );
        headers.insert(
            "APCA-API-SECRET-KEY",
            HeaderValue::from_str(&self.secret_key)
                .map_err(|error| AlpacaError::InvalidPayload(error.to_string()))?,
        );
        Ok(headers)
    }
}

#[derive(Debug)]
struct BarsPage {
    bars: Vec<MinuteBar>,
    next_page_token: Option<String>,
}

fn parse_bars_page(body: &str) -> Result<BarsPage, AlpacaError> {
    let value: Value = serde_json::from_str(body)?;
    let raw_bars = value
        .get("bars")
        .and_then(Value::as_array)
        .ok_or_else(|| AlpacaError::InvalidPayload("bars array is missing".to_owned()))?;
    let bars = raw_bars
        .iter()
        .map(parse_bar)
        .collect::<Result<Vec<_>, _>>()?;
    let next_page_token = value
        .get("next_page_token")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(BarsPage {
        bars,
        next_page_token,
    })
}

fn parse_bar(value: &Value) -> Result<MinuteBar, AlpacaError> {
    let field = |name: &str| {
        value
            .get(name)
            .ok_or_else(|| AlpacaError::InvalidPayload(format!("bar field {name} is missing")))
    };
    let decimal = |name: &str| -> Result<Decimal, AlpacaError> {
        Decimal::from_str(&field(name)?.to_string())
            .map_err(|error| AlpacaError::InvalidPayload(format!("bar field {name}: {error}")))
    };
    let timestamp =
        DateTime::parse_from_rfc3339(field("t")?.as_str().ok_or_else(|| {
            AlpacaError::InvalidPayload("bar timestamp is not a string".to_owned())
        })?)
        .map_err(|error| AlpacaError::InvalidPayload(error.to_string()))?
        .with_timezone(&Utc);
    Ok(MinuteBar {
        timestamp,
        open: decimal("o")?,
        high: decimal("h")?,
        low: decimal("l")?,
        close: decimal("c")?,
        volume: decimal("v")?,
        vwap: value
            .get("vw")
            .filter(|item| !item.is_null())
            .map(|_| decimal("vw"))
            .transpose()?,
        trade_count: value.get("n").and_then(Value::as_u64),
    })
}

pub async fn store_daily_files(
    root: &Path,
    symbol: &str,
    bars: &[MinuteBar],
) -> Result<Vec<DailyManifest>, AlpacaError> {
    let mut sessions: BTreeMap<NaiveDate, Vec<MinuteBar>> = BTreeMap::new();
    for bar in bars {
        let date = bar
            .timestamp
            .with_timezone(&chrono_tz::America::New_York)
            .date_naive();
        sessions.entry(date).or_default().push(bar.clone());
    }
    let directory = root.join("alpaca").join("iex").join(symbol);
    tokio::fs::create_dir_all(&directory).await?;
    let mut manifests = Vec::with_capacity(sessions.len());
    for (date, mut session) in sessions {
        session.sort_by_key(|bar| bar.timestamp);
        let encoded = serde_json::to_vec(&session)?;
        let compressed = zstd::stream::encode_all(encoded.as_slice(), 9)?;
        let sha256 = hex::encode(Sha256::digest(&compressed));
        let file_name = format!("{date}.json.zst");
        let final_path = directory.join(file_name);
        let partial_path = final_path.with_extension("json.zst.partial");
        tokio::fs::write(&partial_path, &compressed).await?;
        verify_file(&partial_path, symbol, date, session.len()).await?;
        tokio::fs::rename(&partial_path, &final_path).await?;
        manifests.push(DailyManifest {
            provider: "alpaca".to_owned(),
            feed: "iex".to_owned(),
            symbol: symbol.to_owned(),
            session_date: date,
            schema: "minute_bar_v1".to_owned(),
            record_count: session.len(),
            sha256,
            path: final_path,
            first_bar_at: session.first().expect("non-empty session").timestamp,
            last_bar_at: session.last().expect("non-empty session").timestamp,
        });
    }
    Ok(manifests)
}

pub async fn read_manifest_file(manifest: &DailyManifest) -> Result<Vec<MinuteBar>, AlpacaError> {
    let compressed = tokio::fs::read(&manifest.path).await?;
    let actual_hash = hex::encode(Sha256::digest(&compressed));
    if actual_hash != manifest.sha256 {
        return Err(AlpacaError::InvalidPayload("sha256 mismatch".to_owned()));
    }
    decode_and_verify(
        &compressed,
        &manifest.symbol,
        manifest.session_date,
        manifest.record_count,
    )
}

async fn verify_file(
    path: &Path,
    symbol: &str,
    expected_date: NaiveDate,
    expected_records: usize,
) -> Result<Vec<MinuteBar>, AlpacaError> {
    let compressed = tokio::fs::read(path).await?;
    decode_and_verify(&compressed, symbol, expected_date, expected_records)
}

fn decode_and_verify(
    compressed: &[u8],
    symbol: &str,
    expected_date: NaiveDate,
    expected_records: usize,
) -> Result<Vec<MinuteBar>, AlpacaError> {
    if symbol.trim().is_empty() {
        return Err(AlpacaError::InvalidPayload("symbol is empty".to_owned()));
    }
    let decoded = zstd::stream::decode_all(compressed)?;
    let bars: Vec<MinuteBar> = serde_json::from_slice(&decoded)?;
    if bars.len() != expected_records {
        return Err(AlpacaError::InvalidPayload(
            "record count mismatch".to_owned(),
        ));
    }
    if bars.iter().any(|bar| {
        bar.timestamp
            .with_timezone(&chrono_tz::America::New_York)
            .date_naive()
            != expected_date
    }) {
        return Err(AlpacaError::InvalidPayload(
            "session date mismatch".to_owned(),
        ));
    }
    Ok(bars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_decimal_bar_without_binary_float_conversion() {
        let page = parse_bars_page(
            r#"{"bars":[{"t":"2025-01-02T15:00:00Z","o":300.01,"h":301.02,"l":299.99,"c":300.50,"v":12345,"vw":300.25,"n":99}],"next_page_token":null}"#,
        )
        .unwrap();
        assert_eq!(page.bars[0].open, Decimal::from_str("300.01").unwrap());
        assert_eq!(page.bars[0].volume, Decimal::from(12_345));
    }

    #[test]
    fn missing_bars_is_an_explicit_error() {
        assert!(matches!(
            parse_bars_page(r#"{"message":"not found"}"#),
            Err(AlpacaError::InvalidPayload(_))
        ));
    }
}
