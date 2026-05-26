import { describe, it, expect } from 'bun:test';
import { timingSafeStringEqual, normalizeMessages, estimateInputTokens } from '../src/routes/middleware.js';
import type { ChatMessage } from '@freellmapi/shared/types.js';

describe('timingSafeStringEqual', () => {
  it('should return true for identical strings', () => {
    expect(timingSafeStringEqual('abc', 'abc')).toBe(true);
  });

  it('should return false for different strings', () => {
    expect(timingSafeStringEqual('abc', 'xyz')).toBe(false);
  });

  it('should return false for different length strings', () => {
    expect(timingSafeStringEqual('abc', 'abcdef')).toBe(false);
  });

  it('should return false for empty vs non-empty', () => {
    expect(timingSafeStringEqual('', 'abc')).toBe(false);
  });

  it('should return true for two empty strings', () => {
    expect(timingSafeStringEqual('', '')).toBe(true);
  });
});

describe('normalizeMessages', () => {
   it('should normalize a basic user message', () => {
     const messages = [{ role: 'user', content: 'Hello' }];
     const result = normalizeMessages(messages);
     expect(result).toHaveLength(1);
     expect(result[0]!.role).toBe('user');
     expect(result[0]!.content).toBe('Hello');
   });

   it('should preserve system messages', () => {
     const messages = [{ role: 'system', content: 'You are a bot.' }];
     const result = normalizeMessages(messages);
     expect(result[0]!.role).toBe('system');
     expect(result[0]!.content).toBe('You are a bot.');
   });

   it('should handle null content in assistant messages', () => {
     const messages = [{ role: 'assistant', content: null }];
     const result = normalizeMessages(messages);
     expect(result[0]!.content).toBeNull();
   });

   it('should preserve assistant tool_calls', () => {
     const messages = [{
       role: 'assistant',
       content: 'Calling tool...',
       tool_calls: [{ id: 'call_1', type: 'function', function: { name: 'get_weather', arguments: '{}' } }],
     }];
     const result = normalizeMessages(messages);
     expect(result[0]!.tool_calls).toBeDefined();
     expect(result[0]!.tool_calls).toHaveLength(1);
     expect(result[0]!.tool_calls![0]!.id).toBe('call_1');
   });

   it('should preserve tool_call_id for tool messages', () => {
     const messages = [{
       role: 'tool',
       content: 'Sunny',
       tool_call_id: 'call_1',
     }];
     const result = normalizeMessages(messages);
     expect(result[0]!.tool_call_id).toBe('call_1');
   });

   it('should preserve name field', () => {
     const messages = [{ role: 'user', content: 'Hi', name: 'Alice' }];
     const result = normalizeMessages(messages);
     expect(result[0]!.name).toBe('Alice');
   });

   it('should handle mixed conversation', () => {
     const messages = [
       { role: 'system', content: 'You are helpful.' },
       { role: 'user', content: 'Hello' },
       { role: 'assistant', content: 'Hi there', tool_calls: [] },
       { role: 'user', content: 'How are you?' },
     ];
     const result = normalizeMessages(messages);
     expect(result).toHaveLength(4);
   });
 });

describe('estimateInputTokens', () => {
  it('should return 0 for empty messages', () => {
    const messages: ChatMessage[] = [];
    expect(estimateInputTokens(messages)).toBe(0);
  });

  it('should estimate tokens from message content length', () => {
    const messages: ChatMessage[] = [{ role: 'user', content: 'Hello' }];
    // 'Hello' = 5 chars, 5/4 = 1.25, ceil = 2
    expect(estimateInputTokens(messages)).toBe(2);
  });

  it('should sum tokens across multiple messages', () => {
    const messages: ChatMessage[] = [
      { role: 'user', content: 'Hello' },
      { role: 'assistant', content: 'World' },
    ];
    expect(estimateInputTokens(messages)).toBe(4);
  });

  it('should handle null content gracefully', () => {
    const messages: ChatMessage[] = [{ role: 'assistant', content: null }];
    expect(estimateInputTokens(messages)).toBe(0);
  });

  it('should handle long content', () => {
    const messages: ChatMessage[] = [{ role: 'user', content: 'A'.repeat(100) }];
    expect(estimateInputTokens(messages)).toBe(25);
  });
});

describe('stream_options validation', () => {
  it('should accept stream_options with include_usage set to true', () => {
    const { chatCompletionSchema } = require('../src/routes/middleware.js');
    const result = chatCompletionSchema.safeParse({
      messages: [{ role: 'user', content: 'Hello' }],
      stream: true,
      stream_options: { include_usage: true },
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.stream_options?.include_usage).toBe(true);
    }
  });

  it('should accept stream_options with include_usage set to false', () => {
    const { chatCompletionSchema } = require('../src/routes/middleware.js');
    const result = chatCompletionSchema.safeParse({
      messages: [{ role: 'user', content: 'Hello' }],
      stream: true,
      stream_options: { include_usage: false },
    });
    expect(result.success).toBe(true);
  });

  it('should accept stream_options without include_usage', () => {
    const { chatCompletionSchema } = require('../src/routes/middleware.js');
    const result = chatCompletionSchema.safeParse({
      messages: [{ role: 'user', content: 'Hello' }],
      stream: true,
      stream_options: {},
    });
    expect(result.success).toBe(true);
  });
});
