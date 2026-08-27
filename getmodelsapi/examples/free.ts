import "dotenv/config";
import { getModels } from "../src/api/index";

// Free-tier only: show only models marked as freeTier = true
const freeModels = await getModels({ free: true });

console.log(`Free models: ${freeModels.length}`);
for (const m of freeModels) {
  console.log(`  ${m.provider.padEnd(15)} ${m.name} (${m.contextWindow.toLocaleString()} tokens)`);
}
