import "dotenv/config";
import { getModels, enrichModels } from "../src/api/index";

// Step 1: Get models from listing pages
const models = await getModels({ provider: "google", limit: 5 });

console.log("Before enrichment:");
for (const m of models) {
  console.log(`  ${m.id.padEnd(35)} ctx=${String(m.contextWindow).padStart(10)} features=[${m.supportedFeatures.join(',')}]`);
}

// Step 2: Enrich with data from individual model pages
const enriched = await enrichModels(models);

console.log("\nAfter enrichment (scraped individual pages):");
for (const m of enriched) {
  console.log(`  ${m.id.padEnd(35)} ctx=${String(m.contextWindow).padStart(10)} features=[${m.supportedFeatures.join(',')}]`);
}
