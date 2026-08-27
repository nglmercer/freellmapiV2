interface HttpConfig {
  headers?: Record<string, string>;
  timeout?: number;
  params?: Record<string, string>;
}

interface HttpResponse<T = any> {
  data: T;
  status: number;
  statusText: string;
  headers: Headers;
}

interface RetryConfig {
  retries: number;
  retryDelay: (retryCount: number, error: Error) => number;
  retryCondition: (error: HttpError) => boolean;
}

export class HttpError extends Error {
  response?: { status: number };
  constructor(message: string, status?: number) {
    super(message);
    if (status) this.response = { status };
  }
}

export const retry = {
  exponentialDelay(retryCount: number): number {
    return Math.pow(2, retryCount) * 1000;
  },
};

let _retryConfig: RetryConfig = {
  retries: 0,
  retryDelay: retry.exponentialDelay,
  retryCondition: () => true,
};

export function configureRetry(config: Partial<RetryConfig>): void {
  _retryConfig = { ..._retryConfig, ...config };
}

async function get<T = any>(url: string, config: HttpConfig = {}): Promise<HttpResponse<T>> {
  let finalUrl = url;
  if (config.params) {
    const sp = new URLSearchParams(config.params);
    finalUrl = `${url}${url.includes('?') ? '&' : '?'}${sp}`;
  }

  let lastError: Error | undefined;

  for (let attempt = 0; attempt <= _retryConfig.retries; attempt++) {
    const controller = new AbortController();
    const timeoutMs = config.timeout || 30000;
    const timeoutId = setTimeout(() => controller.abort(), timeoutMs);

    try {
      const response = await fetch(finalUrl, {
        headers: config.headers,
        signal: controller.signal,
      });
      clearTimeout(timeoutId);

      if (!response.ok) {
        const err = new HttpError(
          `HTTP ${response.status}: ${response.statusText}`,
          response.status,
        );
        if (
          attempt < _retryConfig.retries &&
          _retryConfig.retryCondition(err)
        ) {
          lastError = err;
          await sleep(_retryConfig.retryDelay(attempt, err));
          continue;
        }
        throw err;
      }

      const contentType = response.headers.get("content-type") || "";
      const data = contentType.includes("application/json")
        ? await response.json()
        : await response.text();

      return {
        data: data as T,
        status: response.status,
        statusText: response.statusText,
        headers: response.headers,
      };
    } catch (error: unknown) {
      clearTimeout(timeoutId);
      if (error instanceof HttpError) {
        lastError = error;
      } else if (error instanceof Error) {
        lastError = new HttpError(error.message);
      } else {
        lastError = new HttpError(String(error));
      }
      if (
        attempt < _retryConfig.retries &&
        _retryConfig.retryCondition(lastError as HttpError)
      ) {
        await sleep(_retryConfig.retryDelay(attempt, lastError as HttpError));
        continue;
      }
      throw lastError;
    }
  }

  throw lastError!;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

const http = { get };
export default http;
