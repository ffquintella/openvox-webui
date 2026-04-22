import { describe, expect, it } from 'vitest';
import { getDeleteNodeErrorMessage } from '../pages/nodeDetailErrors';

describe('getDeleteNodeErrorMessage', () => {
  it('returns authorization message for 403 errors', () => {
    const message = getDeleteNodeErrorMessage({
      response: {
        status: 403,
        data: {
          message: 'Permission denied',
        },
      },
      message: 'Request failed with status code 403',
    });

    expect(message).toBe('You do not have authorization to delete nodes.');
  });

  it('returns API message for non-403 errors', () => {
    const message = getDeleteNodeErrorMessage({
      response: {
        status: 500,
        data: {
          message: 'Internal server error',
        },
      },
      message: 'Request failed with status code 500',
    });

    expect(message).toBe('Internal server error');
  });
});
