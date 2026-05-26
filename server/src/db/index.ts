import { initDb, getDb, runInTransaction, DB_PATH, resetDb } from './connection.js';
import type { Transaction } from './connection.js';
import { seedModels } from './seed.js';
import { ensureFallbackEntries } from './seed.js';
import { migrateModels, migrateModelsV2 } from './migrations-v1.js';
import { migrateModelsV3Ranks, migrateModelsV4 } from './migrations-v4.js';
import { migrateModelsV5, migrateModelsV6, migrateModelsV7, migrateModelsV8, migrateModelsV9, migrateModelsV10, migrateModelsV11 } from './migrations-v5.js';
import { ensureUnifiedKey, getUnifiedApiKey, regenerateUnifiedKey } from './unified-key.js';

export { DB_PATH };
export type { Transaction };
export { initDb, resetDb };

function runMigrations(tx: Transaction): void {
  seedModels(tx);
  migrateModels(tx);
  migrateModelsV2(tx);
  migrateModelsV3Ranks(tx);
  migrateModelsV4(tx);
  migrateModelsV5(tx);
  migrateModelsV6(tx);
  migrateModelsV7(tx);
  migrateModelsV8(tx);
  migrateModelsV9(tx);
  migrateModelsV10(tx);
  migrateModelsV11(tx);
  ensureUnifiedKey(tx);
}

initDb();
runInTransaction(runMigrations);

export { getDb, runInTransaction, runMigrations, getUnifiedApiKey, regenerateUnifiedKey };
