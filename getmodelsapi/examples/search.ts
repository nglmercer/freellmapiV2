import "dotenv/config";
import { getModels } from "../src/api/index";

// Search across all public providers
const query = process.argv[2] || "llama";
const results = await getModels({ search: query, limit: 20 });

console.log(`Results for "${query}": ${results.length} models`);
for (const m of results) {
  console.log(`  [${m.provider}] ${m.name}`);
  if (m.pricing) {
    console.log(`    $${m.pricing.prompt}/1M in  $${m.pricing.completion}/1M out`);
  }
  if (m.freeTier) console.log(`    ⬆ free tier`);
}
