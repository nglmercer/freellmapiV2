import http from "../utils/http";
import type { Model } from "../types";
import { cacheGet, cacheSet, ONE_HOUR } from "./cache";

interface OpenRouterModel {
  id: string;
  context_length: number;
}

async function getGoogleContextWindows(): Promise<Map<string, number>> {
  const cacheKey = "crossref-openrouter-context";
  const cached = cacheGet<Map<string, number>>(cacheKey);
  if (cached) return cached;

  const map = new Map<string, number>();

  try {
    const response = await http.get("https://openrouter.ai/api/v1/models", {
      timeout: 15000,
    });
    if (Array.isArray(response.data?.data)) {
      for (const m of response.data.data as OpenRouterModel[]) {
        if (m.id.startsWith("google/") && m.context_length > 0) {
          // "google/gemini-2.5-flash" → "gemini-2.5-flash"
          const name = m.id.replace("google/", "");
          map.set(name, m.context_length);
          // Also index by slug variants
          map.set(name.replace(/-preview-\d{2}-\d{4}$/, ""), m.context_length);
        }
      }
    }
  } catch {
    // OpenRouter may be down
  }

  cacheSet(cacheKey, map, ONE_HOUR);
  return map;
}

async function scrapeGoogleModelPage(modelId: string): Promise<number | null> {
  try {
    const url = `https://ai.google.dev/gemini-api/docs/models/${modelId}`;
    const response = await http.get(url, {
      headers: { "User-Agent": "Mozilla/5.0 GetModels" },
      timeout: 8000,
    });

    const html = response.data as string;

    // Try "1M token context window" style
    const shorthand = html.match(/(\d+)\s*[mM]\s*(?:token|context)/i);
    if (shorthand?.[1]) return parseInt(shorthand[1], 10) * 1_000_000;

    // Try "128k token" style
    const kStyle = html.match(/(\d+)\s*[kK]\s*(?:token|context)/i);
    if (kStyle?.[1]) return parseInt(kStyle[1], 10) * 1_000;

    // Try raw number with "input token"
    const inputMatch = html.match(/input\s+token\s+limit[^<]*?(\d[\d,]*)/i);
    if (inputMatch?.[1]) return parseInt(inputMatch[1].replace(/,/g, ""), 10);

    // Try context window number
    const ctxMatch = html.match(
      /context\s*(?:length|window)\s*(?::|is|of)?\s*(\d[\d,]*)/i,
    );
    if (ctxMatch?.[1]) return parseInt(ctxMatch[1].replace(/,/g, ""), 10);

    return null;
  } catch {
    return null;
  }
}

async function enrichGoogleModel(model: Model): Promise<Model> {
  if (model.contextWindow > 40000) return model; // already enriched

  const cacheKey = `enrich-google-${model.id}`;
  const cached = cacheGet<Pick<Model, "contextWindow">>(cacheKey);
  if (cached) return { ...model, ...cached };

  // Method 1: Cross-reference with OpenRouter (cached)
  const ctxMap = await getGoogleContextWindows();
  const ctx = ctxMap.get(model.id);
  if (ctx && ctx > model.contextWindow) {
    cacheSet(cacheKey, { contextWindow: ctx }, ONE_HOUR);
    return { ...model, contextWindow: ctx };
  }

  // Method 2: Direct page scrape for unknown models
  const scraped = await scrapeGoogleModelPage(model.id);
  if (scraped && scraped > model.contextWindow) {
    cacheSet(cacheKey, { contextWindow: scraped }, ONE_HOUR);
    return { ...model, contextWindow: scraped };
  }

  return model;
}

export async function enrichModel(model: Model): Promise<Model> {
  if (model.provider === "google" && model.contextWindow < 40000) {
    return enrichGoogleModel(model);
  }
  return model;
}

export async function enrichModels(models: Model[]): Promise<Model[]> {
  return Promise.all(models.map(enrichModel));
}
