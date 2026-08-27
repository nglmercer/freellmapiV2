import http from '../../utils/http';
import type { Model, ProviderConfig, ScraperResult } from './index';

interface SambaNovaModel {
  id: string;
  object: string;
  context_length: number;
  max_completion_tokens: number;
  pricing: { prompt: string; completion: string };
}

export async function scrapeSambaNova(_config: ProviderConfig): Promise<ScraperResult> {
  try {
    const response = await http.get('https://api.sambanova.ai/v1/models', {
      timeout: 15000,
    });

    if (!Array.isArray(response.data?.data)) {
      return { success: false, models: [], error: 'Unexpected SambaNova response' };
    }

    const models: Model[] = response.data.data.map((m: SambaNovaModel) => ({
      id: m.id,
      name: m.id,
      provider: 'sambanova',
      contextWindow: m.context_length || 4096,
      supportedFeatures: ['chat'],
      pricing: m.pricing ? {
        prompt: parseFloat(m.pricing.prompt || '0'),
        completion: parseFloat(m.pricing.completion || '0'),
      } : undefined,
      url: `https://cloud.sambanova.ai/models/${m.id}`,
    }));

    return { success: true, models };
  } catch (error) {
    return {
      success: false,
      models: [],
      error: error instanceof Error ? error.message : 'Unknown error',
    };
  }
}
