mod models;
mod websocket;

use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use serde::de::DeserializeOwned;

use crate::auth::jwt::JwtService;
use crate::core::error::{AppError, AuthErrorCode};
use crate::core::state::AppState;
use crate::tool::voice::{VoiceCallbackClaims, VoiceSessionClaims};

use models::TokenQuery;

fn build_twiml(
    ws_url: &str,
    welcome_greeting: Option<&str>,
    hints: Option<&str>,
    voice_id: Option<&str>,
    speech_model: Option<&str>,
) -> String {
    use xml::writer::{EmitterConfig, XmlEvent};

    let mut buf = Vec::new();
    let mut w = EmitterConfig::new()
        .perform_indent(false)
        .write_document_declaration(true)
        .create_writer(&mut buf);

    let mut relay = XmlEvent::start_element("ConversationRelay")
        .attr("url", ws_url)
        .attr("language", "en-US")
        .attr("interruptible", "any")
        .attr("interruptSensitivity", "medium")
        .attr("welcomeGreetingInterruptible", "any");

    if let Some(g) = welcome_greeting {
        relay = relay.attr("welcomeGreeting", g);
    }
    if let Some(v) = voice_id {
        relay = relay.attr("voice", v);
    }
    if let Some(m) = speech_model {
        relay = relay.attr("speechModel", m);
    }
    if let Some(h) = hints {
        relay = relay.attr("hints", h);
    }

    w.write(XmlEvent::start_element("Response")).unwrap();
    w.write(XmlEvent::start_element("Connect")).unwrap();
    w.write(relay).unwrap();
    w.write(XmlEvent::end_element()).unwrap(); // ConversationRelay
    w.write(XmlEvent::end_element()).unwrap(); // Connect
    w.write(XmlEvent::end_element()).unwrap(); // Response

    String::from_utf8(buf).expect("xml-rs always emits valid UTF-8")
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/voice/twilio/callback", post(twilio_callback))
        .route("/api/voice/twilio/ws", get(websocket::twilio_ws_handler))
}

pub(super) async fn verify_jwt<T: DeserializeOwned>(state: &AppState, token: &str) -> Result<T, AppError> {
    let jwt_svc = JwtService::new();
    let kid = jwt_svc
        .decode_unverified_header(token)?
        .kid
        .ok_or_else(|| AppError::Auth { message: "Token missing kid".into(), code: AuthErrorCode::TokenInvalid })?;
    let key = state.keypair_service.get_verifying_key(&kid).await?;
    jwt_svc.verify::<T>(token, &key)
}

async fn twilio_callback(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
) -> Response {
    let claims: VoiceCallbackClaims = match verify_jwt(&state, &q.token).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "Voice callback JWT verification failed");
            return (StatusCode::FORBIDDEN, "Invalid token").into_response();
        }
    };

    let user_id = claims.sub.clone();
    let chat_id = claims.chat_id.clone();

    let call_id = match state.call_service.find_by_chat_id(&chat_id).await {
        Ok(Some(call)) => {
            match state.call_service.mark_active(&call.id).await {
                Ok(updated) => {
                    tracing::info!(call_id = %updated.id, chat_id = %chat_id, "Call marked Active");
                    Some(updated.id)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to mark call active");
                    Some(call.id)
                }
            }
        }
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to look up call by chat_id");
            None
        }
    };

    let owner = format!("user:{user_id}");
    let expiry_secs = state.config.auth.presign_expiry_secs as i64;
    let (enc_key, kid) = match state.keypair_service.get_signing_key(&owner).await {
        Ok(k) => k,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get signing key for voice session");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
        }
    };

    let exp = (Utc::now().timestamp() + expiry_secs) as usize;
    let ws_claims = VoiceSessionClaims {
        sub: user_id.clone(),
        chat_id: chat_id.clone(),
        exp,
        contact_id: claims.contact_id.clone(),
        call_id: call_id.clone(),
    };
    let ws_token = match JwtService::new().sign(&ws_claims, &enc_key, &kid) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "Failed to sign voice session JWT");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
        }
    };

    let base_url = state.config.voice.callback_base_url.clone()
        .or_else(|| state.config.server.base_url.clone())
        .unwrap_or_else(|| format!("http://localhost:{}", state.config.server.port));
    let ws_base = base_url
        .replace("https://", "wss://")
        .replace("http://", "ws://");
    let ws_url = format!("{ws_base}/api/voice/twilio/ws?token={ws_token}");

    let twiml = build_twiml(
        &ws_url,
        claims.welcome_greeting.as_deref(),
        claims.hints.as_deref(),
        state.config.voice.twilio_voice_id.as_deref(),
        state.config.voice.twilio_speech_model.as_deref(),
    );

    tracing::info!(chat_id = %chat_id, user_id = %user_id, ws_url = %ws_url, "Voice callback: issuing TwiML with ConversationRelay");

    let mut response = twiml.into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml"),
    );
    response
}
