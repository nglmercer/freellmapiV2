//! Optional modality interfaces for providers.
//!
//! Chat is part of the base [`Provider`](super::base::Provider) trait. Every
//! other modality is an optional interface: a provider implements only the
//! ones it really supports and advertises them through
//! [`Provider::capabilities`](super::base::Provider::capabilities). The
//! accessor methods on `Provider` return `None` by default, so providers never
//! have to stub out modalities they do not offer.

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::Serialize;
use serde_json::Value;

use super::base::ProviderError;

/// Operations a provider or model can support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// `/v1/chat/completions` and `/v1/completions`.
    Chat,
    /// Image input inside chat messages.
    Vision,
    /// Text-to-speech (`/v1/audio/speech`).
    Speech,
    /// Speech-to-text (`/v1/audio/transcriptions`).
    Transcription,
    /// Speech translation (`/v1/audio/translations`).
    Translation,
    /// Text-to-image (`/v1/images/generations`).
    ImageGeneration,
    /// Image editing / image-to-image (`/v1/images/edits`).
    ImageEdit,
    /// Batched embeddings (`/v1/embeddings`).
    Embeddings,
    /// Asynchronous video generation (`/v1/videos`).
    VideoGeneration,
    /// Real-time audio transport. Declared for completeness; no adapter yet.
    RealtimeAudio,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Chat => "chat",
            Capability::Vision => "vision",
            Capability::Speech => "speech",
            Capability::Transcription => "transcription",
            Capability::Translation => "translation",
            Capability::ImageGeneration => "image_generation",
            Capability::ImageEdit => "image_edit",
            Capability::Embeddings => "embeddings",
            Capability::VideoGeneration => "video_generation",
            Capability::RealtimeAudio => "realtime_audio",
        }
    }

    pub fn parse(s: &str) -> Option<Capability> {
        Some(match s {
            "chat" => Capability::Chat,
            "vision" => Capability::Vision,
            "speech" => Capability::Speech,
            "transcription" => Capability::Transcription,
            "translation" => Capability::Translation,
            "image_generation" => Capability::ImageGeneration,
            "image_edit" => Capability::ImageEdit,
            "embeddings" => Capability::Embeddings,
            "video_generation" => Capability::VideoGeneration,
            "realtime_audio" => Capability::RealtimeAudio,
            _ => return None,
        })
    }
}

/// An ordered, de-duplicated set of capabilities.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CapabilitySet(Vec<Capability>);

impl CapabilitySet {
    /// Chat only: the baseline every provider offers.
    pub fn chat_only() -> Self {
        Self(vec![Capability::Chat])
    }

    pub fn of(caps: &[Capability]) -> Self {
        let mut v: Vec<Capability> = caps.to_vec();
        v.sort();
        v.dedup();
        Self(v)
    }

    pub fn contains(&self, cap: Capability) -> bool {
        self.0.contains(&cap)
    }

    pub fn with(mut self, cap: Capability) -> Self {
        if !self.contains(cap) {
            self.0.push(cap);
            self.0.sort();
        }
        self
    }

    pub fn iter(&self) -> impl Iterator<Item = Capability> + '_ {
        self.0.iter().copied()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.0.iter().map(|c| c.as_str()).collect()
    }
}

/// A complete binary response (for example transcription text or an image
/// JSON body) with its upstream content type.
#[derive(Debug, Clone)]
pub struct MediaResponse {
    pub content_type: String,
    pub body: axum::body::Bytes,
}

/// A binary response delivered as a stream so large media is never fully
/// buffered in memory by the proxy.
pub struct MediaStream {
    pub content_type: String,
    pub body: BoxStream<'static, Result<axum::body::Bytes, ProviderError>>,
}

/// One file part of a multipart upload forwarded to a provider.
#[derive(Debug, Clone)]
pub struct MultipartFile {
    pub field: String,
    pub filename: String,
    pub content_type: String,
    pub data: axum::body::Bytes,
}

/// Multipart payload: plain text fields plus file parts.
#[derive(Debug, Clone, Default)]
pub struct MultipartRequest {
    pub fields: Vec<(String, String)>,
    pub files: Vec<MultipartFile>,
}

/// Text-to-speech.
#[async_trait]
pub trait SpeechProvider: Send + Sync {
    /// `body` is the caller's JSON request, forwarded with provider-specific
    /// options intact.
    async fn speech(&self, api_key: &str, body: &Value) -> Result<MediaStream, ProviderError>;
}

/// Speech-to-text and speech translation.
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    async fn transcribe(
        &self,
        api_key: &str,
        request: MultipartRequest,
    ) -> Result<MediaResponse, ProviderError>;

    async fn translate(
        &self,
        api_key: &str,
        request: MultipartRequest,
    ) -> Result<MediaResponse, ProviderError>;
}

/// Text-to-image and image editing.
#[async_trait]
pub trait ImageProvider: Send + Sync {
    async fn generate(&self, api_key: &str, body: &Value) -> Result<Value, ProviderError>;
    async fn edit(&self, api_key: &str, request: MultipartRequest) -> Result<Value, ProviderError>;
}

/// Batched embeddings.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, api_key: &str, body: &Value) -> Result<Value, ProviderError>;
}

/// Provider-side state of an asynchronous video job.
#[derive(Debug, Clone)]
pub struct VideoJob {
    pub remote_id: String,
    /// Normalised status: `queued`, `in_progress`, `completed`, `failed`.
    pub status: String,
    pub error: Option<String>,
    pub raw: Value,
}

/// Asynchronous video generation.
#[async_trait]
pub trait VideoProvider: Send + Sync {
    async fn create(&self, api_key: &str, body: &Value) -> Result<VideoJob, ProviderError>;
    async fn status(&self, api_key: &str, remote_id: &str) -> Result<VideoJob, ProviderError>;
    async fn content(&self, api_key: &str, remote_id: &str) -> Result<MediaStream, ProviderError>;
    async fn delete(&self, api_key: &str, remote_id: &str) -> Result<(), ProviderError>;
}

/// Error for an operation the provider does not implement. Never silently
/// downgraded to another operation.
pub fn unsupported(provider: &str, cap: Capability) -> ProviderError {
    ProviderError {
        status: Some(400),
        message: format!("Provider '{provider}' does not support '{}'", cap.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_names_round_trip() {
        for cap in [
            Capability::Chat,
            Capability::Vision,
            Capability::Speech,
            Capability::Transcription,
            Capability::Translation,
            Capability::ImageGeneration,
            Capability::ImageEdit,
            Capability::Embeddings,
            Capability::VideoGeneration,
            Capability::RealtimeAudio,
        ] {
            assert_eq!(Capability::parse(cap.as_str()), Some(cap));
        }
        assert_eq!(Capability::parse("teleport"), None);
    }

    #[test]
    fn capability_set_is_sorted_and_deduplicated() {
        let set = CapabilitySet::of(&[Capability::Speech, Capability::Chat, Capability::Speech]);
        assert_eq!(set.names(), vec!["chat", "speech"]);
        assert!(set.contains(Capability::Speech));
        assert!(!set.contains(Capability::Embeddings));
        assert_eq!(
            CapabilitySet::chat_only().with(Capability::Chat).names(),
            vec!["chat"]
        );
    }

    #[test]
    fn unsupported_error_is_explicit_and_not_retryable() {
        let err = unsupported("groq", Capability::Embeddings);
        assert_eq!(err.status, Some(400));
        assert!(err.message.contains("embeddings"));
        assert!(!err.is_retryable());
    }
}
