import http from '../../utils/http';
import type { Model, ProviderConfig, ScraperResult } from './index';

interface OpenRouterModel {
  id: string;
  name: string;
  description: string;
  context_length: number;
  architecture: { modality: string; input_modalities: string[]; output_modalities: string[]; tokenizer: string };
  pricing: { prompt: string; completion: string; input_cache_write?: string; input_cache_read?: string; web_search?: string };
}

function parseFeatures(m: OpenRouterModel): string[] {
  const features: string[] = [];
  const modality = m.architecture.modality.toLowerCase();
  const inputs = (m.architecture.input_modalities || []).map(s => s.toLowerCase());

  if (inputs.includes('text') || modality.includes('text')) features.push('chat');
  if (inputs.includes('image') || modality.includes('image')) features.push('vision');
  if (inputs.includes('audio')) features.push('audio');
  if (inputs.includes('video')) features.push('video');
  if (m.id.includes('embed') || m.id.includes('embedding')) features.push('embeddings');
  if (features.length === 0) features.push('chat');
  return features;
}

function parsePricing(pricing: OpenRouterModel['pricing']): { prompt: number; completion: number } | undefined {
  const p = parseFloat(pricing.prompt || '0');
  const c = parseFloat(pricing.completion || '0');
  if (p === 0 && c === 0) return undefined;
  return { prompt: p, completion: c };
}

export async function scrapeOpenRouter(_config: ProviderConfig): Promise<ScraperResult> {
  try {
    const response = await http.get('https://openrouter.ai/api/v1/models', {
      timeout: 15000,
    });

    if (!Array.isArray(response.data?.data)) {
      return { success: false, models: [], error: 'Unexpected OpenRouter API response' };
    }

    const models: Model[] = response.data.data.map((m: OpenRouterModel) => ({
      id: m.id,
      name: m.name,
      provider: 'openrouter',
      contextWindow: m.context_length || 4096,
      supportedFeatures: parseFeatures(m),
      pricing: parsePricing(m.pricing),
      freeTier: m.id.includes('free') || m.id.includes(':free'),
      url: `https://openrouter.ai/models/${m.id}`,
      description: m.description?.substring(0, 300) || undefined,
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
