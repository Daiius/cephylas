'use client'

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useState,
  useSyncExternalStore,
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

// 「このチャートのハイドレートが済んだか」を表すフラグ。値は変わらないので購読は空である。
// サーバでの描画とハイドレート中の描画では false、ハイドレートの直後に React が true で
// 描き直す。ハイドレートを伴わずにクライアントで初めて描くときは、最初から true になる
// (合わせるべき HTML が無い)。
const subscribeNothing = () => () => {};
const useHydrated = (): boolean =>
  useSyncExternalStore(
    subscribeNothing,
    () => true,
    () => false,
  );

/**
 * このチャートが持つコンテナ名を登録し、名前から色を引く関数を返す。
 * 線の色 (Chart) と凡例の色 (MiniLegend) は、必ずこの同じ関数から取る。
 *
 * **ハイドレートが済むまでは、必ずこのチャート自身の名前一覧での位置で色を決める**
 * (和集合を見ない)。サーバでは登録 (effect) が走らないので、サーバが出す HTML は
 * 必ずこの色である。先にハイドレートした別のチャートの登録が和集合に入っていても、
 * 最初の描画をサーバの HTML と一致させる。React は本番で属性の食い違いを直さないので、
 * 食い違うと凡例の点だけがサーバの色のまま残り、線の色とずれる。
 *
 * ハイドレートのあとは和集合での位置を使う (和集合にまだ無い名前は、自身の一覧での
 * 位置)。和集合が変わると関数も新しくなるので、それを契機に全チャートが色を更新する。
 * 名前が全チャートで同じなら、どの段階でも同じ色になる (切り替えで色は変わらない)。
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
  const hydrated = useHydrated();

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
        (hydrated ? indexOf.get(containerName) : undefined) ??
        ownIndexOf.get(containerName) ??
        0;
      return {
        border: borderColorFor(index),
        background: backgroundColorFor(index),
      };
    },
    [hydrated, indexOf, ownIndexOf],
  );
};
