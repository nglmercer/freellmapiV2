import { Hono } from 'hono';
import { getDb } from '../db/index.js';
import * as schema from '../db/schema.js';
import { sql, eq, gte, and, desc, asc } from 'drizzle-orm';

// Map range to a JS-computed ISO timestamp passed as a bind parameter,
// so the SQL string never includes user-controlled fragments.
function getSinceTimestamp(range: string): string {
  const now = Date.now();
  switch (range) {
    case '24h':
      return new Date(now - 24 * 60 * 60 * 1000).toISOString();
    case '30d':
      return new Date(now - 30 * 24 * 60 * 60 * 1000).toISOString();
    case '7d':
    default:
      return new Date(now - 7 * 24 * 60 * 60 * 1000).toISOString();
  }
}

export const analyticsRouter = new Hono();

// Summary stats
analyticsRouter.get('/summary', async (c) => {
  const range = c.req.query('range') ?? '7d';
  const since = getSinceTimestamp(range);
  const db = getDb();

  const stats = db.select({
    total_requests: sql<number>`COUNT(*)`,
    success_count: sql<number>`SUM(CASE WHEN ${schema.requests.status} = 'success' THEN 1 ELSE 0 END)`,
    total_input_tokens: sql<number>`SUM(${schema.requests.inputTokens})`,
    total_output_tokens: sql<number>`SUM(${schema.requests.outputTokens})`,
    avg_latency_ms: sql<number>`AVG(${schema.requests.latencyMs})`
  })
  .from(schema.requests)
  .where(gte(schema.requests.createdAt, since))
  .get();

  const totalRequests = stats?.total_requests ?? 0;
  const successRate = totalRequests > 0 ? ((stats?.success_count ?? 0) / totalRequests) * 100 : 0;

  // Estimate cost savings: average ~$3/M input + $15/M output tokens (GPT-4o pricing)
  const inputCost = ((stats?.total_input_tokens ?? 0) / 1_000_000) * 3;
  const outputCost = ((stats?.total_output_tokens ?? 0) / 1_000_000) * 15;

  return c.json({
    totalRequests,
    successRate: Math.round(successRate * 10) / 10,
    totalInputTokens: stats?.total_input_tokens ?? 0,
    totalOutputTokens: stats?.total_output_tokens ?? 0,
    avgLatencyMs: Math.round(stats?.avg_latency_ms ?? 0),
    estimatedCostSavings: Math.round((inputCost + outputCost) * 100) / 100,
  });
});

// Stats grouped by model
analyticsRouter.get('/by-model', async (c) => {
  const range = c.req.query('range') ?? '7d';
  const since = getSinceTimestamp(range);
  const db = getDb();

  const requestsCount = sql<number>`COUNT(*)`;
  const rows = db.select({
    platform: schema.requests.platform,
    modelId: schema.requests.modelId,
    displayName: schema.models.displayName,
    requests: requestsCount,
    success_rate: sql<number>`SUM(CASE WHEN ${schema.requests.status} = 'success' THEN 1 ELSE 0 END) * 100.0 / ${requestsCount}`,
    avg_latency_ms: sql<number>`AVG(${schema.requests.latencyMs})`,
    total_input_tokens: sql<number>`SUM(${schema.requests.inputTokens})`,
    total_output_tokens: sql<number>`SUM(${schema.requests.outputTokens})`
  })
  .from(schema.requests)
  .leftJoin(schema.models, and(eq(schema.models.platform, schema.requests.platform), eq(schema.models.modelId, schema.requests.modelId)))
  .where(gte(schema.requests.createdAt, since))
  .groupBy(schema.requests.platform, schema.requests.modelId)
  .orderBy(desc(requestsCount))
  .all();

  return c.json(rows.map(r => ({
    platform: r.platform,
    modelId: r.modelId,
    displayName: r.displayName ?? r.modelId,
    requests: r.requests,
    successRate: Math.round((r.success_rate ?? 0) * 10) / 10,
    avgLatencyMs: Math.round(r.avg_latency_ms ?? 0),
    totalInputTokens: r.total_input_tokens ?? 0,
    totalOutputTokens: r.total_output_tokens ?? 0,
  })));
});

// Stats grouped by platform
analyticsRouter.get('/by-platform', async (c) => {
  const range = c.req.query('range') ?? '7d';
  const since = getSinceTimestamp(range);
  const db = getDb();

  const requestsCount = sql<number>`COUNT(*)`;
  const rows = db.select({
    platform: schema.requests.platform,
    requests: requestsCount,
    success_rate: sql<number>`SUM(CASE WHEN ${schema.requests.status} = 'success' THEN 1 ELSE 0 END) * 100.0 / ${requestsCount}`,
    avg_latency_ms: sql<number>`AVG(${schema.requests.latencyMs})`,
    total_input_tokens: sql<number>`SUM(${schema.requests.inputTokens})`,
    total_output_tokens: sql<number>`SUM(${schema.requests.outputTokens})`
  })
  .from(schema.requests)
  .where(gte(schema.requests.createdAt, since))
  .groupBy(schema.requests.platform)
  .orderBy(desc(requestsCount))
  .all();

  return c.json(rows.map(r => ({
    platform: r.platform,
    requests: r.requests,
    successRate: Math.round((r.success_rate ?? 0) * 10) / 10,
    avgLatencyMs: Math.round(r.avg_latency_ms ?? 0),
    totalInputTokens: r.total_input_tokens ?? 0,
    totalOutputTokens: r.total_output_tokens ?? 0,
  })));
});

// Timeline data
analyticsRouter.get('/timeline', async (c) => {
  const range = c.req.query('range') ?? '7d';
  const interval = c.req.query('interval') ?? (range === '24h' ? 'hour' : 'day');
  const since = getSinceTimestamp(range);
  const db = getDb();

  // dateFormat is a hardcoded whitelist — never user-controlled.
  const dateFormat = interval === 'hour' ? '%Y-%m-%dT%H:00:00' : '%Y-%m-%d';

  const timestampSql = sql<string>`strftime(${dateFormat}, ${schema.requests.createdAt})`;
  const rows = db.select({
    timestamp: timestampSql,
    requests: sql<number>`COUNT(*)`,
    success_count: sql<number>`SUM(CASE WHEN ${schema.requests.status} = 'success' THEN 1 ELSE 0 END)`,
    failure_count: sql<number>`SUM(CASE WHEN ${schema.requests.status} = 'error' THEN 1 ELSE 0 END)`
  })
  .from(schema.requests)
  .where(gte(schema.requests.createdAt, since))
  .groupBy(timestampSql)
  .orderBy(asc(timestampSql))
  .all();

  return c.json(rows.map(r => ({
    timestamp: r.timestamp,
    requests: r.requests,
    successCount: r.success_count ?? 0,
    failureCount: r.failure_count ?? 0,
  })));
});

// Error distribution (grouped by error type and platform)
analyticsRouter.get('/error-distribution', async (c) => {
  const range = c.req.query('range') ?? '7d';
  const since = getSinceTimestamp(range);
  const db = getDb();

  const errorCategorySql = sql`
    CASE
      WHEN ${schema.requests.error} LIKE '%429%' OR ${schema.requests.error} LIKE '%rate limit%' OR ${schema.requests.error} LIKE '%too many%' OR ${schema.requests.error} LIKE '%quota%' THEN 'Rate Limited (429)'
      WHEN ${schema.requests.error} LIKE '%401%' OR ${schema.requests.error} LIKE '%unauthorized%' OR ${schema.requests.error} LIKE '%invalid.*key%' THEN 'Auth Error (401)'
      WHEN ${schema.requests.error} LIKE '%403%' OR ${schema.requests.error} LIKE '%forbidden%' THEN 'Forbidden (403)'
      WHEN ${schema.requests.error} LIKE '%404%' OR ${schema.requests.error} LIKE '%not found%' THEN 'Not Found (404)'
      WHEN ${schema.requests.error} LIKE '%timeout%' OR ${schema.requests.error} LIKE '%ETIMEDOUT%' OR ${schema.requests.error} LIKE '%ECONNREFUSED%' THEN 'Timeout/Connection'
      WHEN ${schema.requests.error} LIKE '%500%' OR ${schema.requests.error} LIKE '%internal server%' THEN 'Server Error (500)'
      WHEN ${schema.requests.error} LIKE '%503%' OR ${schema.requests.error} LIKE '%unavailable%' THEN 'Unavailable (503)'
      ELSE 'Other'
    END`;

  const countSql = sql<number>`COUNT(*)`;
  // Group errors by category (extract the key part of the error message)
  const rows = db.select({
    platform: schema.requests.platform,
    modelId: schema.requests.modelId,
    error_category: errorCategorySql,
    count: countSql
  })
  .from(schema.requests)
  .where(and(eq(schema.requests.status, 'error'), gte(schema.requests.createdAt, since)))
  .groupBy(schema.requests.platform, errorCategorySql)
  .orderBy(desc(countSql))
  .all();

  // Also get totals by category
  const byCategory = db.select({
    category: errorCategorySql,
    count: countSql
  })
  .from(schema.requests)
  .where(and(eq(schema.requests.status, 'error'), gte(schema.requests.createdAt, since)))
  .groupBy(errorCategorySql)
  .orderBy(desc(countSql))
  .all();

  // Errors by platform
  const byPlatform = db.select({
    platform: schema.requests.platform,
    count: countSql
  })
  .from(schema.requests)
  .where(and(eq(schema.requests.status, 'error'), gte(schema.requests.createdAt, since)))
  .groupBy(schema.requests.platform)
  .orderBy(desc(countSql))
  .all();

  return c.json({
    byCategory,
    byPlatform,
    detailed: rows,
  });
});

// Recent errors
analyticsRouter.get('/errors', async (c) => {
  const range = c.req.query('range') ?? '7d';
  const since = getSinceTimestamp(range);
  const db = getDb();

  const rows = db.select({
    id: schema.requests.id,
    platform: schema.requests.platform,
    modelId: schema.requests.modelId,
    error: schema.requests.error,
    latencyMs: schema.requests.latencyMs,
    createdAt: schema.requests.createdAt
  })
  .from(schema.requests)
  .where(and(eq(schema.requests.status, 'error'), gte(schema.requests.createdAt, since)))
  .orderBy(desc(schema.requests.createdAt))
  .limit(50)
  .all();

  return c.json(rows.map(r => ({
    id: r.id,
    platform: r.platform,
    modelId: r.modelId,
    error: r.error,
    latencyMs: r.latencyMs,
    createdAt: r.createdAt,
  })));
});
