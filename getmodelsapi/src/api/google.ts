import http from "../utils/http";
import type { Model, ProviderConfig } from "../types";

interface GeminiModel {
  name: string;
  displayName: string;
  description: string;
  inputTokenLimit: number;
  outputTokenLimit: number;
  supportedGenerationMethods: string[];
}

export async function fetchModelsFromGoogle(
  provider: ProviderConfig,
): Promise<Model[]> {
  if (!provider.apiKey) return [];

  try {
    const response = await http.get(`${provider.baseUrl}/models`, {
      params: { key: provider.apiKey },
      timeout: 10000,
    });

    const models: Model[] = (response.data.models || []).map(
      (m: GeminiModel) => ({
        id: m.name.replace("models/", ""),
        name: m.displayName || m.name,
        provider: "google",
        gateway: "google",
        contextWindow: m.inputTokenLimit || 32768,
        supportedFeatures: m.supportedGenerationMethods?.includes(
          "generateContent",
        )
          ? ["chat"]
          : ["completion"],
        freeTier: true,
        url: `https://ai.google.dev/gemini-api/docs/models#${m.name.replace("models/", "")}`,
        description: m.description,
      }),
    );

    return models;
  } catch (error) {
    console.error("Failed to fetch models from Google:", error);
    return [];
  }
}
