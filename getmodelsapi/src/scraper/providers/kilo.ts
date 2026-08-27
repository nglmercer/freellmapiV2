import http from "../../utils/http";
import type { Model, ProviderConfig, ScraperResult } from "./index";

interface KiloModel {
  id: string;
  name: string;
  description: string;
  openrouterId: string;
  slug: string;
  contextLength: number;
  maxOutputTokens: number;
  inputModalities: string[];
  priceInput: string;
  priceOutput: string;
  modelCreator: string;
  speedTokensPerSec: number;
  codingIndex: number;
}

function parseFeatures(m: KiloModel): string[] {
  const inputs = (m.inputModalities || []).map((s) => s.toLowerCase());
  const features: string[] = [];

  if (inputs.includes("text")) features.push("chat");
  if (inputs.includes("image")) features.push("vision");
  if (inputs.includes("file")) features.push("file");
  if (inputs.includes("audio")) features.push("audio");
  if (m.id.includes("embed") || m.name.toLowerCase().includes("embed"))
    features.push("embeddings");
  if (features.length === 0) features.push("chat");
  return features;
}

export async function scrapeKilo(
  _config: ProviderConfig,
): Promise<ScraperResult> {
  try {
    const response = await http.get("https://kilo.ai/api/models", {
      timeout: 15000,
    });

    if (!Array.isArray(response.data)) {
      return {
        success: false,
        models: [],
        error: "Unexpected Kilo API response",
      };
    }

    const models: Model[] = response.data.map((m: KiloModel) => {
      const providerName = m.modelCreator?.toLowerCase() || "kilo";
      const promptPrice = parseFloat(m.priceInput || "0");
      const completionPrice = parseFloat(m.priceOutput || "0");

      return {
        id: m.openrouterId || m.id,
        name: m.name,
        provider: "kilo",
        gateway: providerName === "kilo" ? undefined : providerName,
        contextWindow: m.contextLength || 4096,
        supportedFeatures: parseFeatures(m),
        pricing:
          promptPrice > 0 || completionPrice > 0
            ? { prompt: promptPrice, completion: completionPrice }
            : undefined,
        freeTier:
          (promptPrice === 0 && completionPrice === 0) ||
          (!promptPrice && !completionPrice),
        url: `https://kilo.ai/models/${m.slug || m.id}`,
        description: m.description?.substring(0, 300) || undefined,
      };
    });

    return { success: true, models };
  } catch (error) {
    return {
      success: false,
      models: [],
      error: error instanceof Error ? error.message : "Unknown error",
    };
  }
}
