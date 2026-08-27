export interface Model {
  id: string;
  name: string;
  provider: string;
  gateway?: string;
  contextWindow: number;
  supportedFeatures: string[];
  pricing?: {
    prompt: number;
    completion: number;
  };
  url?: string;
  description?: string;
  freeTier?: boolean;
}

export interface SearchParams {
  provider?: string;
  gateway?: string;
  search?: string;
  limit?: number;
  offset?: number;
}

export interface ApiResponse<T> {
  success: boolean;
  data: T;
  meta?: {
    total: number;
    limit: number;
    offset: number;
  };
  error?: string;
}

export interface ProviderConfig {
  name: string;
  type: 'provider' | 'gateway';
  baseUrl: string;
  apiKey?: string;
  supportsScraping: boolean;
  priority: number;
  freeTier: boolean;
}
