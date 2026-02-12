use std::future::Future;

use backon::Retryable;
use rig::completion::Message as RigMessage;
use tokio::sync::mpsc;

use super::config::{ModelGroup, RetryConfig};
use super::context::truncate_history;
use super::error::InferenceError;
use super::provider::ModelRef;
use super::registry::ModelProviderRegistry;

pub async fn inference_with_fallback(
    registry: &ModelProviderRegistry,
    model_group: &ModelGroup,
    system_prompt: &str,
    history: Vec<RigMessage>,
    user_message: RigMessage,
) -> Result<String, InferenceError> {
    let mut errors = Vec::new();
    let max_tokens = model_group.max_tokens;
    let temperature = model_group.temperature;
    let max_output = max_tokens.unwrap_or(8192) as usize;

    let truncated = truncate_history(
        history,
        system_prompt,
        &model_group.main.model_id,
        model_group.context_window,
        max_output,
    );

    let ref_str = model_group.main.as_str();
    match retry_with_backoff(&model_group.retry, &model_group.main, || async {
        let provider = registry.get_provider(&model_group.main.provider)?;
        provider
            .inference(
                &model_group.main.model_id,
                system_prompt,
                truncated.clone(),
                user_message.clone(),
                max_tokens,
                temperature,
            )
            .await
    })
    .await
    {
        Ok(response) => {
            tracing::info!(model = %ref_str, "Completion succeeded");
            return Ok(response);
        }
        Err(e) => {
            tracing::warn!(model = %ref_str, error = %e, "Main model failed, trying fallbacks");
            errors.push((ref_str, e.to_string()));
        }
    }

    for fallback in &model_group.fallbacks {
        let ref_str = fallback.as_str();
        let truncated_fb = truncate_history(
            truncated.clone(),
            system_prompt,
            &fallback.model_id,
            model_group.context_window,
            max_output,
        );
        match retry_with_backoff(&model_group.retry, fallback, || async {
            let provider = registry.get_provider(&fallback.provider)?;
            provider
                .inference(
                    &fallback.model_id,
                    system_prompt,
                    truncated_fb.clone(),
                    user_message.clone(),
                    max_tokens,
                    temperature,
                )
                .await
        })
        .await
        {
            Ok(response) => {
                tracing::info!(model = %ref_str, "Fallback succeeded");
                return Ok(response);
            }
            Err(e) => {
                tracing::warn!(model = %ref_str, error = %e, "Fallback failed");
                errors.push((ref_str, e.to_string()));
            }
        }
    }

    Err(InferenceError::AllFallbacksFailed(errors))
}

pub async fn stream_inference_with_fallback(
    registry: &ModelProviderRegistry,
    model_group: &ModelGroup,
    system_prompt: &str,
    history: Vec<RigMessage>,
    user_message: RigMessage,
    token_tx: mpsc::Sender<Result<String, InferenceError>>,
) -> Result<(), InferenceError> {
    let mut errors = Vec::new();
    let max_tokens = model_group.max_tokens;
    let temperature = model_group.temperature;
    let max_output = max_tokens.unwrap_or(8192) as usize;

    let truncated = truncate_history(
        history,
        system_prompt,
        &model_group.main.model_id,
        model_group.context_window,
        max_output,
    );

    let ref_str = model_group.main.as_str();
    match retry_with_backoff(&model_group.retry, &model_group.main, || async {
        let provider = registry.get_provider(&model_group.main.provider)?;
        provider
            .stream_inference(
                &model_group.main.model_id,
                system_prompt,
                truncated.clone(),
                user_message.clone(),
                token_tx.clone(),
                max_tokens,
                temperature,
            )
            .await
    })
    .await
    {
        Ok(()) => {
            tracing::info!(model = %ref_str, "Stream succeeded");
            return Ok(());
        }
        Err(e) => {
            tracing::warn!(model = %ref_str, error = %e, "Main model stream failed, trying fallbacks");
            errors.push((ref_str, e.to_string()));
        }
    }

    for fallback in &model_group.fallbacks {
        let ref_str = fallback.as_str();
        let truncated_fb = truncate_history(
            truncated.clone(),
            system_prompt,
            &fallback.model_id,
            model_group.context_window,
            max_output,
        );
        match retry_with_backoff(&model_group.retry, fallback, || async {
            let provider = registry.get_provider(&fallback.provider)?;
            provider
                .stream_inference(
                    &fallback.model_id,
                    system_prompt,
                    truncated_fb.clone(),
                    user_message.clone(),
                    token_tx.clone(),
                    max_tokens,
                    temperature,
                )
                .await
        })
        .await
        {
            Ok(()) => {
                tracing::info!(model = %ref_str, "Fallback stream succeeded");
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(model = %ref_str, error = %e, "Fallback stream failed");
                errors.push((ref_str, e.to_string()));
            }
        }
    }

    Err(InferenceError::AllFallbacksFailed(errors))
}

async fn retry_with_backoff<T, F, Fut>(
    retry_config: &RetryConfig,
    model_ref: &ModelRef,
    op: F,
) -> Result<T, InferenceError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, InferenceError>>,
{
    let model_str = model_ref.as_str();
    (|| async { op().await })
        .retry(retry_config.to_backoff())
        .sleep(tokio::time::sleep)
        .when(|e| e.is_retryable())
        .notify(|e, dur| {
            tracing::warn!(model = %model_str, error = %e, delay = ?dur, "Retryable error, backing off");
        })
        .await
}
