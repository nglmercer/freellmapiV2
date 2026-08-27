import http from '../utils/http';
import type { Model, ProviderConfig } from '../types';

interface TogetherModel {
  id: string;
  display_name: string;
  context_length: number;
  pricing?: { hourly?: number; input?: number; output?: number };
  type?: string;
}

export async function fetchModelsFromTogether(provider: ProviderConfig): Promise<Model[]> {
  if (!provider.apiKey) return [];

  try {
    const response = await http.get(`${provider.baseUrl}/models`, {
      headers: { 'Authorization': `Bearer ${provider.apiKey}` },
      timeout: 10000,
    });

    const models: Model[] = (response.data || []).map((m: TogetherModel) => ({
      id: m.id,
      name: m.display_name || m.id,
      provider: 'together',
      gateway: 'together',
      contextWindow: m.context_length || 4096,
      supportedFeatures: ['chat', 'completion'],
      pricing: m.pricing ? {
        prompt: m.pricing.input || 0,
        completion: m.pricing.output || 0,
      } : undefined,
      freeTier: !m.pricing || (m.pricing.input === 0 && m.pricing.output === 0),
      url: `https://api.together.xyz/models/${m.id}`,
    }));

    return models;
  } catch (error) {
    console.error('Failed to fetch models from Together:', error);
    return [];
  }
}
