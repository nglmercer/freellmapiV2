### Phase 1 — Security (hours)
1. Replace `cors(origin)` with env-allowlisted origin in `app.ts`
2. Require `apiKeyAuth` on all `routes/settings.ts` handlers
3. Validate `ENCRYPTION_KEY` at module load and assert 64-char hex
4. Confirm Google auth only uses `Authorization` header (audit `google.ts` fetch options)
5. Strip keys/account IDs from all upstream error responses
6. Delete `middleware/errorHandler.ts` (dead Express code)

### Phase 2 — Reliability (hours)
7. `await initDb()` in `index.ts`
8. Fix `timingSafeStringEqual` null guard on `unifiedKey` in `routes/middleware.ts`
9. Add concurrency limit (`pMap`-style) to `checkAllKeys`
10. TTL-stale-entries in `windows` / `cooldowns` maps in `ratelimit.ts`

### Phase 3 — Performance (days)
11. Fix N+1 in `routeRequest`: pre-fetch keys for all models in chain in one query
12. Batch SSE commits in `PlaygroundPage` (every 8 chunks or 200ms)
13. Add connection pooling to all providers
14. Migration version tracking in `db/index.ts`
15. `extractStatus` robust non-Error handling

### Phase 4 — Code Quality / Maintainability (days)
16. Refactor `routeRequest` into 3 focused helpers
17. Add response types to all analytics queries (remove `any`)
18. Replace inline `tooltipContentStyle` objects with module-level constants
19. Add `role="log" aria-live` to Playground chat feed
20. Typed `ApiError` wrapper in `lib/api.ts` with `status`/`body` fields

### Phase 5 — Observability & Infra (week)
21. Replace `console.log` with `pino` structured logger
22. Add `/healthz` + `/readyz` without auth
23. CI: add `tsc --noEmit`, `eslint`, and `npm run lint` steps
24. Add integration tests for `routeRequest` with in-memory DB fixture
25. `crypto/ts`, `handler/ts`, `middleware/ts` test coverage gap fill
