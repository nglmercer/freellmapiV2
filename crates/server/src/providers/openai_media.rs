//! Speech, transcription and translation for OpenAI-compatible endpoints.
//!
//! These impls are only reachable when the adapter declares the matching
//! capability (see `OpenAICompatProvider::with_capabilities`); the accessors
//! on `Provider` gate them.

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::Value;

use super::base::{http_client, send_request, ProviderError};
use super::capabilities::{
    EmbeddingProvider, ImageProvider, MediaResponse, MediaStream, MultipartRequest, SpeechProvider,
    TranscriptionProvider,
};
use super::openai_compat::OpenAICompatProvider;

/// Audio generation and transcription are slower than chat: use a longer
/// timeout than the adapter's chat default.
const MEDIA_TIMEOUT_MS: u64 = 120_000;

impl OpenAICompatProvider {
    fn authorize(&self, req: reqwest::RequestBuilder, api_key: &str) -> reqwest::RequestBuilder {
        let mut req = req.header("Authorization", format!("Bearer {api_key}"));
        for (k, v) in &self.extra_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// Converts the caller's multipart payload into a reqwest form.
    fn form(request: MultipartRequest) -> Result<reqwest::multipart::Form, ProviderError> {
        let mut form = reqwest::multipart::Form::new();
        for (name, value) in request.fields {
            form = form.text(name, value);
        }
        for file in request.files {
            let part = reqwest::multipart::Part::bytes(file.data.to_vec())
                .file_name(file.filename)
                .mime_str(&file.content_type)
                .map_err(|e| ProviderError::new(format!("Invalid file content type: {e}")))?;
            form = form.part(file.field, part);
        }
        Ok(form)
    }

    async fn multipart_call(
        &self,
        api_key: &str,
        path: &str,
        request: MultipartRequest,
    ) -> Result<MediaResponse, ProviderError> {
        let form = Self::form(request)?;
        let req = self
            .authorize(http_client().post(self.url(path)), api_key)
            .multipart(form);
        let res = send_request(req, Some(MEDIA_TIMEOUT_MS)).await?;
        if !res.status().is_success() {
            return Err(Self::error_from_response(res, &self.provider_name).await);
        }
        let content_type = content_type_of(&res, "application/json");
        let body = res
            .bytes()
            .await
            .map_err(|e| ProviderError::new(format!("Failed to read response: {e}")))?;
        Ok(MediaResponse { content_type, body })
    }
}

fn content_type_of(res: &reqwest::Response, fallback: &str) -> String {
    res.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or(fallback)
        .to_string()
}

#[async_trait]
impl SpeechProvider for OpenAICompatProvider {
    async fn speech(&self, api_key: &str, body: &Value) -> Result<MediaStream, ProviderError> {
        let req = self
            .authorize(http_client().post(self.url("/audio/speech")), api_key)
            .json(body);
        let res = send_request(req, Some(MEDIA_TIMEOUT_MS)).await?;
        if !res.status().is_success() {
            return Err(Self::error_from_response(res, &self.provider_name).await);
        }
        let content_type = content_type_of(&res, "application/octet-stream");
        let name = self.provider_name.clone();
        // Forward upstream bytes as they arrive; nothing is buffered here.
        let body = res.bytes_stream().map(move |chunk| {
            chunk.map_err(|e| ProviderError::new(format!("{name} audio stream error: {e}")))
        });
        Ok(MediaStream {
            content_type,
            body: Box::pin(body),
        })
    }
}

#[async_trait]
impl TranscriptionProvider for OpenAICompatProvider {
    async fn transcribe(
        &self,
        api_key: &str,
        request: MultipartRequest,
    ) -> Result<MediaResponse, ProviderError> {
        self.multipart_call(api_key, "/audio/transcriptions", request)
            .await
    }

    async fn translate(
        &self,
        api_key: &str,
        request: MultipartRequest,
    ) -> Result<MediaResponse, ProviderError> {
        self.multipart_call(api_key, "/audio/translations", request)
            .await
    }
}

/// Sends a request expecting a JSON body (image generation and editing).
async fn json_call(
    provider: &OpenAICompatProvider,
    req: reqwest::RequestBuilder,
) -> Result<Value, ProviderError> {
    let res = send_request(req, Some(MEDIA_TIMEOUT_MS)).await?;
    if !res.status().is_success() {
        return Err(OpenAICompatProvider::error_from_response(res, &provider.provider_name).await);
    }
    res.json::<Value>()
        .await
        .map_err(|e| ProviderError::new(format!("Failed to parse response: {e}")))
}

#[async_trait]
impl ImageProvider for OpenAICompatProvider {
    async fn generate(&self, api_key: &str, body: &Value) -> Result<Value, ProviderError> {
        let req = self
            .authorize(http_client().post(self.url("/images/generations")), api_key)
            .json(body);
        json_call(self, req).await
    }

    async fn edit(&self, api_key: &str, request: MultipartRequest) -> Result<Value, ProviderError> {
        let form = Self::form(request)?;
        let req = self
            .authorize(http_client().post(self.url("/images/edits")), api_key)
            .multipart(form);
        json_call(self, req).await
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAICompatProvider {
    async fn embed(&self, api_key: &str, body: &Value) -> Result<Value, ProviderError> {
        let req = self
            .authorize(http_client().post(self.url("/embeddings")), api_key)
            .json(body);
        json_call(self, req).await
    }
}
