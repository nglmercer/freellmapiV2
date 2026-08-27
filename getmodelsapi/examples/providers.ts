import "dotenv/config";
import { getProviders } from "../src/api/index";

// List all registered providers with their metadata
const providers = await getProviders();

console.log("Registered providers:\n");
for (const p of providers) {
  const free = p.freeTier ? "FREE" : "paid";
  const scrape = p.supportsScraping ? "scrape" : "auth";
  console.log(`  ${p.name.padEnd(15)} ${p.type.padEnd(10)} ${free.padEnd(5)} ${scrape}`);
}
