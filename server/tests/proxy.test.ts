import { describe, it, expect, vi, beforeEach } from 'vitest'
import { proxyRouter } from '../../src/routes/proxy'
import { Hono } from 'hono'
import type { RouteResult } from '../services/router'
import type { ChatMessage } from '@freellmapi/shared/types'

// Mock dependencies
vi.mock('../db/index.js', () => ({
  getDb: vi.fn(),
  getUnifiedApiKey: vi.fn(() => 'test-unified-key')
}))

vi.mock('../services/router.js', () => ({
  routeRequest: vi.fn(),
  recordRateLimitHit: vi.fn(),
  recordSuccess: vi.fn()
}))

vi.mock('../services/ratelimit.js', () => ({
  recordRequest: vi.fn(),
  recordTokens: vi.fn(),
  setCooldown: vi.fn()
}))

// Mock crypto
vi.mock('crypto', () => ({
  timingsSafeEqual: vi.fn((a, b) => a === b),
  createHash: vi.fn().mockImplementation(() => ({
    update: vi.fn().mockReturnThis(),
    digest: vi.fn().mockReturnValue('hashed')
  }))
}))

describe('Proxy Router', () => {
  let db: any
  let c: any

  beforeEach(() => {
    db = {
      query: vi.fn()
    }
    ;(require('../db/index.js') as any).getDb.mockReturnValue(db)

    // Create a mock Hono context
    c = {
      req: {
        header: vi.fn(),
        json: vi.fn()
      },
      status: vi.fn().mockReturnThis(),
      json: vi.fn().mockReturnThis(),
      header: vi.fn().mockReturnThis(),
      body: {
        write: vi.fn(),
        close: vi.fn()
      }
    }
  })

  describe('GET /models', () => {
    it('should return list of models including auto model', async () => {
      db.query.mockReturnValue([
        { platform: 'openai', model_id: 'gpt-4', display_name: 'GPT-4', context_window: 8192 },
        { platform: 'anthropic', model_id: 'claude-3', display_name: 'Claude-3', context_window: 100000 }
      ])

      await proxyRouter.route('/models').get(c as any)

      expect(c.status).not.toHaveBeenCalled() // Default status is 200
      expect(c.json).toHaveBeenCalledWith({
        object: 'list',
        data: [
          {
            id: 'auto',
            object: 'model',
            created: 0,
            owned_by: 'freellmapi',
            name: 'Auto (router picks the best available model)',
            context_window: null
          },
          {
            id: 'gpt-4',
            object: 'model',
            created: 0,
            owned_by: 'openai',
            name: 'GPT-4',
            context_window: 8192
          },
          {
            id: 'claude-3',
            object: 'model',
            created: 0,
            owned_by: 'anthropic',
            name: 'Claude-3',
            context_window: 100000
          }
        ]
      })
    })
  })

  describe('POST /chat/completions', () => {
    const validRequest = {
      messages: [
        { role: 'user', content: 'Hello' }
      ],
      model: 'auto',
      temperature: 0.7,
      max_tokens: 100
    }

    it('should return 401 when API key is invalid', async () => {
      ;(require('../db/index.js') as any).getUnifiedApiKey.mockReturnValue('real-key')
      c.req.header.mockReturnValue('Bearer wrong-key')

      await proxyRouter.route('/chat/completions').post(c as any)

      expect(c.status).toHaveBeenCalledWith(401)
      expect(c.json).toHaveBeenCalledWith({
        error: { message: 'Invalid API key', type: 'authentication_error' }
      })
    })

    it('should return 400 when request validation fails', async () => {
      c.req.header.mockReturnValue('Bearer test-unified-key')
      c.req.json.mockResolvedValue({}) // Invalid: missing messages

      await proxyRouter.route('/chat/completions').post(c as any)

      expect(c.status).toHaveBeenCalledWith(400)
      expect(c.json).toHaveBeenCalledWith(
        expect.objectContaining({
          error: expect.objectContaining({ type: 'invalid_request_error' })
        })
      )
    })

    it('should return 400 when model is not found', async () => {
      c.req.header.mockReturnValue('Bearer test-unified-key')
      c.req.json.mockResolvedValue(validRequest)
      db.query.mockReturnValue(undefined) // Model not found

      await proxyRouter.route('/chat/completions').post(c as any)

      expect(c.status).toHaveBeenCalledWith(400)
      expect(c.json).toHaveBeenCalledWith(
        expect.objectContaining({
          error: expect.objectContaining({
            type: 'invalid_request_error',
            code: 'model_not_found'
          })
        })
      )
    })

    it('should handle successful non-streaming request', async () => {
      c.req.header.mockReturnValue('Bearer test-unified-key')
      c.req.json.mockResolvedValue({
        ...validRequest,
        stream: false
      })

      // Mock sticky session to return undefined (no sticky model)
      ;(proxyRouter as any).getStickyModel = vi.fn().mockReturnValue(undefined)

      // Mock routeRequest to return a successful route
      const mockRoute: RouteResult = {
        platform: 'openai',
        modelId: 'gpt-4',
        keyId: 'key1',
        modelDbId: 1,
        displayName: 'OpenAI GPT-4',
        provider: {
          chatCompletion: vi.fn().mockResolvedValue({
            id: 'chatcmpl-123',
            object: 'chat.completion',
            created: 1234567890,
            model: 'gpt-4',
            choices: [{
              index: 0,
              message: { role: 'assistant', content: 'Hello!' },
              finish_reason: 'stop'
            }],
            usage: { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 }
          })
        }
      }
      ;(require('../services/router.js') as any).routeRequest.mockReturnValue(mockRoute)

      await proxyRouter.route('/chat/completions').post(c as any)

      expect(c.status).not.toHaveBeenCalled() // Default 200
      expect(c.json).toHaveBeenCalledWith(
        expect.objectContaining({
          id: 'chatcmpl-123',
          object: 'chat.completion',
          model: 'gpt-4'
        })
      )
      expect(c.header).toHaveBeenCalledWith('X-Routed-Via', 'openai/gpt-4')
    })

    it('should handle successful streaming request', async () => {
      c.req.header.mockReturnValue('Bearer test-unified-key')
      c.req.json.mockResolvedValue({
        ...validRequest,
        stream: true
      })

      ;(proxyRouter as any).getStickyModel = vi.fn().mockReturnValue(undefined)

      const mockRoute: RouteResult = {
        platform: 'openai',
        modelId: 'gpt-4',
        keyId: 'key1',
        modelDbId: 1,
        displayName: 'OpenAI GPT-4',
        provider: {
          streamChatCompletion: vi.fn().mockReturnValue(async function* () {
            yield { choices: [{ delta: { content: 'Hello' } }] }
            yield { choices: [{ delta: { content: ' World' } }] }
          })
        }
      }
      ;(require('../services/router.js') as any).routeRequest.mockReturnValue(mockRoute)

      await proxyRouter.route('/chat/completions').post(c as any)

      expect(c.header).toHaveBeenCalledWith('Content-Type', 'text/event-stream')
      expect(c.body.write).toHaveBeenCalled()
      expect(c.body.close).toHaveBeenCalled()
    })
  })
})