use std::fs;

use reqwest::Url;

#[derive(Debug, Clone)]
pub(crate) struct ReconciliationAlertConfig {
    pub(crate) webhook_url: Url,
    pub(crate) root_certificate_pem: Option<Vec<u8>>,
}

impl ReconciliationAlertConfig {
    pub(crate) fn from_env() -> Result<Option<Self>, String> {
        let root_certificate_pem = optional_root_certificate_pem()?;
        match std::env::var("BALLAST_RECONCILIATION_ALERT_WEBHOOK_URL") {
            Ok(value) if value.trim().is_empty() => Ok(None),
            Ok(value) => parse_webhook_url(&value).map(|webhook_url| {
                Some(Self {
                    webhook_url,
                    root_certificate_pem,
                })
            }),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err("BALLAST_RECONCILIATION_ALERT_WEBHOOK_URL must be valid UTF-8".to_owned())
            }
        }
    }
}

fn optional_root_certificate_pem() -> Result<Option<Vec<u8>>, String> {
    match std::env::var("BALLAST_RECONCILIATION_ALERT_ROOT_CERT_FILE") {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => {
            let pem = fs::read(value.trim()).map_err(|_| {
                "BALLAST_RECONCILIATION_ALERT_ROOT_CERT_FILE could not be read".to_owned()
            })?;
            reqwest::Certificate::from_pem(&pem).map_err(|_| {
                "BALLAST_RECONCILIATION_ALERT_ROOT_CERT_FILE is not a valid PEM certificate"
                    .to_owned()
            })?;
            Ok(Some(pem))
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err("BALLAST_RECONCILIATION_ALERT_ROOT_CERT_FILE must be valid UTF-8".to_owned())
        }
    }
}

fn parse_webhook_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value.trim())
        .map_err(|_| "BALLAST_RECONCILIATION_ALERT_WEBHOOK_URL must be a valid URL".to_owned())?;
    if url.scheme() != "https" {
        return Err("BALLAST_RECONCILIATION_ALERT_WEBHOOK_URL must use https".to_owned());
    }
    if url.host_str().is_none() || url.username() != "" || url.password().is_some() {
        return Err(
            "BALLAST_RECONCILIATION_ALERT_WEBHOOK_URL must have a host and no credentials"
                .to_owned(),
        );
    }
    if url.fragment().is_some() {
        return Err("BALLAST_RECONCILIATION_ALERT_WEBHOOK_URL must not have a fragment".to_owned());
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::parse_webhook_url;

    #[test]
    fn webhook_url_requires_https_without_credentials_or_fragment() {
        assert!(parse_webhook_url("https://alerts.example.test/hook").is_ok());
        assert!(parse_webhook_url("http://alerts.example.test/hook").is_err());
        assert!(parse_webhook_url("https://user:pass@alerts.example.test/hook").is_err());
        assert!(parse_webhook_url("https://alerts.example.test/hook#secret").is_err());
        assert!(parse_webhook_url("/relative").is_err());
    }
}
