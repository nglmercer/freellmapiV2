import http from '../../utils/http';
import type { Model, ProviderConfig, ScraperResult } from './index';

interface NovitaModel {
  id: string;
  object: string;
  display_name: string;
  description: string;
  context_size: number;
  max_output_tokens: number;
  input_token_price_per_m: number;
  output_token_price_per_m: number;
  features: string[];
  model_type: string;
}

function parseFeatures(features: string[]): string[] {
  const feat = features.map(f => f.toLowerCase());
  const result: string[] = [];
  if (feat.some(f => f.includes('reasoning'))) result.push('reasoning');
  if (feat.some(f => f.includes('function'))) result.push('tools');
  if (feat.some(f => f.includes('structured'))) result.push('tools');
  result.push('chat');
  return result;
}

export async function scrapeNovita(_config: ProviderConfig): Promise<ScraperResult> {
  try {
    const response = await http.get('https://api.novita.ai/v3/openai/models', {
      timeout: 15000,
    });

    if (!Array.isArray(response.data?.data)) {
      return { success: false, models: [], error: 'Unexpected Novita response' };
    }

    const models: Model[] = response.data.data.map((m: NovitaModel) => {
      const freeTier = (m.input_token_price_per_m ?? 1) === 0 && (m.output_token_price_per_m ?? 1) === 0;

      return {
        id: m.id,
        name: m.display_name || m.id,
        provider: 'novita',
        contextWindow: m.context_size || 4096,
        supportedFeatures: parseFeatures(m.features || []),
        pricing: { prompt: (m.input_token_price_per_m ?? 0) / 1e6, completion: (m.output_token_price_per_m ?? 0) / 1e6 },
        freeTier: freeTier || undefined,
        url: `https://novita.ai/models/${m.id}`,
        description: m.description?.substring(0, 300) || undefined,
      };
    });

    return { success: true, models };
  } catch (error) {
    return {
      success: false,
      models: [],
      error: error instanceof Error ? error.message : 'Unknown error',
    };
  }
}
