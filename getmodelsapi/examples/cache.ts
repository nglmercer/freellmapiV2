import "dotenv/config";
import { getModels, clearCache } from "../src/api/index";

// Clear disk cache (to force fresh data)
const removed = clearCache();
console.log(`Cleared ${removed} cached files`);

// Fresh fetch
const t0 = performance.now();
const models = await getModels({ limit: 5 });
const t1 = performance.now();
console.log(`First fetch: ${models.length} models in ${(t1 - t0).toFixed(0)}ms`);

// Second fetch hits both memory + disk cache
const t2 = performance.now();
const cached = await getModels({ limit: 5 });
const t3 = performance.now();
console.log(`Cached fetch: ${cached.length} models in ${(t3 - t2).toFixed(0)}ms`);
