use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{collections::HashMap, fs};

use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{AlgorithmParameters, JwkSet},
};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};

use super::{Principal, Role, role_rank};
use crate::api::ApiError;

#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub audience: String,
    pub role_claim: String,
    pub clock_skew_secs: u64,
}

pub struct OidcValidatorInner {
    config: OidcConfig,
    http: reqwest::Client,
    cache: RwLock<JwksCache>,
    refresh_lock: Mutex<()>,
    /// When true, never hit the network; only static cache entries are used.
    static_only: bool,
}

struct JwksCache {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Option<Instant>,
    refresh_attempted_at: Option<Instant>,
}

const JWKS_TTL: Duration = Duration::from_secs(300);
const JWKS_REFRESH_COOLDOWN: Duration = Duration::from_secs(1);

pub struct OidcValidator {
    inner: Arc<OidcValidatorInner>,
}

impl OidcValidator {
    pub fn from_inner(inner: OidcValidatorInner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub async fn authenticate(&self, token: &str) -> Result<Principal, ApiError> {
        self.inner.authenticate(token).await
    }

    #[allow(dead_code)]
    pub fn config(&self) -> &OidcConfig {
        &self.inner.config
    }
}

impl OidcValidatorInner {
    pub fn new(mut config: OidcConfig) -> Result<Self, String> {
        let issuer = config.issuer.trim_end_matches('/');
        let parsed_issuer = reqwest::Url::parse(issuer)
            .map_err(|_| "BALLAST_OIDC_ISSUER must be an absolute HTTPS URL".to_owned())?;
        if parsed_issuer.scheme() != "https" || parsed_issuer.host_str().is_none() {
            return Err("BALLAST_OIDC_ISSUER must be an absolute HTTPS URL".into());
        }
        config.issuer = issuer.to_owned();
        let mut http_builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none());
        if let Some(path) = optional_certificate_path("BALLAST_OIDC_ROOT_CERT_FILE")? {
            let pem = fs::read(path)
                .map_err(|_| "BALLAST_OIDC_ROOT_CERT_FILE could not be read".to_owned())?;
            let certificate = reqwest::Certificate::from_pem(&pem).map_err(|_| {
                "BALLAST_OIDC_ROOT_CERT_FILE is not a valid PEM certificate".to_owned()
            })?;
            http_builder = http_builder.add_root_certificate(certificate);
        }
        let http = http_builder
            .build()
            .map_err(|error| format!("oidc http client: {error}"))?;
        Ok(Self {
            config,
            http,
            cache: RwLock::new(JwksCache {
                keys: HashMap::new(),
                fetched_at: None,
                refresh_attempted_at: None,
            }),
            refresh_lock: Mutex::new(()),
            static_only: false,
        })
    }

    #[cfg(test)]
    pub fn with_static_keys(config: OidcConfig, keys: HashMap<String, DecodingKey>) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
            cache: RwLock::new(JwksCache {
                keys,
                fetched_at: Some(Instant::now()),
                refresh_attempted_at: None,
            }),
            refresh_lock: Mutex::new(()),
            static_only: true,
        }
    }

    #[cfg(test)]
    fn with_cached_keys(config: OidcConfig, keys: HashMap<String, DecodingKey>) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
            cache: RwLock::new(JwksCache {
                keys,
                fetched_at: Some(Instant::now()),
                refresh_attempted_at: None,
            }),
            refresh_lock: Mutex::new(()),
            static_only: false,
        }
    }

    pub async fn authenticate(&self, token: &str) -> Result<Principal, ApiError> {
        let header = decode_header(token).map_err(|_| ApiError::unauthorized("invalid_token"))?;
        let kid = header
            .kid
            .ok_or_else(|| ApiError::unauthorized("token_missing_kid"))?;
        let key = self.decoding_key(&kid).await?;
        let algorithm = match header.alg {
            Algorithm::RS256
            | Algorithm::RS384
            | Algorithm::RS512
            | Algorithm::ES256
            | Algorithm::ES384 => header.alg,
            _ => return Err(ApiError::unauthorized("invalid_token")),
        };
        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[self.config.issuer.as_str()]);
        validation.set_audience(&[self.config.audience.as_str()]);
        validation.leeway = self.config.clock_skew_secs;

        let data = decode::<Claims>(token, &key, &validation).map_err(|error| {
            tracing::error!(%error, "jwt decode failed");
            ApiError::unauthorized("invalid_token")
        })?;
        let roles = extract_roles(&data.claims, &self.config.role_claim);
        if roles.is_empty() {
            return Err(ApiError::forbidden("role_claim_missing"));
        }
        Ok(Principal {
            subject: data.claims.sub,
            roles,
        })
    }

    async fn decoding_key(&self, kid: &str) -> Result<DecodingKey, ApiError> {
        {
            let cache = self.cache.read().await;
            let fresh = cache.fetched_at.is_some_and(|at| at.elapsed() < JWKS_TTL);
            if let Some(key) = cache.keys.get(kid) {
                if self.static_only || fresh {
                    return Ok(key.clone());
                }
            } else if self.static_only {
                return Err(ApiError::unauthorized("unknown_token_kid"));
            }
        }

        // Re-check after acquiring the lock so concurrent requests share one refresh.
        let _refresh_guard = self.refresh_lock.lock().await;
        {
            let cache = self.cache.read().await;
            let fresh = cache.fetched_at.is_some_and(|at| at.elapsed() < JWKS_TTL);
            if let Some(key) = cache.keys.get(kid) {
                if self.static_only || fresh {
                    return Ok(key.clone());
                }
            } else if self.static_only {
                return Err(ApiError::unauthorized("unknown_token_kid"));
            }
            if cache
                .refresh_attempted_at
                .is_some_and(|at| at.elapsed() < JWKS_REFRESH_COOLDOWN)
            {
                return Err(ApiError::unauthorized("jwks_refresh_throttled"));
            }
        }
        {
            let mut cache = self.cache.write().await;
            cache.refresh_attempted_at = Some(Instant::now());
        }
        self.refresh_jwks().await?;
        let cache = self.cache.read().await;
        cache
            .keys
            .get(kid)
            .cloned()
            .ok_or_else(|| ApiError::unauthorized("unknown_token_kid"))
    }

    async fn refresh_jwks(&self) -> Result<(), ApiError> {
        if self.static_only {
            return Err(ApiError::unauthorized("unknown_token_kid"));
        }
        let issuer = self.config.issuer.as_str();
        let discovery_url = format!("{issuer}/.well-known/openid-configuration");
        let discovery = self
            .http
            .get(&discovery_url)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(%error, "oidc discovery failed");
                ApiError::unauthorized("oidc_discovery_failed")
            })?
            .error_for_status()
            .map_err(|error| {
                tracing::error!(%error, "oidc discovery status");
                ApiError::unauthorized("oidc_discovery_failed")
            })?
            .json::<DiscoveryDocument>()
            .await
            .map_err(|error| {
                tracing::error!(%error, "oidc discovery decode failed");
                ApiError::unauthorized("oidc_discovery_failed")
            })?;
        validate_discovery_issuer(issuer, &discovery.issuer)?;
        let jwks_uri = reqwest::Url::parse(&discovery.jwks_uri)
            .ok()
            .filter(|url| url.scheme() == "https" && url.host_str().is_some())
            .ok_or_else(|| ApiError::unauthorized("jwks_invalid"))?;
        let jwks = self
            .http
            .get(jwks_uri)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(%error, "jwks fetch failed");
                ApiError::unauthorized("jwks_fetch_failed")
            })?
            .error_for_status()
            .map_err(|error| {
                tracing::error!(%error, "jwks status");
                ApiError::unauthorized("jwks_fetch_failed")
            })?
            .json::<JwkSet>()
            .await
            .map_err(|error| {
                tracing::error!(%error, "jwks decode failed");
                ApiError::unauthorized("jwks_fetch_failed")
            })?;

        let mut keys = HashMap::new();
        for jwk in jwks.keys {
            let Some(kid) = jwk.common.key_id.clone() else {
                continue;
            };
            let decoding = match &jwk.algorithm {
                AlgorithmParameters::RSA(params) => {
                    DecodingKey::from_rsa_components(&params.n, &params.e).map_err(|error| {
                        tracing::error!(%error, %kid, "invalid rsa jwk");
                        ApiError::unauthorized("jwks_invalid")
                    })?
                }
                AlgorithmParameters::EllipticCurve(params) => {
                    DecodingKey::from_ec_components(&params.x, &params.y).map_err(|error| {
                        tracing::error!(%error, %kid, "invalid ec jwk");
                        ApiError::unauthorized("jwks_invalid")
                    })?
                }
                _ => continue,
            };
            keys.insert(kid, decoding);
        }
        let mut cache = self.cache.write().await;
        cache.keys = keys;
        cache.fetched_at = Some(Instant::now());
        Ok(())
    }
}

fn optional_certificate_path(name: &str) -> Result<Option<String>, String> {
    match std::env::var(name) {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value.trim().to_owned())),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid UTF-8")),
    }
}

#[derive(Debug, Deserialize)]
struct DiscoveryDocument {
    issuer: String,
    jwks_uri: String,
}

fn validate_discovery_issuer(configured: &str, discovered: &str) -> Result<(), ApiError> {
    if discovered != configured {
        return Err(ApiError::unauthorized("oidc_issuer_mismatch"));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

fn extract_roles(claims: &Claims, role_claim: &str) -> Vec<Role> {
    let mut roles = Vec::new();
    if let Some(value) = claims.extra.get(role_claim) {
        push_roles_from_value(value, &mut roles);
    }
    roles.sort_by_key(|role| std::cmp::Reverse(role_rank(*role)));
    roles.dedup();
    roles
}

fn push_roles_from_value(value: &Value, roles: &mut Vec<Role>) {
    match value {
        Value::String(text) => {
            for part in text.split([',', ' ']) {
                if let Some(role) = Role::parse(part) {
                    roles.push(role);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                if let Some(text) = item.as_str() {
                    if let Some(role) = Role::parse(text) {
                        roles.push(role);
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::test_keys::{TEST_RSA_PRIVATE_PEM, TEST_RSA_PUBLIC_PEM};
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde::Serialize;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Serialize)]
    struct TestClaims {
        sub: String,
        iss: String,
        aud: String,
        exp: usize,
        iat: usize,
        ballast_roles: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        roles: Option<Vec<String>>,
    }

    fn now() -> usize {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize
    }

    fn validator_with_test_key() -> OidcValidatorInner {
        let decoding = DecodingKey::from_rsa_pem(TEST_RSA_PUBLIC_PEM.as_bytes()).unwrap();
        let mut keys = HashMap::new();
        keys.insert("test-key".into(), decoding);
        OidcValidatorInner::with_static_keys(
            OidcConfig {
                issuer: "http://issuer.test".into(),
                audience: "ballast".into(),
                role_claim: "ballast_roles".into(),
                clock_skew_secs: 60,
            },
            keys,
        )
    }

    fn mint(roles: &[&str], aud: &str) -> String {
        let encoding = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_PEM.as_bytes()).unwrap();
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".into());
        let claims = TestClaims {
            sub: "user-1".into(),
            iss: "http://issuer.test".into(),
            aud: aud.into(),
            exp: now() + 300,
            iat: now(),
            ballast_roles: roles.iter().map(|role| (*role).to_owned()).collect(),
            roles: None,
        };
        encode(&header, &claims, &encoding).unwrap()
    }

    #[tokio::test]
    async fn accepts_valid_rsa_token() {
        let token = mint(&["operator"], "ballast");
        let principal = validator_with_test_key()
            .authenticate(&token)
            .await
            .unwrap();
        assert_eq!(principal.subject, "user-1");
        assert!(principal.has_at_least(Role::Operator));
        assert!(!principal.has_at_least(Role::Admin));
    }

    #[tokio::test]
    async fn rejects_wrong_audience() {
        let token = mint(&["admin"], "other");
        let err = validator_with_test_key()
            .authenticate(&token)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid_token");
    }

    #[tokio::test]
    async fn rejects_missing_roles() {
        let token = mint(&[], "ballast");
        let err = validator_with_test_key()
            .authenticate(&token)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "role_claim_missing");
    }

    #[tokio::test]
    async fn rejects_roles_from_unconfigured_claim() {
        let encoding = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_PEM.as_bytes()).unwrap();
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".into());
        let claims = TestClaims {
            sub: "user-1".into(),
            iss: "http://issuer.test".into(),
            aud: "ballast".into(),
            exp: now() + 300,
            iat: now(),
            ballast_roles: Vec::new(),
            roles: Some(vec!["admin".into()]),
        };
        let token = encode(&header, &claims, &encoding).unwrap();
        let err = validator_with_test_key()
            .authenticate(&token)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "role_claim_missing");
    }

    #[test]
    fn rejects_non_https_issuer() {
        let result = OidcValidatorInner::new(OidcConfig {
            issuer: "http://issuer.test".into(),
            audience: "ballast".into(),
            role_claim: "ballast_roles".into(),
            clock_skew_secs: 60,
        });
        let error = result
            .err()
            .expect("OIDC issuer must use HTTPS in network mode");
        assert_eq!(error, "BALLAST_OIDC_ISSUER must be an absolute HTTPS URL");
    }

    #[test]
    fn discovery_issuer_must_match_configured_issuer() {
        let configured = "https://issuer.test";
        assert!(validate_discovery_issuer(configured, configured).is_ok());
        let slash_error = validate_discovery_issuer(configured, "https://issuer.test/")
            .expect_err("discovery issuer comparison must be exact");
        assert_eq!(slash_error.code(), "oidc_issuer_mismatch");
        let host_error = validate_discovery_issuer(configured, "https://other.test")
            .expect_err("a different discovery issuer must be rejected");
        assert_eq!(host_error.code(), "oidc_issuer_mismatch");
    }

    #[tokio::test]
    async fn unknown_kid_refresh_is_throttled_after_the_first_attempt() {
        let decoding = DecodingKey::from_rsa_pem(TEST_RSA_PUBLIC_PEM.as_bytes()).unwrap();
        let mut keys = HashMap::new();
        keys.insert("test-key".into(), decoding);
        let validator = OidcValidatorInner::with_cached_keys(
            OidcConfig {
                issuer: "https://127.0.0.1:1".into(),
                audience: "ballast".into(),
                role_claim: "ballast_roles".into(),
                clock_skew_secs: 60,
            },
            keys,
        );

        let first = validator
            .decoding_key("rotated-key")
            .await
            .err()
            .expect("the first unknown kid must attempt a refresh");
        assert_eq!(first.code(), "oidc_discovery_failed");

        let second = validator
            .decoding_key("another-key")
            .await
            .err()
            .expect("subsequent refreshes inside the cooldown must be rejected");
        assert_eq!(second.code(), "jwks_refresh_throttled");
    }
}
