export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

const LEVELS: LogLevel[] = ['debug', 'info', 'warn', 'error'];

let currentLevel: LogLevel = (process.env.LOG_LEVEL?.toLowerCase() as LogLevel) || 'warn';
let enabled = process.env.LOG_ENABLED !== 'false';

function shouldLog(level: LogLevel): boolean {
  if (!enabled) return false;
  return LEVELS.indexOf(level) >= LEVELS.indexOf(currentLevel);
}

function log(level: LogLevel, ...args: unknown[]): void {
  if (!shouldLog(level)) return;
  const timestamp = new Date().toISOString();
  const prefix = `[${timestamp}] [${level.toUpperCase()}]`;
  if (level === 'error') {
    console.error(prefix, ...args);
  } else if (level === 'warn') {
    console.warn(prefix, ...args);
  } else {
    console.log(prefix, ...args);
  }
}

export const logger = {
  debug: (...args: unknown[]) => log('debug', ...args),
  info: (...args: unknown[]) => log('info', ...args),
  warn: (...args: unknown[]) => log('warn', ...args),
  error: (...args: unknown[]) => log('error', ...args),
  setLevel: (level: LogLevel) => {
    if (LEVELS.includes(level)) currentLevel = level;
  },
  enable: () => {
    enabled = true;
  },
  disable: () => {
    enabled = false;
  },
};