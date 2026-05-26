import dotenv from 'dotenv';
import path from 'path';
import { fileURLToPath } from 'url';
import { existsSync, readFileSync, writeFileSync } from 'fs';
import { randomBytes } from 'crypto';
import { logger } from './lib/logger.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Ensure .env file exists with a valid ENCRYPTION_KEY
function ensureEncryptionKey(): void {
  const envPath = path.join(__dirname, '../../.env');
  const examplePath = path.join(__dirname, '../../.env.example');

  // Generate a random 64-character hex key
  const randomKey = randomBytes(32).toString('hex');

  if (!existsSync(envPath)) {
    // If .env doesn't exist, copy from .env.example and set the key
    if (existsSync(examplePath)) {
      const exampleContent = readFileSync(examplePath, 'utf8');
      // Replace the example key line with our random key
      const updatedContent = exampleContent.replace(
        /^ENCRYPTION_KEY=.*$/m,
        `ENCRYPTION_KEY=${randomKey}`
      );
      writeFileSync(envPath, updatedContent, 'utf8');
      logger.info('[ENV] Created .env with generated encryption key');
    } else {
      // Fallback: create a minimal .env file
      writeFileSync(
        envPath,
        `ENCRYPTION_KEY=${randomKey}\nPORT=3001\n`,
        'utf8'
      );
      logger.info('[ENV] Created .env with generated encryption key (no .env.example found)');
    }
  } else {
    // .env exists, check if ENCRYPTION_KEY is present and valid
    let envContent = readFileSync(envPath, 'utf8');
    const keyMatch = envContent.match(/^ENCRYPTION_KEY=(.*)$/m);
    let hasValidKey = false;

    if (keyMatch) {
      const keyValue = keyMatch[1].trim();
      if (/^[0-9a-fA-F]{64}$/.test(keyValue)) {
        hasValidKey = true;
      }
    }

    if (!hasValidKey) {
      // Replace or add the ENCRYPTION_KEY line
      const updatedContent = envContent.replace(
        /^ENCRYPTION_KEY=.*$/m,
        `ENCRYPTION_KEY=${randomKey}`
      );
      // If the line didn't exist, we need to add it
      if (updatedContent === envContent) {
        // No existing ENCRYPTION_KEY line, append it
        writeFileSync(envPath, envContent + `\nENCRYPTION_KEY=${randomKey}\n`, 'utf8');
      } else {
        writeFileSync(envPath, updatedContent, 'utf8');
      }
      logger.info('[ENV] Updated .env with generated encryption key');
    }
  }
}

// Ensure the encryption key is set up before loading environment variables
ensureEncryptionKey();

// Now load environment variables
dotenv.config({ path: path.join(__dirname, '../../.env') });

export const env = {
  /**
   * Validates that the encryption key is a 64-character hexadecimal string.
   * Throws an error if the key is invalid or missing.
   */
  validateEncryptionKey(): void {
    const key = process.env.ENCRYPTION_KEY;
    if (!key) {
      throw new Error('ENCRYPTION_KEY environment variable is required');
    }
    
    if (!/^[0-9a-fA-F]{64}$/.test(key)) {
      throw new Error('ENCRYPTION_KEY must be a 64-character hexadecimal string');
    }
  },
  
  /**
   * Returns the port number from environment or default.
   */
  getPort(): number {
    return Number(process.env.PORT ?? 3001);
  }
};

// Validate encryption key on module load
env.validateEncryptionKey();
