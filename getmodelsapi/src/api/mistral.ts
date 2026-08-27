import http from '../utils/http';
import type { Model, ProviderConfig } from '../types';

interface MistralModel {
  id: string;
  object: string;
  created: number;
  owned_by: string;
  capabilities?: { completion_chat?: boolean; function_calling?: boolean };
  max_context_length?: number;
}

export async function fetchModelsFromMistral(provider: ProviderConfig): Promise<Model[]> {
  if (!provider.apiKey) return [];

  try {
    const response = await http.get(`${provider.baseUrl}/models`, {
      headers: { 'Authorization': `Bearer ${provider.apiKey}` },
      timeout: 10000,
    });

    const models: Model[] = (response.data.data || []).map((m: MistralModel) => ({
      id: m.id,
      name: m.id,
      provider: 'mistral',
      gateway: 'mistral',
      contextWindow: m.max_context_length || 32768,
      supportedFeatures: ['chat', 'completion'],
      freeTier: true,
      url: `https://docs.mistral.ai/getting-started/models/#${m.id}`,
      description: m.owned_by ? `Owned by ${m.owned_by}` : undefined,
    }));

    return models;
  } catch (error) {
    console.error('Failed to fetch models from Mistral:', error);
    return [];
  }
}
