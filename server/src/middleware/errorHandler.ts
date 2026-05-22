import type { Request, Response, NextFunction } from 'express';

export function errorHandler(err: Error, _req: Request, res: Response, next: NextFunction) {
  console.error('[Error]', err.message);

  if (res.headersSent) return next(err);

  const status = 'status' in err ? err.status : 500;
  res.status(Number(status))
  return res.json({
    error: {
      message: err.message,
      type: err.name ?? 'server_error',
    },
  });
}
