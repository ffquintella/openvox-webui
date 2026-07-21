import { useState, useEffect, useRef, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Search, Loader2, Check, X } from 'lucide-react';
import clsx from 'clsx';
import { api } from '../services/api';

interface NodeAutocompleteProps {
  /** Currently committed certname (empty string when nothing valid is selected). */
  value: string;
  /** Called with the certname when a real node is selected, or '' when cleared. */
  onChange: (certname: string) => void;
  /** Certnames to hide from the results (e.g. already-pinned nodes). */
  excluded?: string[];
  placeholder?: string;
  /** Max number of matching nodes to fetch per search. */
  limit?: number;
  /**
   * When true, the typed text is committed as-is even if it does not match a
   * known node. Suggestions are still offered from the backend. Use this where
   * a not-yet-reported certname is a legitimate value (e.g. facter generation).
   * When false (default), only a node that exists in the results can be
   * selected, guarding against pinning non-existent nodes.
   */
  allowFreeText?: boolean;
  className?: string;
}

// Escape user input so it is matched literally by PuppetDB's regex-based
// certname search rather than being interpreted as a regular expression.
function escapeRegex(input: string): string {
  return input.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Searchable, server-side autocomplete for selecting a single node by certname.
 *
 * Unlike a plain <select>, this does not require the whole fleet to be loaded
 * up front (which previously truncated the list to the configured page size).
 * As the user types, matching nodes are fetched from the backend. Only a node
 * that exists in the results can be selected, so free-typed values that do not
 * correspond to a real node are never committed.
 */
export default function NodeAutocomplete({
  value,
  onChange,
  excluded = [],
  placeholder = 'Search nodes...',
  limit = 25,
  allowFreeText = false,
  className,
}: NodeAutocompleteProps) {
  const [query, setQuery] = useState(value);
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const [isOpen, setIsOpen] = useState(false);
  const [highlightedIndex, setHighlightedIndex] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);

  // Debounce the query so we don't hit the backend on every keystroke.
  useEffect(() => {
    const handle = setTimeout(() => setDebouncedQuery(query.trim()), 250);
    return () => clearTimeout(handle);
  }, [query]);

  // Reflect external value changes (e.g. a programmatic reset). In strict mode a
  // typed-but-uncommitted query intentionally leaves value === '', so don't let
  // that clobber what the user is typing.
  useEffect(() => {
    if (value !== query && (allowFreeText || value !== '')) {
      setQuery(value);
    }
  }, [value, allowFreeText, query]);

  const { data, isFetching } = useQuery({
    queryKey: ['node-search', debouncedQuery, limit],
    queryFn: () =>
      api.getNodesPaginated({
        search: debouncedQuery ? escapeRegex(debouncedQuery) : undefined,
        limit,
        order_by: 'certname',
        order_dir: 'asc',
      }),
    enabled: isOpen,
    placeholderData: (prev) => prev,
  });

  const excludedSet = useMemo(() => new Set(excluded), [excluded]);
  const results = useMemo(
    () => (data?.nodes ?? []).filter((n) => !excludedSet.has(n.certname)),
    [data, excludedSet]
  );
  const total = data?.total ?? 0;
  const hiddenCount = Math.max(0, total - results.length);

  // Keep the highlighted item within range as results change.
  useEffect(() => {
    setHighlightedIndex(0);
  }, [debouncedQuery, results.length]);

  // Close the dropdown when clicking outside the component.
  useEffect(() => {
    if (!isOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClick);
    return () => document.removeEventListener('mousedown', handleClick);
  }, [isOpen]);

  const select = (certname: string) => {
    onChange(certname);
    setQuery(certname);
    setIsOpen(false);
  };

  const clear = () => {
    onChange('');
    setQuery('');
    setDebouncedQuery('');
    setIsOpen(true);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (!isOpen) setIsOpen(true);
      setHighlightedIndex((i) => Math.min(i + 1, results.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHighlightedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === 'Enter') {
      if (isOpen && results[highlightedIndex]) {
        e.preventDefault();
        select(results[highlightedIndex].certname);
      }
    } else if (e.key === 'Escape') {
      setIsOpen(false);
    }
  };

  return (
    <div ref={containerRef} className={clsx('relative', className)}>
      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400 pointer-events-none" />
        <input
          type="text"
          value={query}
          placeholder={placeholder}
          onChange={(e) => {
            const next = e.target.value;
            setQuery(next);
            if (allowFreeText) {
              // Commit the typed text directly; suggestions are advisory.
              onChange(next);
            } else if (value) {
              // Typing invalidates any previously committed selection until the
              // user picks a real node from the list again.
              onChange('');
            }
            setIsOpen(true);
          }}
          onFocus={() => setIsOpen(true)}
          onKeyDown={handleKeyDown}
          className="input pl-9 pr-9"
          autoComplete="off"
          role="combobox"
          aria-expanded={isOpen}
          aria-autocomplete="list"
        />
        {value && !allowFreeText ? (
          <Check className="absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-green-500" />
        ) : query ? (
          <button
            type="button"
            onClick={clear}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200"
            aria-label="Clear"
          >
            <X className="w-4 h-4" />
          </button>
        ) : null}
      </div>

      {isOpen && (
        <div className="absolute z-20 mt-1 w-full bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg max-h-64 overflow-y-auto">
          {isFetching && results.length === 0 ? (
            <div className="flex items-center gap-2 px-3 py-3 text-sm text-gray-500 dark:text-gray-400">
              <Loader2 className="w-4 h-4 animate-spin" />
              Searching...
            </div>
          ) : results.length === 0 ? (
            <div className="px-3 py-3 text-sm text-gray-500 dark:text-gray-400">
              {debouncedQuery ? 'No matching nodes found' : 'No nodes available'}
            </div>
          ) : (
            <>
              <ul role="listbox">
                {results.map((node, index) => (
                  <li
                    key={node.certname}
                    role="option"
                    aria-selected={index === highlightedIndex}
                    onMouseDown={(e) => {
                      // Prevent input blur before the click registers.
                      e.preventDefault();
                      select(node.certname);
                    }}
                    onMouseEnter={() => setHighlightedIndex(index)}
                    className={clsx(
                      'px-3 py-2 text-sm cursor-pointer text-gray-900 dark:text-gray-100',
                      index === highlightedIndex
                        ? 'bg-primary-50 dark:bg-primary-900/30'
                        : 'hover:bg-gray-50 dark:hover:bg-gray-700/50'
                    )}
                  >
                    {node.certname}
                  </li>
                ))}
              </ul>
              {hiddenCount > 0 && (
                <div className="px-3 py-2 text-xs text-gray-400 dark:text-gray-500 border-t border-gray-100 dark:border-gray-700">
                  {hiddenCount} more match{hiddenCount === 1 ? '' : 'es'} — keep typing to narrow results
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
