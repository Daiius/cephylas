'use client'

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
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

export const FilterProvider = ({ children }: { children: ReactNode }) => {
  const [hidden, setHidden] = useState<Set<string>>(() => new Set());

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
