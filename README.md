# FreeLLMAPI V2

FreeLLMAPI is a Rust backend with an OpenAI-compatible API and a React/Vite administration client. It aggregates free-tier models from Google, Groq, Cerebras, SambaNova, NVIDIA, Mistral, OpenRouter, GitHub Models, Cohere, Cloudflare, Z.ai, and custom OpenAI-compatible providers.

## Architecture

- `crates/server` — Rust HTTP server, provider adapters, routing, SQLite persistence, health checks, analytics, and graceful shutdown.
- `crates/getmodelsapi` — Rust model discovery, scraping, filtering, enrichment, caching, and serialization.
- `client` — React/Vite/TypeScript dashboard.
- `shared` — TypeScript types shared with the dashboard.

The backend and model-discovery implementations are entirely Rust. The frontend remains TypeScript as intended.

## Requirements

- Rust stable
- Node.js 20+
- npm
- SQLite support provided by the Rust dependencies

## Development

```bash
git clone https://github.com/nglmercer/freellmapiV2
cd freellmapiV2

npm install
npm run build -w client

cargo run -p server
```

The API listens on port `3001` by default. The dashboard development server can be started with `npm run dev -w client`.

## Rust checks

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Network-dependent provider smoke tests are intentionally excluded from normal tests. Run them explicitly with `GETMODELS_NETWORK_TESTS=1` when validating provider availability.

## Release build

```bash
npm run build -w client
cargo build --release -p server
./target/release/server
```

## Configuration

Supported environment variables are:

- `ENCRYPTION_KEY` — 64 hexadecimal characters (32 bytes) used for AES-256-GCM provider-key encryption.
- `PORT` — HTTP port; defaults to `3001`.
- `DB_PATH` — explicit SQLite database path.
- `STATIC_DIR` — directory containing the built client assets.
- `RUST_LOG` — tracing filter, for example `info,tower_http=debug`.

Copy `.env.example` to `.env` and set `ENCRYPTION_KEY` for a deployment that already has encrypted provider keys. Never commit real keys.

### Database compatibility

Existing installations are preserved. When `DB_PATH` is not set, the server first uses an existing legacy database at `server/data/freeapi.db`; if it does not exist, it creates the new default at `data/freeapi.db`. Runtime state is stored as `runtime-state.json` beside the selected database. The migration creates missing tables and columns in place and is idempotent; it does not reset or recreate an existing database.

## API

Administrative routes include `/api/ping`, `/api/keys`, `/api/models`, `/api/fallback`, `/api/analytics`, `/api/health`, `/api/settings`, and `/api/providers`. OpenAI-compatible routes are `/v1/models`, `/v1/chat/completions`, and `/v1/completions`.

The server supports streaming and non-streaming responses, tool calls, multimodal messages, parallel non-streaming choices, fallback routing, sticky sessions, rate limiting, and request analytics.

## Maintenance commands

To probe every currently enabled model/key pair using the configured providers:

```bash
cargo run -p server --bin test_all_models
```

To run the network provider tests locally:

```bash
GETMODELS_NETWORK_TESTS=1 cargo test -p getmodelsapi --test getmodels -- --nocapture
```

## License

[MIT](./LICENSE)
