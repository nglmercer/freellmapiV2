import http from "../../utils/http";
import * as cheerio from "cheerio";
import type { Model, ProviderConfig, ScraperResult } from "./index";

interface CardData {
  id: string;
  rawId: string;
  name: string;
  description: string | undefined;
}

function extractCards($: cheerio.CheerioAPI): CardData[] {
  const cards: CardData[] = [];

  // Layout 1: Compact grid cards
  $(".gemini-model-grid-compact .gemini-model-row").each((_, card) => {
    const heading = $(card).find('h3[id*="gemini"]');
    const rawId = heading.attr("id");
    if (!rawId) return;
    const name = heading.text().trim();
    const desc = $(card).find(".gemini-model-desc").first().text().trim();
    cards.push({
      id: normalizeId(rawId),
      rawId,
      name,
      description: desc || undefined,
    });
  });

  // Layout 2: Centered hero cards
  $(".gemini-centered-model-grid .gemini-card-centered").each((_, card) => {
    const heading = $(card).find('h3[id*="gemini"]');
    const rawId = heading.attr("id");
    if (!rawId) return;
    const name = heading.text().trim();
    const desc = $(card).find(".description-centered").first().text().trim();
    cards.push({
      id: normalizeId(rawId),
      rawId,
      name,
      description: desc || undefined,
    });
  });

  return cards;
}

function normalizeId(rawId: string): string {
  return rawId
    .replace(/-preview(-\d{2}-\d{4})?$/, "")
    .replace(/-deprecated$/, "");
}

function parseContextWindow(text: string): number {
  const match = text.match(/(\d[\d,]*)\s*[kmM]\s*(?:token|context)/i);
  if (match?.[1]) {
    let n = parseInt(match[1].replace(/,/g, ""), 10);
    if (/m/i.test(match[0] ?? "")) n *= 1000000;
    else if (/k/i.test(match[0] ?? "")) n *= 1000;
    return n;
  }
  return 32768;
}

function deduceFeatures(name: string, desc: string, id: string): string[] {
  const text = (name + " " + desc + " " + id).toLowerCase();
  const features: string[] = [];

  if (id.includes("embedding") || text.includes("embedding"))
    return ["embeddings"];

  if (
    text.includes("vision") ||
    text.includes("image") ||
    text.includes("multimodal")
  )
    features.push("vision");
  if (
    text.includes("audio") ||
    text.includes("voice") ||
    (text.includes("speech") && !text.includes("speech synthesis"))
  )
    features.push("audio");
  if (text.includes("video")) features.push("video");
  if (text.includes("code") || text.includes("coding")) features.push("code");
  if (
    text.includes("text-to-speech") ||
    text.includes("speech synthesis") ||
    text.includes("speech generation")
  )
    features.push("tts");
  if (text.includes("reasoning")) features.push("reasoning");

  features.push("chat");
  return features;
}

// Normalize any Google model ID to a comparable key: strip dots/dashes, dates, preview suffixes
function normalizeForMatch(id: string): string {
  return id
    .replace(/^google\//, "") // strip google/ prefix
    .replace(/\./g, "-") // gemini-2.5 → gemini-2-5
    .replace(/-preview.*$/, "") // strip -preview* suffixes
    .replace(/-deprecated$/, "") // deprecated
    .replace(/-latest$/, "")
    .replace(/-0\d{3}$/, "") // trailing -001
    .replace(/-2$/, "") // trailing -2 suffix (duplicate sections)
    .replace(/-shut-down$/, "")
    .replace(/-tts$/, "")
    .replace(/-live$/, "")
    .replace(/-lite$/, "")
    .replace(/-pro$/, "")
    .replace(/-flash$/, "")
    .replace(/-max$/, "")
    .replace(/-research$/, "")
    .replace(/-image$/, "")
    .replace(/-robotics$/, "")
    .replace(/-embedding.*$/, "-embedding")
    .replace(/--+/g, "-") // collapse double dashes
    .replace(/-$/, ""); // strip trailing dash
}

async function getContextFromOpenRouter(): Promise<Map<string, number>> {
  try {
    const response = await http.get("https://openrouter.ai/api/v1/models", {
      timeout: 10000,
    });
    const map = new Map<string, number>();
    if (Array.isArray(response.data?.data)) {
      for (const m of response.data.data) {
        if (m.id?.startsWith("google/") && m.context_length > 0) {
          const key = normalizeForMatch(m.id);
          const existing = map.get(key);
          // Prefer higher context window if multiple models map to same key
          if (!existing || m.context_length > existing) {
            map.set(key, m.context_length);
          }
        }
      }
    }
    return map;
  } catch {
    return new Map();
  }
}

async function scrapeGooglePage(): Promise<Model[]> {
  const [response, ctxMap] = await Promise.all([
    http.get("https://ai.google.dev/gemini-api/docs/models", {
      headers: {
        "User-Agent": "Mozilla/5.0 GetModels",
        "Accept-Language": "en-US,en;q=0.9",
      },
      timeout: 15000,
    }),
    getContextFromOpenRouter(),
  ]);

  const $ = cheerio.load(response.data);
  const cards = extractCards($);
  const seen = new Set<string>();

  return cards
    .filter((c) => {
      if (seen.has(c.id)) return false;
      seen.add(c.id);
      return true;
    })
    .map((c) => {
      const scrapedCtx = parseContextWindow(c.description || "");
      const orCtx =
        ctxMap.get(normalizeForMatch(c.id)) ||
        ctxMap.get(normalizeForMatch(c.rawId));
      const contextWindow = orCtx || scrapedCtx || 32768;

      return {
        id: c.id,
        name: c.name,
        provider: "google" as const,
        contextWindow,
        supportedFeatures: deduceFeatures(c.name, c.description || "", c.id),
        freeTier: true,
        url: `https://ai.google.dev/gemini-api/docs/models#${c.rawId}`,
        description: c.description?.substring(0, 300) || undefined,
      };
    });
}

export async function scrapeGoogle(
  config: ProviderConfig,
): Promise<ScraperResult> {
  if (config.apiKey) {
    try {
      const response = await http.get(`${config.baseUrl}/models`, {
        params: { key: config.apiKey },
        timeout: 10000,
      });

interface GeminiAPIResponse {
  name: string;
  displayName?: string;
  description?: string;
  inputTokenLimit?: number;
  outputTokenLimit?: number;
  supportedGenerationMethods?: string[];
}

      const models: Model[] = (response.data.models || []).map((m: GeminiAPIResponse) => {
        const methods: string[] = m.supportedGenerationMethods || [];
        const features: string[] = [];
        if (
          methods.some(
            (mt: string) =>
              mt.toLowerCase().includes("generatecontent") ||
              mt.toLowerCase().includes("generatemessage"),
          )
        )
          features.push("chat");
        if (
          methods.some(
            (mt: string) =>
              mt.toLowerCase().includes("vision") ||
              mt.toLowerCase().includes("image"),
          )
        )
          features.push("vision");
        if (
          methods.some(
            (mt: string) =>
              mt.toLowerCase().includes("audio") ||
              mt.toLowerCase().includes("speech"),
          )
        )
          features.push("audio");
        if (methods.some((mt: string) => mt.toLowerCase().includes("video")))
          features.push("video");
        if (m.name.includes("embedding")) features.push("embeddings");
        if (features.length === 0) features.push("chat");

        return {
          id: m.name.replace("models/", ""),
          name: m.displayName || m.name,
          provider: "google",
          contextWindow: m.inputTokenLimit || 32768,
          supportedFeatures: features,
          freeTier: true,
          url: `https://ai.google.dev/gemini-api/docs/models#${m.name.replace("models/", "")}`,
          description: m.description,
        };
      });

      return { success: true, models };
    } catch {
      // fall through to scraper
    }
  }

  try {
    const models = await scrapeGooglePage();
    return { success: true, models };
  } catch (error) {
    return {
      success: false,
      models: [],
      error: error instanceof Error ? error.message : "Unknown error",
    };
  }
}
