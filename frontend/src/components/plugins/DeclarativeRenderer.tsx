import { Fragment, useMemo, type ReactNode } from 'react';
import { Link, useInRouterContext } from 'react-router-dom';
import { DeclarativeContext, type DeclarativeEnv } from './declarativeContext';
import { isHostRoute, isObject, str, type Json } from './declarativeSpec';
import {
  BlockHeader,
  DetailBlock,
  FormBlock,
  NoticeBlock,
  StatsBlock,
  TableBlock,
  Unsupported,
} from './DeclarativeData';
import type { UiRouteAccess } from '../../types';

/**
 * Renders a plugin-supplied declarative spec with host components only
 * (format in PLUGIN_PLATFORM.md section 17.18). Every value is rendered as
 * React text; there is no dangerouslySetInnerHTML and no plugin JavaScript.
 * Links go only to host routes; data and actions only to the plugin's own
 * routes that the server listed for this user.
 */

const MAX_DEPTH = 4;

// ---------------------------------------------------------------------------
// Minimal markdown: headings, bullet lists, paragraphs, **bold**, `code` and
// [text](/host/route). Everything else, including other links, is text.
// ---------------------------------------------------------------------------

const INLINE_RE = /(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\([^)\s]+\))/g;

function HostLink({ to, children }: { to: string; children: ReactNode }) {
  const className = 'text-primary-600 hover:underline';
  return useInRouterContext() ? (
    <Link to={to} className={className}>
      {children}
    </Link>
  ) : (
    <a href={to} className={className}>
      {children}
    </a>
  );
}

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
      return isHostRoute(link[2]) ? (
        <HostLink key={i} to={link[2]}>
          {link[1]}
        </HostLink>
      ) : (
        <Fragment key={i}>{link[1]}</Fragment>
      );
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
// Blocks
// ---------------------------------------------------------------------------

function renderBlock(spec: unknown, depth: number): ReactNode {
  if (!isObject(spec)) return <Unsupported />;
  const layout = str(spec.layout);
  switch (layout) {
    case 'markdown':
      return (
        <div className="space-y-2">
          <BlockHeader spec={spec} />
          <MarkdownBlock content={str(spec.content) ?? ''} />
        </div>
      );
    case 'notice':
      return <NoticeBlock spec={spec} />;
    case 'table':
      return <TableBlock spec={spec} />;
    case 'stats':
      return <StatsBlock spec={spec} />;
    case 'detail':
      return <DetailBlock spec={spec} />;
    case 'form':
      return <FormBlock spec={spec} />;
    case 'stack': {
      if (depth >= MAX_DEPTH) return <Unsupported layout="nesting too deep" />;
      const blocks = Array.isArray(spec.blocks) ? spec.blocks : [];
      return (
        <div className="space-y-6">
          <BlockHeader spec={spec as Json} />
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

export interface DeclarativeRendererProps {
  spec: unknown;
  /** The contributing plugin; without it, data-bound blocks are not rendered. */
  pluginId?: string;
  /** Routes of the plugin the spec uses, as the server listed them for this user. */
  routes?: UiRouteAccess[];
  /** The slot context (for example `certname`), for route parameters. */
  params?: Record<string, string>;
}

export default function DeclarativeRenderer({
  spec,
  pluginId,
  routes,
  params,
}: DeclarativeRendererProps) {
  const paramsKey = JSON.stringify(params ?? {});
  const env = useMemo<DeclarativeEnv | null>(
    () =>
      pluginId
        ? {
            pluginId,
            routes: routes ?? [],
            params: JSON.parse(paramsKey) as Record<string, string>,
          }
        : null,
    [pluginId, routes, paramsKey]
  );
  return (
    <DeclarativeContext.Provider value={env}>
      <div data-testid="declarative-renderer">{renderBlock(spec, 0)}</div>
    </DeclarativeContext.Provider>
  );
}
