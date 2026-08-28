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
npm run build:client

cargo run -p server
```

The API listens on `127.0.0.1:3001` by default. The dashboard development server can be started with `npm run dev:client`.

## Rust checks

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Network-dependent provider smoke tests are intentionally excluded from normal tests. Run them explicitly with `GETMODELS_NETWORK_TESTS=1` when validating provider availability.

## Release build

```bash
npm run build:client
cargo build --release -p server
./target/release/server
```

### Desktop tray launcher

The optional desktop build is a native tray-only launcher. It starts the
server on `127.0.0.1`, opens the dashboard in the user's existing browser, and
does not embed a webview or require Tauri. No manually created `.env` file is
needed: the launcher keeps generated runtime state in the platform's local
application-data directory.

```bash
npm run build:desktop
./target/release/freellmapi-tray
```

The tray menu provides dashboard, setup, restart, and quit actions. On first
launch the browser is taken through the normal setup page. The launcher passes
the local admin credential in a URL fragment; the dashboard consumes it and
removes it from the address bar before making API requests.

For distribution, place these files in one application directory:

```text
freellmapi-tray(.exe)
server(.exe)
resources/client/dist/index.html and the built assets
```

The launcher also accepts `FREELLMAPI_SERVER_BIN`, `FREELLMAPI_STATIC_DIR`, and
`FREELLMAPI_DATA_DIR` when a different layout or data location is required.
Linux builds need the GTK 3 and AppIndicator development libraries used by the
native tray integration.

## Configuration

Supported environment variables are:

- `ENCRYPTION_KEY` — 64 hexadecimal characters (32 bytes) used for AES-256-GCM provider-key encryption.
- `ADMIN_API_KEY` — separate credential required for every administrative `/api/*` route except `/api/ping`. If omitted from a local `.env`, startup generates one.
- `PORT` — HTTP port; defaults to `3001`.
- `BIND_ADDRESS` — listen address; defaults to `127.0.0.1`. Set `0.0.0.0` only when the deployment intentionally exposes the service.
- `DASHBOARD_ORIGINS` — optional comma-separated browser-origin allowlist for cross-origin dashboard requests. CORS is denied by default.
- `ARTIFICIAL_ANALYSIS_API_KEY` — optional Artificial Analysis Data API key. The default endpoint is the Free integration at `/api/v2/language/models/free`.
- `ARTIFICIAL_ANALYSIS_URL` — optional endpoint override for Pro/commercial or compatible deployments.
- `DB_PATH` — explicit SQLite database path.
- `STATIC_DIR` — directory containing the built client assets.
- `RUST_LOG` — tracing filter, for example `info,tower_http=debug`.

Copy `.env.example` to `.env` and set `ENCRYPTION_KEY` and `ADMIN_API_KEY` for a deployment that already has encrypted provider keys. Never commit real keys.

The dashboard prompts for `ADMIN_API_KEY` when the server is not built with a `VITE_ADMIN_API_KEY`; the entered credential is kept only in the browser session. For local Vite development, set `DASHBOARD_ORIGINS=http://localhost:5173,http://127.0.0.1:5173` and optionally set `VITE_ADMIN_API_KEY` to avoid the prompt. Do not publish the admin credential in a broadly accessible production bundle. The OpenAI-compatible `/v1/*` routes use the separate unified API key returned by the protected Settings endpoints.

### Database compatibility

Existing installations are preserved. When `DB_PATH` is not set, the server first uses an existing legacy database at `server/data/freeapi.db`; if it does not exist, it creates the new default at `data/freeapi.db`. Runtime state is stored as `runtime-state.json` beside the selected database. The migration creates missing tables and columns in place and is idempotent; it does not reset or recreate an existing database.

## API

`/api/ping` is public. All other administrative routes (`/api/keys`, `/api/models`, `/api/fallback`, `/api/analytics`, `/api/health`, `/api/settings`, and `/api/providers`) require `Authorization: Bearer <ADMIN_API_KEY>`. OpenAI-compatible routes (`/v1/models`, `/v1/chat/completions`, and `/v1/completions`) require `Authorization: Bearer <unified API key>`.

Ranking enrichment uses the Artificial Analysis Free endpoint by default and reuses a successful source snapshot for 24 hours during scheduled model syncs. The administrative ranking-sync endpoint forces a fresh fetch.

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
