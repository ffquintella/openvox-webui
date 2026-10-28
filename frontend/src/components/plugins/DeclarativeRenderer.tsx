import { Fragment, type ReactNode } from 'react';
import { Info } from 'lucide-react';
import { isSafeHttpUrl, toText } from './pluginHelpers';

/**
 * Renders a plugin-supplied declarative page spec with host components only.
 * Every value is rendered as React text (escaped by React); there is no
 * dangerouslySetInnerHTML and no plugin JavaScript.
 */

type Json = Record<string, unknown>;

function isObject(v: unknown): v is Json {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

const MAX_DEPTH = 4;

// ---------------------------------------------------------------------------
// Minimal markdown: headings, bullet lists, paragraphs, **bold**, `code`,
// [text](http/https url). Everything else is plain text.
// ---------------------------------------------------------------------------

const INLINE_RE = /(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\([^)\s]+\))/g;

function renderInline(text: string): ReactNode[] {
  return text.split(INLINE_RE).map((part, i) => {
    if (!part) return null;
    if (part.startsWith('**') && part.endsWith('**') && part.length > 4) {
      return <strong key={i}>{part.slice(2, -2)}</strong>;
    }
    if (part.startsWith('`') && part.endsWith('`') && part.length > 2) {
      return (
        <code key={i} className="px-1 rounded bg-gray-100 dark:bg-gray-700 text-xs">
          {part.slice(1, -1)}
        </code>
      );
    }
    const link = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(part);
    if (link) {
      if (isSafeHttpUrl(link[2])) {
        return (
          <a
            key={i}
            href={link[2]}
            target="_blank"
            rel="noopener noreferrer nofollow"
            className="text-primary-600 hover:underline"
          >
            {link[1]}
          </a>
        );
      }
      return <Fragment key={i}>{link[1]}</Fragment>;
    }
    return <Fragment key={i}>{part}</Fragment>;
  });
}

function MarkdownBlock({ content }: { content: string }) {
  const lines = content.split(/\r?\n/);
  const out: ReactNode[] = [];
  let paragraph: string[] = [];
  let list: string[] = [];

  const flushParagraph = () => {
    if (paragraph.length) {
      out.push(
        <p key={out.length} className="text-sm text-gray-700 dark:text-gray-300">
          {renderInline(paragraph.join(' '))}
        </p>
      );
      paragraph = [];
    }
  };
  const flushList = () => {
    if (list.length) {
      out.push(
        <ul key={out.length} className="list-disc pl-5 text-sm text-gray-700 dark:text-gray-300">
          {list.map((item, i) => (
            <li key={i}>{renderInline(item)}</li>
          ))}
        </ul>
      );
      list = [];
    }
  };

  for (const raw of lines) {
    const line = raw.trimEnd();
    const heading = /^(#{1,3})\s+(.*)$/.exec(line);
    const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
    if (heading) {
      flushParagraph();
      flushList();
      const level = heading[1].length;
      const cls =
        level === 1
          ? 'text-xl font-semibold text-gray-900 dark:text-white'
          : level === 2
            ? 'text-lg font-semibold text-gray-900 dark:text-white'
            : 'text-base font-semibold text-gray-900 dark:text-white';
      out.push(
        <div key={out.length} role="heading" aria-level={level} className={cls}>
          {renderInline(heading[2])}
        </div>
      );
    } else if (bullet) {
      flushParagraph();
      list.push(bullet[1]);
    } else if (line.trim() === '') {
      flushParagraph();
      flushList();
    } else {
      flushList();
      paragraph.push(line.trim());
    }
  }
  flushParagraph();
  flushList();

  return <div className="space-y-3">{out}</div>;
}

// ---------------------------------------------------------------------------
// Layouts
// ---------------------------------------------------------------------------

function Unsupported({ layout }: { layout?: string }) {
  return (
    <div
      role="note"
      className="flex items-start gap-2 rounded-md border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 p-3 text-sm text-gray-600 dark:text-gray-300"
    >
      <Info className="w-4 h-4 mt-0.5 flex-shrink-0" />
      <span>
        This content uses a layout that is not supported by this version of OpenVox WebUI
        {layout ? ` (${layout})` : ''}.
      </span>
    </div>
  );
}

function TableBlock({ spec }: { spec: Json }) {
  const columns = (Array.isArray(spec.columns) ? spec.columns : []).filter(isObject);
  const rows = (Array.isArray(spec.rows) ? spec.rows : []).filter(isObject);
  const hasDataSource = spec.data_source !== undefined && spec.data_source !== null;

  return (
    <div className="space-y-2">
      {hasDataSource && (
        <p role="note" className="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400">
          <Info className="w-3 h-3" />
          Data sources arrive with the WASM runtime; only the static rows are shown.
        </p>
      )}
      <div className="overflow-x-auto">
        <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
          <thead className="bg-gray-50 dark:bg-gray-800">
            <tr>
              {columns.map((col, i) => (
                <th
                  key={i}
                  className="px-4 py-2 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase"
                >
                  {toText(col.label ?? col.key)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-200 dark:divide-gray-700">
            {rows.length === 0 ? (
              <tr>
                <td
                  colSpan={Math.max(columns.length, 1)}
                  className="px-4 py-3 text-sm text-gray-500 dark:text-gray-400"
                >
                  No rows
                </td>
              </tr>
            ) : (
              rows.map((row, r) => (
                <tr key={r}>
                  {columns.map((col, c) => (
                    <td key={c} className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300">
                      {toText(row[toText(col.key)])}
                    </td>
                  ))}
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function StatsBlock({ spec }: { spec: Json }) {
  const items = (Array.isArray(spec.items) ? spec.items : []).filter(isObject);
  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
      {items.map((item, i) => (
        <div key={i} className="rounded-lg border border-gray-200 dark:border-gray-700 p-4">
          <p className="text-sm text-gray-500 dark:text-gray-400">{toText(item.label)}</p>
          <p className="text-2xl font-semibold text-gray-900 dark:text-white">
            {toText(item.value)}
          </p>
          {item.hint !== undefined && item.hint !== null && (
            <p className="text-xs text-gray-400 dark:text-gray-500">{toText(item.hint)}</p>
          )}
        </div>
      ))}
    </div>
  );
}

function DetailBlock({ spec }: { spec: Json }) {
  const fields = (Array.isArray(spec.fields) ? spec.fields : []).filter(isObject);
  return (
    <div className="space-y-3">
      {spec.title !== undefined && spec.title !== null && (
        <h3 className="text-base font-semibold text-gray-900 dark:text-white">
          {toText(spec.title)}
        </h3>
      )}
      <dl className="grid grid-cols-1 sm:grid-cols-2 gap-x-6 gap-y-3">
        {fields.map((field, i) => (
          <div key={i}>
            <dt className="text-xs uppercase text-gray-500 dark:text-gray-400">
              {toText(field.label)}
            </dt>
            <dd className="text-sm text-gray-900 dark:text-gray-100 break-words">
              {toText(field.value)}
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

export interface DeclarativeRendererProps {
  spec: unknown;
}

function renderBlock(spec: unknown, depth: number): ReactNode {
  if (!isObject(spec)) return <Unsupported />;
  const layout = typeof spec.layout === 'string' ? spec.layout : undefined;
  switch (layout) {
    case 'markdown':
      return <MarkdownBlock content={toText(spec.content)} />;
    case 'table':
      return <TableBlock spec={spec} />;
    case 'stats':
      return <StatsBlock spec={spec} />;
    case 'detail':
      return <DetailBlock spec={spec} />;
    case 'stack': {
      if (depth >= MAX_DEPTH) return <Unsupported layout="nesting too deep" />;
      const blocks = Array.isArray(spec.blocks) ? spec.blocks : [];
      return (
        <div className="space-y-6">
          {blocks.map((b, i) => (
            <Fragment key={i}>{renderBlock(b, depth + 1)}</Fragment>
          ))}
        </div>
      );
    }
    default:
      return <Unsupported layout={layout} />;
  }
}

export default function DeclarativeRenderer({ spec }: DeclarativeRendererProps) {
  return <div data-testid="declarative-renderer">{renderBlock(spec, 0)}</div>;
}
