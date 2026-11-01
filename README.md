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
launch the browser is taken through the normal account setup. The launcher
generates a random shutdown token for each run and passes it to the server
through its environment, so graceful shutdown works without any credential on
disk.

For distribution, place these files in one application directory:

```text
freellmapi-tray(.exe)
server(.exe)
resources/client/dist/index.html and the built assets
```

The launcher also accepts `FREELLMAPI_SERVER_BIN`, `FREELLMAPI_STATIC_DIR`, and
`FREELLMAPI_DATA_DIR` when a different layout or data location is required.

### Linux desktop tray

The tray launcher requires a graphical Linux session and GTK 3/AppIndicator
runtime support. Installing the libraries alone is not sufficient when no
graphical session is available.

Build dependencies on Debian/Ubuntu:

```bash
sudo apt install libgtk-3-dev libappindicator3-dev libxdo-dev
```

The launcher runs its GTK tray event loop separately from the server and opens
the dashboard in the user's normal system browser. For SSH, headless, or
server deployments, use the `server` binary instead:

```bash
cargo run -p server
```

## Configuration

No `.env` file is required. Every setting is optional and has a default.

- `PORT` — HTTP port; defaults to `3001`.
- `BIND_ADDRESS` — listen address; defaults to `127.0.0.1`. Set `0.0.0.0` only when the deployment intentionally exposes the service.
- `ENCRYPTION_KEY` — optional 64 hexadecimal characters (32 bytes) for AES-256-GCM provider-key encryption. When unset, a random key is generated on first start and stored in the database.
- `BETTER_AUTH_SECRET` — optional secret (at least 32 characters) that signs dashboard sessions and hashes API keys. When unset, a random secret is generated on first start and stored in the database.
- `DASHBOARD_ORIGINS` — optional comma-separated browser-origin allowlist for cross-origin dashboard requests. Same-origin use needs no setting. `localhost:5173` and `127.0.0.1:5173` are always trusted for the Vite dev server.
- `ARTIFICIAL_ANALYSIS_API_KEY` — optional Artificial Analysis Data API key. The default endpoint is the Free integration at `/api/v2/language/models/free`.
- `ARTIFICIAL_ANALYSIS_URL` — optional endpoint override for Pro/commercial or compatible deployments.
- `DB_PATH` — explicit SQLite database path.
- `STATIC_DIR` — directory containing the built client assets.
- `RUST_LOG` — tracing filter, for example `info,tower_http=debug`.

A `.env` file in the project root is loaded when present, so these variables can
also be kept there. See `.env.example` for the list. Never commit real keys.

### Accounts and API keys

Authentication uses [Better Auth for Rust](https://github.com/nglmercer/better-auth-rs).

- **Dashboard:** the first visit asks for a username and password and creates
  the single owner account. Once it exists, the dashboard asks for the same
  username and password. The session is an `HttpOnly` cookie; sign out from the
  header.
- **Join code:** on the Keys page, generate a fast-join code. Enter it on the
  sign-in screen's "Join code" tab, or open the join link
  (`/?join=CODE`), to sign in on another device without the password. Generating
  a new code invalidates the old one; revoke removes it. The code is shown only
  once and stored only as a hash. Treat it like the password.
- **OpenAI-compatible API (`/v1/*`):** create API keys on the Keys page. Send one
  as `Authorization: Bearer <key>` or `x-api-key: <key>`. Keys can be revoked
  there. A dashboard session also works, which is how the playground calls `/v1`.
- Browser-based local development with Vite (`npm run dev:client`) works with no
  extra configuration.

### Database compatibility

Existing provider keys and models are preserved. When `DB_PATH` is not set, the server first uses an existing legacy database at `server/data/freeapi.db`; if it does not exist, it creates the new default at `data/freeapi.db`. Runtime state is stored as `runtime-state.json` beside the selected database. The migration creates missing tables and columns in place and is idempotent; it does not reset or recreate an existing database.

The authentication change is not backward compatible. The old `ADMIN_API_KEY` and the unified proxy key are no longer read. After upgrading, open the dashboard, create the owner account, and create new API keys for your clients.

## API

Public routes: `/api/ping`, `/api/account-status` (reports whether the owner
account exists), `/api/account/create` (creates the owner from a username and
password, once), `/api/account/join` (signs in with a join code), and the Better
Auth routes under `/api/auth/*` (username sign-in at `/api/auth/sign-in/username`,
sign-out, session lookup, and API-key management at `/api/auth/api-key/*`).

The join code is managed at `/api/account/join-code` (`GET` status, `POST` to
generate, `DELETE` to revoke) and requires a dashboard session.

Dashboard routes (`/api/keys`, `/api/models`, `/api/fallback`, `/api/analytics`,
`/api/health`, `/api/settings`, `/api/providers`, and the rest of `/api/*`)
require a signed-in session cookie. `/api/shutdown` accepts that session or the
launcher's per-run token; it requests a graceful stop.

OpenAI-compatible routes (`/v1/models`, `/v1/chat/completions`, and
`/v1/completions`) require an API key or a dashboard session.

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

## Model intelligence

Open **Models** for the canonical model explorer, deployment evidence, source controls,
comparison, match review, and routing simulation/policy settings. Artificial Analysis
and Models.dev are the default sources; OpenRouter, LiteLLM, and targeted public Hugging
Face metadata are opt-in. See [model intelligence](docs/model-intelligence.md) for
migration compatibility, API contracts, source attribution, metric units, and validation.
