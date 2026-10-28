// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import DeclarativeRenderer from '../components/plugins/DeclarativeRenderer';

afterEach(cleanup);

describe('DeclarativeRenderer', () => {
  it('renders table columns and rows as text', () => {
    render(
      <DeclarativeRenderer
        spec={{
          layout: 'table',
          columns: [
            { key: 'host', label: 'Host' },
            { key: 'status', label: 'Status' },
          ],
          rows: [{ host: 'a.example.com', status: 'ok' }],
        }}
      />
    );
    expect(screen.getByText('Host')).toBeInTheDocument();
    expect(screen.getByText('a.example.com')).toBeInTheDocument();
    expect(screen.getByText('ok')).toBeInTheDocument();
  });

  it('shows a note when a table declares a data_source', () => {
    render(
      <DeclarativeRenderer
        spec={{ layout: 'table', columns: [], rows: [], data_source: { fn: 'list' } }}
      />
    );
    expect(screen.getByText(/Data sources arrive with the WASM runtime/)).toBeInTheDocument();
  });

  it('renders stats items with label, value and hint', () => {
    render(
      <DeclarativeRenderer
        spec={{ layout: 'stats', items: [{ label: 'Nodes', value: 42, hint: 'last hour' }] }}
      />
    );
    expect(screen.getByText('Nodes')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
    expect(screen.getByText('last hour')).toBeInTheDocument();
  });

  it('renders detail fields and nested stack blocks', () => {
    render(
      <DeclarativeRenderer
        spec={{
          layout: 'stack',
          blocks: [
            { layout: 'detail', title: 'Info', fields: [{ label: 'Owner', value: 'ops' }] },
            { layout: 'markdown', content: '# Title\n\n- one\n- two' },
          ],
        }}
      />
    );
    expect(screen.getByText('Info')).toBeInTheDocument();
    expect(screen.getByText('ops')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Title' })).toBeInTheDocument();
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
  });

  it('escapes HTML and script in table, stats and detail values', () => {
    const { container } = render(
      <DeclarativeRenderer
        spec={{
          layout: 'stack',
          blocks: [
            {
              layout: 'table',
              columns: [{ key: 'x', label: '<img src=x onerror=alert(1)>' }],
              rows: [{ x: '<script>alert(1)</script>' }],
            },
            { layout: 'stats', items: [{ label: '<b>l</b>', value: '<i>v</i>' }] },
            {
              layout: 'detail',
              title: '<u>t</u>',
              fields: [{ label: 'a', value: '<svg onload=x>' }],
            },
          ],
        }}
      />
    );
    expect(container.querySelector('script')).toBeNull();
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('svg[onload]')).toBeNull();
    expect(container.querySelector('b')).toBeNull();
    expect(container.querySelector('i')).toBeNull();
    expect(screen.getByText('<script>alert(1)</script>')).toBeInTheDocument();
  });

  it('escapes HTML in markdown and only links http/https', () => {
    const { container } = render(
      <DeclarativeRenderer
        spec={{
          layout: 'markdown',
          content:
            '<script>alert(1)</script> and <img src=x onerror=alert(1)>\n\n[ok](https://example.com) [bad](javascript:alert(1)) [data](data:text/html,x)',
        }}
      />
    );
    expect(container.querySelector('script')).toBeNull();
    expect(container.querySelector('img')).toBeNull();
    const links = container.querySelectorAll('a');
    expect(links).toHaveLength(1);
    expect(links[0]).toHaveAttribute('href', 'https://example.com');
    expect(links[0]).toHaveAttribute('rel', expect.stringContaining('noopener'));
    expect(container.innerHTML).not.toContain('javascript:');
    expect(screen.getByText(/<script>alert\(1\)<\/script>/)).toBeInTheDocument();
  });

  it('shows a neutral notice for unknown layouts and invalid specs', () => {
    render(<DeclarativeRenderer spec={{ layout: 'carousel' }} />);
    expect(screen.getByRole('note')).toHaveTextContent(/not supported/);
    cleanup();
    render(<DeclarativeRenderer spec={null} />);
    expect(screen.getByRole('note')).toHaveTextContent(/not supported/);
  });
});
