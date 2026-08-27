import "dotenv/config";
import { getModels } from "../src/api/index";

// Single provider: get models from a specific provider
const provider = process.argv[2] || "huggingface";
const models = await getModels({ provider });

console.log(`${provider}: ${models.length} models`);
for (const m of models.slice(0, 10)) {
  console.log(`  ${m.id}`);
  if (m.description) console.log(`    ${m.description.substring(0, 120)}`);
}
