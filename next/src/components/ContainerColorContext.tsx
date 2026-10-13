'use client'

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

import {
  backgroundColorFor,
  borderColorFor,
  sortContainerNames,
} from '@/lib/colors';

export type ContainerColor = { border: string; background: string };
export type ColorOf = (containerName: string) => ContainerColor;

type ContainerColorContextValue = {
  /** 画面全体 (全チャートの和集合) での、コンテナ名 → 並びの位置 */
  indexOf: ReadonlyMap<string, number>;
  register: (id: string, names: readonly string[]) => void;
  unregister: (id: string) => void;
};

const ContainerColorContext =
  createContext<ContainerColorContextValue | null>(null);

const sameNames = (a: readonly string[], b: readonly string[]): boolean =>
  a.length === b.length && a.every((name, i) => name === b[i]);

const indexMapOf = (names: readonly string[]): Map<string, number> =>
  new Map(names.map((name, i) => [name, i]));

/**
 * コンテナの色を、画面全体で 1 つの名前一覧から決める。
 *
 * 各チャートは別々にキャッシュされた応答を描くので、コンテナが増減した直後は
 * チャートごとに持っている名前が違いうる。チャートごとの一覧で色を決めると、
 * 同じコンテナが画面内で違う色になる。そこで各チャートに自分の名前を登録させ、
 * その和集合を並べた位置で色を決める。
 */
export const ContainerColorProvider = ({ children }: { children: ReactNode }) => {
  // チャートごとの登録 (id → そのチャートが持つ名前)
  const [registered, setRegistered] = useState<
    ReadonlyMap<string, readonly string[]>
  >(() => new Map());

  const register = useCallback((id: string, names: readonly string[]) => {
    setRegistered((prev) => {
      const current = prev.get(id);
      if (current && sameNames(current, names)) return prev;
      return new Map(prev).set(id, names);
    });
  }, []);

  const unregister = useCallback((id: string) => {
    setRegistered((prev) => {
      if (!prev.has(id)) return prev;
      const next = new Map(prev);
      next.delete(id);
      return next;
    });
  }, []);

  const indexOf = useMemo(
    () => indexMapOf(sortContainerNames([...registered.values()].flat())),
    [registered],
  );

  const value = useMemo(
    () => ({ indexOf, register, unregister }),
    [indexOf, register, unregister],
  );

  return (
    <ContainerColorContext.Provider value={value}>
      {children}
    </ContainerColorContext.Provider>
  );
};

/**
 * このチャートが持つコンテナ名を登録し、名前から色を引く関数を返す。
 * 線の色 (Chart) と凡例の色 (MiniLegend) は、必ずこの同じ関数から取る。
 *
 * 和集合が変わると関数も新しくなるので、それを契機に全チャートが色を更新する。
 * 登録が反映される前 (最初の描画) は、和集合に無い名前について、このチャート自身の
 * 名前一覧での位置を使う。名前が全チャートで同じなら、最初から最終的な色になる。
 */
export const useContainerColors = (ownNames: readonly string[]): ColorOf => {
  const ctx = useContext(ContainerColorContext);
  if (!ctx) {
    throw new Error(
      'useContainerColors must be used inside ContainerColorProvider',
    );
  }
  const { indexOf, register, unregister } = ctx;
  const id = useId();

  // ownNames は描画のたびに新しい配列で来うるので、中身で同一性を判定する。
  const key = JSON.stringify(ownNames);
  const sortedOwnNames = useMemo(
    () => sortContainerNames(JSON.parse(key) as string[]),
    [key],
  );

  useEffect(() => {
    register(id, sortedOwnNames);
  }, [id, sortedOwnNames, register]);
  // アンマウントのときだけ登録を外す (名前が変わるたびに外すと、色が一瞬ずれる)。
  useEffect(() => () => unregister(id), [id, unregister]);

  const ownIndexOf = useMemo(() => indexMapOf(sortedOwnNames), [sortedOwnNames]);

  return useCallback(
    (containerName: string) => {
      const index =
        indexOf.get(containerName) ?? ownIndexOf.get(containerName) ?? 0;
      return {
        border: borderColorFor(index),
        background: backgroundColorFor(index),
      };
    },
    [indexOf, ownIndexOf],
  );
};
