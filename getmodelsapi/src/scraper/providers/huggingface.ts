import http, { configureRetry, retry } from "../../utils/http";
import * as Parser from "cheerio";
import type { Model, ProviderConfig, ScraperResult } from "./index";

configureRetry({
  retries: 3,
  retryDelay: retry.exponentialDelay,
  retryCondition: (error) => error.response?.status !== 429,
});

export async function scrapeHuggingFace(
  config: ProviderConfig,
): Promise<ScraperResult> {
  try {
    const response = await http.get(
      "https://huggingface.co/api/models?full=true&limit=100",
      {
        headers: {
          Authorization: `Bearer ${config.apiKey || ""}`,
        },
        timeout: 10000,
      },
    );

interface HuggingFaceAPIResponse {
  id: string;
  config?: {
    max_position_embeddings?: number;
    supports_prompt_completion_protocol?: boolean;
  };
  cardData?: {
    description?: string;
  };
}

    const models: Model[] = response.data.map((model: HuggingFaceAPIResponse) => ({
      id: model.id,
      name: model.id,
      provider: "huggingface",
      contextWindow: model.config?.max_position_embeddings || 4096,
      supportedFeatures: model.config?.supports_prompt_completion_protocol
        ? ["chat", "completion"]
        : ["completion"],
      url: `https://huggingface.co/${model.id}`,
      description: model.cardData?.description,
    }));

    return { success: true, models };
  } catch (error) {
    console.error("Failed to scrape HuggingFace:", error);
    return {
      success: false,
      models: [],
      error: error instanceof Error ? error.message : "Unknown error",
    };
  }
}
