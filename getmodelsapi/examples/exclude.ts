import "dotenv/config";
import { getModels } from "../src/api/index";

// Exclude specific providers
const models = await getModels({
  exclude: ["huggingface"],
  free: true,
});

console.log(`Models (excluding huggingface & openrouter): ${models.length}`);
for (const m of models) {
  console.log(
    `  [${m.provider}] ${m.name} ctx=${m.contextWindow.toLocaleString()} `,
  );
}

// Exclude also works with single provider
const googleOnly = await getModels({ provider: "google" });
const googleExcluded = await getModels({ exclude: "google", limit: 3 });
console.log(`\nGoogle models: ${googleOnly.length}`);
console.log(
  `Without google: ${googleExcluded.length} (should not include google)`,
);
const hasGoogle = googleExcluded.some((m) => m.provider === "google");
console.log(`Has google? ${hasGoogle} (should be false)`);
