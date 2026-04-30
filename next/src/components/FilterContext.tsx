'use client'

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';

type FilterContextValue = {
  hidden: ReadonlySet<string>;
  toggle: (containerName: string) => void;
  setAll: (containerNames: readonly string[]) => void;
  clear: () => void;
};

const FilterContext = createContext<FilterContextValue | null>(null);

const parseHiddenParam = (value: string | undefined): Set<string> => {
  if (!value) return new Set();
  return new Set(value.split(',').map((s) => s.trim()).filter(Boolean));
};

const serializeHiddenParam = (set: ReadonlySet<string>): string =>
  [...set].sort().join(',');

const syncUrl = (set: ReadonlySet<string>) => {
  if (typeof window === 'undefined') return;
  const url = new URL(window.location.href);
  if (set.size === 0) {
    url.searchParams.delete('hidden');
  } else {
    url.searchParams.set('hidden', serializeHiddenParam(set));
  }
  window.history.replaceState(null, '', url.toString());
};

export const FilterProvider = ({
  children,
  initialHiddenParam,
}: {
  children: ReactNode;
  initialHiddenParam?: string;
}) => {
  const [hidden, setHidden] = useState<Set<string>>(() =>
    parseHiddenParam(initialHiddenParam),
  );

  // 初回マウント時の URL は initialHiddenParam と一致しているので上書きしない。
  // 以降の変更だけ history に反映する。
  // 注: dev では Next.js が history.replaceState を傍受して
  // 内部 Router の state を更新するため、StrictMode の二重実行で
  // "Cannot update a component (Router) while rendering FilterProvider" の
  // 警告が出る。本番ビルドでは出ない既知の dev-only 警告として許容している。
  const isFirstSync = useRef(true);
  useEffect(() => {
    if (isFirstSync.current) {
      isFirstSync.current = false;
      return;
    }
    syncUrl(hidden);
  }, [hidden]);

  const toggle = useCallback((containerName: string) => {
    setHidden((prev) => {
      const next = new Set(prev);
      if (next.has(containerName)) next.delete(containerName);
      else next.add(containerName);
      return next;
    });
  }, []);

  const setAll = useCallback((containerNames: readonly string[]) => {
    setHidden(new Set(containerNames));
  }, []);

  const clear = useCallback(() => {
    setHidden(new Set());
  }, []);

  const value = useMemo(
    () => ({ hidden, toggle, setAll, clear }),
    [hidden, toggle, setAll, clear],
  );

  return (
    <FilterContext.Provider value={value}>{children}</FilterContext.Provider>
  );
};

export const useFilter = (): FilterContextValue => {
  const ctx = useContext(FilterContext);
  if (!ctx) {
    throw new Error('useFilter must be used inside FilterProvider');
  }
  return ctx;
};
