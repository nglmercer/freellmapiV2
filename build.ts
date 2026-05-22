import { $ } from 'bun';

await $`cd server && bun run build`;
await $`cd client && bun run build`;
console.log('✅ Build completed successfully');