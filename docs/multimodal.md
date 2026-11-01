# Multimodal API

FreeLLMAPI exposes OpenAI-compatible endpoints for chat, speech, transcription,
images, embeddings and video. Every non-chat operation is opt-in: a model serves
an operation only when a provider declares support for it **and** the model is
declared for that operation. Nothing is hardcoded.

## Endpoints

| Endpoint | Operation | Notes |
| --- | --- | --- |
| `POST /v1/chat/completions` | Chat, vision, audio and video understanding | Image, `input_audio` and `video_url` parts are validated and passed through. |
| `POST /v1/audio/speech` | Text-to-speech | JSON. Upstream audio is streamed, not buffered. |
| `POST /v1/audio/transcriptions` | Speech-to-text | Multipart, exactly one `file` part. |
| `POST /v1/audio/translations` | Speech translation | Multipart, exactly one `file` part. |
| `POST /v1/images/generations` | Text-to-image | JSON, forwarded with provider options. |
| `POST /v1/images/edits` | Image editing | Multipart, one or more image parts and an optional mask. |
| `POST /v1/embeddings` | Batched embeddings | `input` is a string or an array; at most 2048 items. |
| `POST /v1/videos` | Start video generation | Returns a durable job. Supports `Idempotency-Key`. |
| `GET /v1/videos/{id}` | Job status | Refreshes active jobs from the provider. |
| `GET /v1/videos/{id}/content` | Download | 409 until completed, 410 after expiry. |
| `DELETE /v1/videos/{id}` | Cancel or delete | Idempotent. |
| `GET /v1/models` | Discovery | Chat entries carry `capabilities`. `?capability=speech` lists declared models for an operation. |
| `GET /v1/models` (unchanged) | Chat models | Backward compatible. |

Authentication is the same as the rest of `/v1`: an API key or a dashboard session.

## Declaring models

```http
POST /api/media-models
{ "platform": "groq", "modelId": "whisper-large-v3-turbo", "capability": "transcription", "priority": 0 }
```

The provider must support the capability. Declaring an unsupported pair returns
`400 Provider '…' does not support '…'`. Models are removed with
`DELETE /api/media-models/{id}`, and the dashboard's **Media** page manages them.

## Capabilities

| Capability | Built-in providers | Custom endpoints |
| --- | --- | --- |
| `chat` | All chat providers | Always |
| `vision`, `audio_input`, `video_input` | Routed by verified model metadata | Routed by verified model metadata |
| `speech` | Groq | Declared by the endpoint |
| `transcription`, `translation` | Groq | Declared by the endpoint |
| `embeddings` | Mistral | Declared by the endpoint |
| `image_generation`, `image_edit` | None | Declared by the endpoint |
| `video_generation` | None | Declared by the endpoint |

Custom providers declare capabilities when they are created or updated:

```json
{ "name": "my-tts", "baseUrl": "https://tts.example.test/v1", "capabilities": ["speech"] }
```

Built-in declarations are based on the providers' documented APIs and have not
been verified against live accounts in this repository. Run the opt-in live
tests (below) before relying on them.

## Routing and fallback

- Candidates come only from `media_models`, filtered by provider capability and
  by enabled, non-invalid keys.
- Chat requests that contain image, audio or video parts are routed only to models
  whose metadata verifies that capability. If none qualifies, the request fails with
  `400 No enabled model has verified support for: vision`.
- Fallback moves to the next candidate only on HTTP 429 or 5xx. Client errors
  (such as an unsupported `response_format`) are returned unchanged.
- Transport failures fall back only for speech and transcription. Image, embedding
  and video creation never retry on an ambiguous transport failure, because the
  provider may already have started a paid job.

## Video jobs

Each job is written to SQLite before the provider is called.

- `Idempotency-Key` (1–200 characters): a retry with the same key returns the
  existing job (200) rather than starting another generation.
- Explicit provider rejection removes the record, so the same key can be used
  again.
- A background sweeper (every 60 seconds) does the following:
  - marks jobs stuck in `submitting` for 10 minutes as failed;
  - expires completed jobs after 24 hours;
  - removes finished job records after 7 days;
  - refreshes up to 20 active jobs per pass.
- Job states: `submitting`, `queued`, `in_progress`, `completed`, `failed`,
  `cancelled`, `deleted`, `expired`.

## Configuration

Set these in `config.toml` (see `config.example.toml`):

| Setting | Default | Purpose |
| --- | --- | --- |
| `limits.v1_max_body_bytes` | 20 MiB | Request body limit for `/v1` chat and embeddings. |
| `limits.v1_media_max_body_bytes` | 64 MiB | Request body limit for audio, image, and video routes. |
| `media.max_concurrency` | 16 | Upstream media calls in flight at once. |

Dashboard and control routes keep a 1 MiB limit.

## Limitations

- **Real-time audio** (`realtime_audio`) is declared as a capability but has no
  adapter. It is not claimed as supported.
- **Video creation** accepts JSON only. Multipart reference-image uploads are not
  implemented. A reference image may be passed as `input_reference`, which must be
  an `http(s)` URL or a base64 image data URL.
- **Costs** for media operations are not estimated: no verified price data is
  stored. Analytics report `costTracked: false` for them.
- **Per-key media quotas** are not enforced. Concurrency is bounded globally.
- **Gemini** adapter: text and image parts only. Other part types are rejected
  explicitly rather than dropped.
- **Remote media** is never fetched by the server, so there is no SSRF surface from
  media URLs. The URLs are forwarded to providers as given.
- **Fallback records**: when a media call falls back, the request log attributes
  the attempt to the first candidate.
- **Model editor filters**: the Model Explorer does not yet filter by modality; the
  Media page does.

## Live provider tests

These make real calls and are off by default:

```sh
FREELLMAPI_LIVE_TESTS=1 GROQ_API_KEY=... cargo test -p server --test provider_smoke -- --ignored
```

The live tests read their keys from the environment, not from `config.toml`.

They check that a key is accepted. They do not check audio, image or video output.
