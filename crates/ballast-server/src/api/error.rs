use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

pub(crate) type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug)]
pub(crate) struct ApiError {
    status: StatusCode,
    code: &'static str,
    params: Value,
}

impl ApiError {
    pub(crate) fn validation(code: &'static str, params: Value) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code,
            params,
        }
    }

    pub(crate) fn not_found(code: &'static str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code,
            params: json!({}),
        }
    }

    pub(crate) fn conflict(code: &'static str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code,
            params: json!({}),
        }
    }

    pub(crate) fn locked(code: &'static str) -> Self {
        Self {
            status: StatusCode::LOCKED,
            code,
            params: json!({}),
        }
    }

    pub(crate) fn unauthorized(code: &'static str) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code,
            params: json!({}),
        }
    }

    pub(crate) fn forbidden(code: &'static str) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code,
            params: json!({}),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn code(&self) -> &'static str {
        self.code
    }

    pub(crate) fn database(error: sqlx::Error) -> Self {
        tracing::error!(%error, "database operation failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "database_error",
            params: json!({}),
        }
    }

    pub(crate) fn internal(code: &'static str) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code,
            params: json!({}),
        }
    }

    pub(crate) fn template_database(error: sqlx::Error) -> Self {
        if error
            .as_database_error()
            .and_then(|database| database.code())
            .is_some_and(|code| code == "23505")
        {
            return Self::conflict("strategy_template_name_conflict");
        }
        if matches!(&error, sqlx::Error::Protocol(message) if message.contains("archived")) {
            return Self::conflict("strategy_template_archived");
        }
        Self::database(error)
    }

    pub(crate) fn gateway(error: ballast_gateway_client::GatewayClientError) -> Self {
        tracing::warn!(%error, "gateway operation failed");
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "gateway_unavailable",
            params: json!({}),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "error": { "code": self.code, "params": self.params } })),
        )
            .into_response()
    }
}
