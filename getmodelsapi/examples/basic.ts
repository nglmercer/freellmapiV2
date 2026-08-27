import "dotenv/config";
import { getModels, getProviders } from "../src/api/index";

// Basic: get all models from all providers (no API keys needed for public providers)
const models = await getModels();
console.log(`Total models: ${models.length}`);
console.log(models);
