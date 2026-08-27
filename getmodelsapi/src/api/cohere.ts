import http from "../utils/http";
import type { Model, ProviderConfig } from "../types";

interface CohereModel {
  name: string;
  endpoints?: string[];
  context_length?: number;
  tokenizer_url?: string;
}

export async function fetchModelsFromCohere(
  provider: ProviderConfig,
): Promise<Model[]> {
  if (!provider.apiKey) return [];

  try {
    const response = await http.get(`${provider.baseUrl}/models`, {
      headers: { Authorization: `Bearer ${provider.apiKey}` },
      timeout: 10000,
    });

    const models: Model[] = (response.data.models || []).map(
      (m: CohereModel) => ({
        id: m.name,
        name: m.name,
        provider: "cohere",
        gateway: "cohere",
        contextWindow: m.context_length || 4096,
        supportedFeatures: m.endpoints?.includes("chat")
          ? ["chat"]
          : ["completion"],
        url: `https://docs.cohere.com/docs/models#${m.name}`,
        freeTier: true,
      }),
    );

    return models;
  } catch (error) {
    console.error("Failed to fetch models from Cohere:", error);
    return [];
  }
}
