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
  isolate: (containerName: string, allNames: readonly string[]) => void;
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

export const FilterProvider = ({ children }: { children: ReactNode }) => {
  // SSR では空集合 (= 全表示) で描画する。cacheComponents 構成上、
  // searchParams を server 側で読むと page 全体が dynamic になってしまうため、
  // URL の ?hidden= は client mount 時に読み取って state に反映する。
  // 一瞬全 chip が pressed で表示されてから filtered になるフラッシュは許容。
  const [hidden, setHidden] = useState<Set<string>>(() => new Set());

  // mount 時に URL → state を同期。SSR 中は実行されない。
  useEffect(() => {
    const param = new URLSearchParams(window.location.search).get('hidden');
    if (param) setHidden(parseHiddenParam(param));
  }, []);

  // 以降の hidden 変更を URL に書き戻す。初回 (mount) は同期処理に任せて skip。
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

  const isolate = useCallback(
    (containerName: string, allNames: readonly string[]) => {
      setHidden(new Set(allNames.filter((n) => n !== containerName)));
    },
    [],
  );

  const clear = useCallback(() => {
    setHidden(new Set());
  }, []);

  const value = useMemo(
    () => ({ hidden, toggle, setAll, isolate, clear }),
    [hidden, toggle, setAll, isolate, clear],
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
