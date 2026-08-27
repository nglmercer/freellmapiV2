import type { Model } from '../../types';
export type { Model, ProviderConfig, SearchParams, ApiResponse } from '../../types';

export interface ScraperResult {
  success: boolean;
  models: Model[];
  error?: string;
}

export interface ScraperOptions {
  timeout?: number;
  userAgent?: string;
}
