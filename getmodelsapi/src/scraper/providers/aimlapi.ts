import http from "../../utils/http";
import type { Model, ProviderConfig, ScraperResult } from "./index";

interface AIMLAPIModel {
  id: string;
  type: string;
  info: {
    name: string;
    developer: string;
    description: string;
    contextLength: number;
    maxTokens: number;
    url: string;
    docs_url: string;
  };
  features: string[];
}

function parseFeatures(features: string[]): string[] {
  const feat = features.map((f) => f.toLowerCase());
  const result: string[] = [];
  if (feat.some((f) => f.includes("chat") || f.includes("completion")))
    result.push("chat");
  if (feat.some((f) => f.includes("vision") || f.includes("image")))
    result.push("vision");
  if (feat.some((f) => f.includes("function") || f.includes("tool")))
    result.push("tools");
  if (feat.some((f) => f.includes("audio") || f.includes("speech")))
    result.push("audio");
  if (result.length === 0) result.push("chat");
  return result;
}

export async function scrapeAIMLAPI(
  _config: ProviderConfig,
): Promise<ScraperResult> {
  try {
    const response = await http.get("https://api.aimlapi.com/v1/models", {
      timeout: 15000,
    });

    if (!Array.isArray(response.data?.data)) {
      return {
        success: false,
        models: [],
        error: "Unexpected AIMLAPI response",
      };
    }

    const models: Model[] = response.data.data.map((m: AIMLAPIModel) => ({
      id: m.id,
      name: m.info.name || m.id,
      provider: "aimlapi",
      gateway: m.info.developer?.toLowerCase() || undefined,
      contextWindow: m.info.contextLength || 4096,
      supportedFeatures: parseFeatures(m.features || []),
      url: m.info.url || m.info.docs_url,
      description: m.info.description?.substring(0, 300) || undefined,
    }));

    return { success: true, models };
  } catch (error) {
    return {
      success: false,
      models: [],
      error: error instanceof Error ? error.message : "Unknown error",
    };
  }
}
