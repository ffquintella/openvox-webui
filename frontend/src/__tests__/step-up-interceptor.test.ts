// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AxiosError, type AxiosAdapter, type InternalAxiosRequestConfig } from 'axios';
import { api, apiClient, registerStepUpHandler } from '../services/api';

const originalAdapter = apiClient.defaults.adapter;

function respond(config: InternalAxiosRequestConfig, status: number, data: unknown) {
  const response = { data, status, statusText: String(status), headers: {}, config };
  if (status >= 400) {
    throw new AxiosError('failed', String(status), config, null, response);
  }
  return response;
}

const stepUpRequired = { error: 'unauthorized', code: 'step_up_required' };

describe('step-up retry in the API client', () => {
  let seen: { url?: string; auth?: string }[];

  beforeEach(() => {
    seen = [];
    localStorage.setItem('auth_token', 'old-token');
  });

  afterEach(() => {
    apiClient.defaults.adapter = originalAdapter;
    registerStepUpHandler(null);
    localStorage.clear();
  });

  function adapter(statuses: number[]): AxiosAdapter {
    return async (config) => {
      seen.push({ url: config.url, auth: String(config.headers.Authorization) });
      const status = statuses[Math.min(seen.length - 1, statuses.length - 1)];
      return respond(config, status, status === 401 ? stepUpRequired : { factors: [] });
    };
  }

  it('repeats the guarded call once with the new session after a step-up', async () => {
    apiClient.defaults.adapter = adapter([401, 200]);
    const handler = vi.fn(async () => {
      localStorage.setItem('auth_token', 'new-token');
      return true;
    });
    registerStepUpHandler(handler);

    await expect(api.getMyFactors()).resolves.toEqual({ factors: [] });
    expect(handler).toHaveBeenCalledTimes(1);
    expect(seen).toEqual([
      { url: '/users/me/factors', auth: 'Bearer old-token' },
      { url: '/users/me/factors', auth: 'Bearer new-token' },
    ]);
  });

  it('rejects with the original error when the user cancels', async () => {
    apiClient.defaults.adapter = adapter([401]);
    registerStepUpHandler(async () => false);
    await expect(api.getMyFactors()).rejects.toMatchObject({
      response: { data: { code: 'step_up_required' } },
    });
    expect(seen).toHaveLength(1);
  });

  it('asks only once when the repeated call still needs a step-up', async () => {
    apiClient.defaults.adapter = adapter([401, 401]);
    const handler = vi.fn(async () => true);
    registerStepUpHandler(handler);
    await expect(api.getMyFactors()).rejects.toMatchObject({ response: { status: 401 } });
    expect(handler).toHaveBeenCalledTimes(1);
    expect(seen).toHaveLength(2);
  });

  it('keeps the old behaviour when no dialog is mounted', async () => {
    apiClient.defaults.adapter = adapter([401]);
    await expect(api.getMyFactors()).rejects.toMatchObject({ response: { status: 401 } });
    expect(seen).toHaveLength(1);
  });

  it('encodes qualified plugin factor ids in enrolment paths', async () => {
    apiClient.defaults.adapter = async (config) => {
      seen.push({ url: config.url });
      return respond(config, 200, {
        factor_id: 'mfa-fido2:security-key',
        enrollment: { publicKey: {} },
      });
    };
    await expect(api.beginFactorEnroll('mfa-fido2:security-key')).resolves.toEqual({
      publicKey: {},
    });
    expect(seen[0].url).toBe('/users/me/factors/mfa-fido2%3Asecurity-key/enroll/begin');
  });
});
