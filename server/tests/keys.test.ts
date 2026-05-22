import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createApp } from '../src/app.js';
import { initDb } from '../src/db/index.js';

describe('Keys Endpoint', () => {
  let app: ReturnType<typeof createApp>;

  beforeEach(async () => {
    // Initialize test database
    initDb(':memory:');
    app = createApp();
  });

  afterEach(() => {
    // Cleanup if needed
  });

  it('should return empty keys array initially', async () => {
    const res = await app.request('/api/keys');
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(Array.isArray(data)).toBe(true);
    expect(data.length).toBe(0);
  });

  it('should add a key and return it', async () => {
    // Add a key
    const addRes = await app.request('/api/keys', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        platform: 'google',
        key: 'test-api-key-123',
        label: 'Test Key'
      })
    });
    
    expect(addRes.status).toBe(201);
    const addData = await addRes.json();
    expect(addData).toHaveProperty('id');
    expect(addData.platform).toBe('google');
    expect(addData.label).toBe('Test Key');
    expect(addData.maskedKey).not.toBe('test-api-key-123');
    expect(addData.enabled).toBe(true);

    // Get all keys
    const getRes = await app.request('/api/keys');
    expect(getRes.status).toBe(200);
    const getData = await getRes.json();
    expect(Array.isArray(getData)).toBe(true);
    expect(getData.length).toBe(1);
    expect(getData[0].id).toBe(addData.id);
  });

  it('should delete a key', async () => {
    // Add a key first
    const addRes = await app.request('/api/keys', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        platform: 'google',
        key: 'test-api-key-delete',
        label: 'Key to Delete'
      })
    });
    
    expect(addRes.status).toBe(201);
    const addData = await addRes.json();
    const keyId = addData.id;

    // Delete the key
    const delRes = await app.request(`/api/keys/${keyId}`, {
      method: 'DELETE'
    });
    
    expect(delRes.status).toBe(200);
    const delData = await delRes.json();
    expect(delData.success).toBe(true);

    // Verify key is deleted
    const getRes = await app.request('/api/keys');
    expect(getRes.status).toBe(200);
    const getData = await getRes.json();
    expect(Array.isArray(getData)).toBe(true);
    expect(getData.length).toBe(0);
  });

  it('should toggle key enabled status', async () => {
    // Add a key first
    const addRes = await app.request('/api/keys', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        platform: 'google',
        key: 'test-api-key-toggle',
        label: 'Key to Toggle'
      })
    });
    
    expect(addRes.status).toBe(201);
    const addData = await addRes.json();
    const keyId = addData.id;

    // Disable the key
    const patchRes = await app.request(`/api/keys/${keyId}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled: false })
    });
    
    expect(patchRes.status).toBe(200);
    const patchData = await patchRes.json();
    expect(patchData.success).toBe(true);
    expect(patchData.enabled).toBe(false);

    // Verify key is disabled
    const getRes = await app.request('/api/keys');
    expect(getRes.status).toBe(200);
    const getData = await getRes.json();
    expect(Array.isArray(getData)).toBe(true);
    expect(getData.length).toBe(1);
    expect(getData[0].enabled).toBe(false);
  });
});