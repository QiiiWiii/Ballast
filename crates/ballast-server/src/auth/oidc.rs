use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{AlgorithmParameters, JwkSet},
};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::RwLock;

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
    /// When true, never hit the network; only static cache entries are used.
    static_only: bool,
}

struct JwksCache {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Option<Instant>,
}

const JWKS_TTL: Duration = Duration::from_secs(300);

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
    pub fn new(config: OidcConfig) -> Result<Self, String> {
        if !config.issuer.starts_with("https://") && !config.issuer.starts_with("http://") {
            return Err("BALLAST_OIDC_ISSUER must be an absolute URL".into());
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| format!("oidc http client: {error}"))?;
        Ok(Self {
            config,
            http,
            cache: RwLock::new(JwksCache {
                keys: HashMap::new(),
                fetched_at: None,
            }),
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
            }),
            static_only: true,
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
        validation.set_issuer(&[self.config.issuer.trim_end_matches('/')]);
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
            if let Some(key) = cache.keys.get(kid) {
                if self.static_only || cache.fetched_at.is_some_and(|at| at.elapsed() < JWKS_TTL) {
                    return Ok(key.clone());
                }
            } else if self.static_only {
                return Err(ApiError::unauthorized("unknown_token_kid"));
            }
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
        let issuer = self.config.issuer.trim_end_matches('/');
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
        let jwks = self
            .http
            .get(&discovery.jwks_uri)
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

#[derive(Debug, Deserialize)]
struct DiscoveryDocument {
    jwks_uri: String,
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
    if roles.is_empty() {
        if let Some(value) = claims.extra.get("roles") {
            push_roles_from_value(value, &mut roles);
        }
    }
    if roles.is_empty() {
        if let Some(realm) = claims.extra.get("realm_access") {
            if let Some(value) = realm.get("roles") {
                push_roles_from_value(value, &mut roles);
            }
        }
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
}
