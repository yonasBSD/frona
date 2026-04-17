use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use super::super::error::ApiError;
use crate::core::error::{AppError, AuthErrorCode};
use crate::core::principal::{Principal, PrincipalKind};
use crate::core::state::AppState;

pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub token_id: String,
    pub token_type: String,
    pub principal: Principal,
    pub scopes: Option<Vec<String>>,
    pub extensions: Option<serde_json::Value>,
}

impl AuthUser {
    pub fn is_pat(&self) -> bool {
        self.token_type == "pat"
    }

    pub fn is_session(&self) -> bool {
        self.token_type == "access"
    }

    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes
            .as_ref()
            .is_some_and(|s| s.iter().any(|sc| sc == scope))
    }

    pub fn agent_id(&self) -> Option<&str> {
        match self.principal.kind {
            PrincipalKind::Agent => Some(&self.principal.id),
            _ => None,
        }
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_token(parts)?;
        let claims = state
            .token_service
            .validate(&state.keypair_service, token)
            .await?;

        Ok(AuthUser {
            user_id: claims.sub,
            username: claims.username,
            email: claims.email,
            token_id: claims.token_id,
            token_type: claims.token_type,
            principal: claims.principal,
            scopes: claims.scopes,
            extensions: claims.extensions,
        })
    }
}

fn extract_token(parts: &Parts) -> Result<&str, ApiError> {
    if let Some(header) = parts
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
    {
        return header.strip_prefix("Bearer ").ok_or_else(|| {
            ApiError(AppError::Auth {
                message: "Invalid authorization format".into(),
                code: AuthErrorCode::InvalidCredentials,
            })
        });
    }

    Err(ApiError(AppError::Auth {
        message: "Missing authorization".into(),
        code: AuthErrorCode::InvalidCredentials,
    }))
}
