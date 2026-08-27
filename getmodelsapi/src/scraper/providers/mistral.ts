//import http from "../../utils/http";
import http from "../../utils/http";
import * as cheerio from "cheerio";
import type { Model, ProviderConfig, ScraperResult } from "./index";

const NAV_HEADINGS =
  /why mistral|explore|documentation|build|legal|community|getting started|overview|quickstart|api reference|sdks|index|featured models|frontier models|generalist|specialist|other models|legacy|deprecated|search/i;

function parseContextWindow(text: string): number {
  // Look for "32k", "128k", "256k", "131k", "1M", "2M", raw numbers
  const patterns = [
    /(\d+)\s*m(?:illion)?\s*(?:token|context)/i,
    /(\d+)\s*tokens?\s*(?:context|window)/i,
    /context\s*(?:window|length).*?(\d[\d,]*)\s*[km]/i,
    /(\d+)\s*k\s*(?:token|context)/i,
    /(\d[\d,]*)\s*token/i,
  ];
  for (const p of patterns) {
    const m = text.match(p);
    if (m?.[1]) {
      let n = parseInt(m[1].replace(/,/g, ""), 10);
      if (p.toString().includes("k")) n *= 1000;
      if (p.toString().includes("million") || /m/i.test(m[0])) n *= 1000000;
      return n;
    }
  }
  return 32768;
}

// Deduce features from model name/description
function deduceFeatures(
  id: string,
  description: string,
  name: string,
): string[] {
  const text = (name + " " + description + " " + id).toLowerCase();
  const features: string[] = [];

  if (text.includes("embed") || text.includes("embedding"))
    return ["embeddings"];
  if (text.includes("moderation")) return ["moderation"];
  if (text.includes("ocr")) return ["ocr"];
  if (
    text.includes("tts") ||
    text.includes("transcribe") ||
    text.includes("speech") ||
    text.includes("audio")
  )
    return ["audio", "tts"];
  if (
    text.includes("vision") ||
    text.includes("image") ||
    id.includes("pixtral")
  )
    features.push("vision");
  if (
    text.includes("code") ||
    id.includes("codestral") ||
    id.includes("devstral") ||
    id.includes("leanstral")
  )
    features.push("code");

  features.push("chat");
  return features;
}

function normalizeForMatch(text: string): string {
  return text
    .replace(/^mistral\//, "")
    .replace(/^mistralai\//, "")
    .replace(/^mistral:\s*/i, "")
    .replace(/^\d{4}\s+/, "")
    .replace(/\s+\d{4}$/, "")
    .replace(/\s+\d+b$/i, "")
    .replace(/-latest$/, "")
    .replace(/-(\d{4}-\d{2}-\d{2})$/, "")
    .replace(/--+/g, "-")
    .replace(/-$/, "")
    .toLowerCase();
}

async function getContextFromOpenRouter(): Promise<Map<string, number>> {
  try {
    const response = await http.get("https://openrouter.ai/api/v1/models", {
      timeout: 10000,
    });
    const map = new Map<string, number>();
    if (Array.isArray(response.data?.data)) {
      for (const m of response.data.data) {
        if (
          (m.id?.startsWith("mistral/") || m.id?.startsWith("mistralai/")) &&
          m.context_length > 0
        ) {
          const nameKey = normalizeForMatch(m.name || "");
          const idKey = normalizeForMatch(m.id);
          const existing = map.get(nameKey);
          if (!existing || m.context_length > existing) {
            map.set(nameKey, m.context_length);
          }
          const idExisting = map.get(idKey);
          if (!idExisting || m.context_length > idExisting) {
            map.set(idKey, m.context_length);
          }
        }
      }
    }
    return map;
  } catch {
    return new Map();
  }
}

async function scrapeMistralPage(): Promise<Model[]> {
  const [response, ctxMap] = await Promise.all([
    http.get("https://docs.mistral.ai/getting-started/models/", {
      headers: {
        "User-Agent": "Mozilla/5.0 GetModels",
        "Accept-Language": "en-US,en;q=0.9",
      },
      timeout: 15000,
    }),
    getContextFromOpenRouter(),
  ]);

  const $ = cheerio.load(response.data);
  const seen = new Set<string>();

  // Collect model API IDs from the deprecation table (col index 2)
  const tableModels = new Map<string, string>(); // display name -> apiId
  $("table")
    .last()
    .find("tr")
    .each((_, row) => {
      const cells = $(row).find("td");
      if (cells.length < 3) return;
      const displayName = cells.eq(0).text().trim();
      const apiId = cells.eq(2).text().trim();
      if (
        apiId &&
        displayName &&
        apiId.length > 3 &&
        !apiId.includes(" ") &&
        displayName.length > 3 &&
        displayName !== "Model"
      ) {
        // Clean display name: remove " ↗" suffix
        const cleanName = displayName.replace(/\s*↗$/, "").trim();
        if (!tableModels.has(cleanName)) {
          tableModels.set(cleanName, apiId);
        }
      }
    });

  // Now extract current models from headings
  const models: Model[] = [];
  $("h2, h3, h4").each((_, el) => {
    const text = $(el).text().trim();
    if (!text || text.length < 3) return;
    if (NAV_HEADINGS.test(text)) return;

    // Must be a model-like heading
    const isModelHeading =
      /mistral|pixtral|ministral|codestral|voxtral|magistral|devstral|leanstral|embed|ocr|moderation/i;
    if (!isModelHeading.test(text)) return;

    const name = text;
    const $section = $(el).nextUntil("h2, h3, h4");
    const description = $section.filter("p").first().text().trim() || undefined;
    const sectionText = description || "";

    // Generate a clean ID from the API table or the heading
    let id = text
      .toLowerCase()
      .replace(/\s+/g, "-")
      .replace(/[^a-z0-9-]/g, "");

    // Try to match against known API IDs in the table
    const tableId = tableModels.get(name);
    if (tableId) id = tableId;

    if (seen.has(id)) return;
    seen.add(id);

    const scrapedCtx = parseContextWindow(sectionText);
    const orCtx =
      ctxMap.get(normalizeForMatch(name)) ||
      ctxMap.get(id) ||
      ctxMap.get(normalizeForMatch(id));
    const contextWindow = orCtx || scrapedCtx || 32768;
    const features = deduceFeatures(id, sectionText, name);

    models.push({
      id,
      name,
      provider: "mistral",
      contextWindow,
      supportedFeatures: features,
      freeTier: true,
      url: `https://docs.mistral.ai/getting-started/models/#${id}`,
      description,
    });
  });

  // Fallback: if heading extraction found little, use table data directly
  if (models.length === 0) {
    for (const [displayName, apiId] of tableModels) {
      if (seen.has(apiId)) continue;
      seen.add(apiId);

      // Skip labs/experimental
      if (apiId.startsWith("labs-")) continue;

      models.push({
        id: apiId,
        name: displayName,
        provider: "mistral",
        contextWindow: 32768,
        supportedFeatures: deduceFeatures(apiId, "", displayName),
        freeTier: true,
        url: `https://docs.mistral.ai/getting-started/models/#${apiId}`,
      });
    }
  }

  return models;
}

export async function scrapeMistral(
  config: ProviderConfig,
): Promise<ScraperResult> {
  if (config.apiKey) {
    try {
      const response = await http.get(`${config.baseUrl}/models`, {
        headers: { Authorization: `Bearer ${config.apiKey}` },
        timeout: 10000,
      });

      const models: Model[] = (response.data.data || []).map((m: { id: string; max_context_length?: number; owned_by?: string }) => {
        const name = m.id;
        return {
          id: name,
          name,
          provider: "mistral",
          contextWindow: m.max_context_length || 32768,
          supportedFeatures: ["chat"],
          freeTier: true,
          url: `https://docs.mistral.ai/getting-started/models/#${name}`,
          description: m.owned_by ? `Owned by ${m.owned_by}` : undefined,
        };
      });

      return { success: true, models };
    } catch (error) {
      console.error("Mistral API failed, falling back to scraper");
    }
  }

  try {
    const models = await scrapeMistralPage();
    return { success: true, models };
  } catch (error) {
    return {
      success: false,
      models: [],
      error: error instanceof Error ? error.message : "Unknown error",
    };
  }
}
