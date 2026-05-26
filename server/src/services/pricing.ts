import { getDb } from '../db/index.js';
import * as schema from '../db/schema.js';
import { eq, and } from 'drizzle-orm';

export interface CostEstimate {
  promptCost: number;
  completionCost: number;
  totalCost: number;
  currency: string;
  pricingPrompt: number | null;
  pricingCompletion: number | null;
}

export function calculateCost(
  platform: string,
  modelId: string,
  promptTokens: number,
  completionTokens: number,
): CostEstimate {
  const db = getDb();
  const model = db.select({
    pricingPrompt: schema.models.pricingPrompt,
    pricingCompletion: schema.models.pricingCompletion,
  })
    .from(schema.models)
    .where(and(eq(schema.models.platform, platform), eq(schema.models.modelId, modelId)))
    .get();

  const pricingPrompt = model?.pricingPrompt ?? null;
  const pricingCompletion = model?.pricingCompletion ?? null;

  if (pricingPrompt == null && pricingCompletion == null) {
    return { promptCost: 0, completionCost: 0, totalCost: 0, currency: 'USD', pricingPrompt: null, pricingCompletion: null };
  }

  const promptCost = promptTokens * (pricingPrompt ?? 0);
  const completionCost = completionTokens * (pricingCompletion ?? 0);

  return {
    promptCost: Math.round(promptCost * 1_000_000) / 1_000_000,
    completionCost: Math.round(completionCost * 1_000_000) / 1_000_000,
    totalCost: Math.round((promptCost + completionCost) * 1_000_000) / 1_000_000,
    currency: 'USD',
    pricingPrompt,
    pricingCompletion,
  };
}

export function getAllPricing(): Array<{
  platform: string;
  modelId: string;
  displayName: string;
  pricingPrompt: number | null;
  pricingCompletion: number | null;
  freeTier: boolean;
}> {
  const db = getDb();
  return db.select({
    platform: schema.models.platform,
    modelId: schema.models.modelId,
    displayName: schema.models.displayName,
    pricingPrompt: schema.models.pricingPrompt,
    pricingCompletion: schema.models.pricingCompletion,
    freeTier: schema.models.freeTier,
  })
    .from(schema.models)
    .where(eq(schema.models.enabled, 1))
    .all()
    .map(m => ({
      ...m,
      freeTier: m.freeTier === 1,
    }));
}
