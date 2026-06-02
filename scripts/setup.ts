import { $ } from "bun";
import { copyFile, access } from "node:fs/promises";

console.log("[setup] 1/3 Initializing submodules...");
const subInit = await $`bun run init:submodules`.nothrow();
if (subInit.exitCode !== 0) {
  process.exit(subInit.exitCode ?? 1);
}

console.log("[setup] 2/3 Installing dependencies...");
const install = await $`bun install`.nothrow();
if (install.exitCode !== 0) {
  process.exit(install.exitCode ?? 1);
}

console.log("[setup] 3/3 Seeding .env from .env.example...");
try {
  await access(".env");
  console.log("[setup] .env already exists, skipping.");
} catch {
  try {
    await copyFile(".env.example", ".env");
    console.log("[setup] Created .env from .env.example. Fill in your secrets before running the server.");
  } catch (err) {
    console.warn(`[setup] Could not create .env (${(err as Error).message}). Copy .env.example manually.`);
  }
}

console.log("[setup] Done. Run `bun run dev` to start.");
