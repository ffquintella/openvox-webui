// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { WarningBox, lifecycleWarnings } from '../components/plugins/pluginUi';

afterEach(cleanup);

describe('plugin lifecycle warnings (OQ-23)', () => {
  it('reads the warning messages of a disable or uninstall answer', () => {
    const out = {
      state: 'disabled',
      warnings: [
        {
          code: 'users_without_usable_factor',
          message: '1 user has no usable second factor left: viewer',
          count: 1,
          users: ['viewer'],
        },
        { code: 'other' },
      ],
    };
    expect(lifecycleWarnings(out)).toEqual(['1 user has no usable second factor left: viewer']);
    expect(lifecycleWarnings(undefined)).toEqual([]);
    expect(lifecycleWarnings('')).toEqual([]);
    expect(lifecycleWarnings({ warnings: 'x' })).toEqual([]);
  });

  it('shows the warnings as a status message', () => {
    render(<WarningBox messages={['2 users have no usable second factor left: a, b']} />);
    expect(screen.getByRole('status')).toHaveTextContent('2 users have no usable second factor');
  });
});
