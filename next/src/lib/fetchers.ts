import z from 'zod';

// dev (host): localhost フォールバック
// prod (Vercel): API_URL を環境変数で `https://<api-domain>/cephylas` に設定
const API_URL = process.env.API_URL ?? 'http://localhost:7878';

// 'use cache' で返すため、Error インスタンスではなく serializable な形にする。
export type FetchResult<T> =
  | { ok: true;  data: T; }
  | { ok: false; error: { message: string }; }
  ;

// 1 系列。t はエポックからのミリ秒、v は値 (欠損は null)。t と v は同じ長さ。
// core は系列ごとに独立して間引くので、同じコンテナでも系列ごとに t が違いうる。
const seriesSchema = z
  .object({
    t: z.array(z.number()),
    v: z.array(z.number().nullable()),
  })
  .refine((series) => series.t.length === series.v.length, {
    message: 't and v must have the same length',
  });
export type UsageSeries = z.infer<typeof seriesSchema>;

// GET /usages/{kind} のレスポンス: コンテナ名 → 系列名 → 系列
const usagesSchemas = {
  cpu: z.record(z.string(), z.object({ percentage: seriesSchema })),
  memory: z.record(z.string(), z.object({ percentage: seriesSchema })),
  io: z.record(z.string(), z.object({ read: seriesSchema, write: seriesSchema })),
  net: z.record(z.string(), z.object({ recv: seriesSchema, send: seriesSchema })),
} as const;

export type UsageKind = keyof typeof usagesSchemas;
export type Usages<K extends UsageKind> = z.infer<(typeof usagesSchemas)[K]>;

/**
 * 全コンテナ分の系列を 1 リクエストで取得する。例外は投げない。
 * cache は宣言しない (呼び出す chart 側の 'use cache' に含まれる)。
 */
export const fetchUsages = async <K extends UsageKind>(
  kind: K,
): Promise<FetchResult<Usages<K>>> => {
  try {
    const response = await fetch(`${API_URL}/usages/${kind}`);
    if (!response.ok) {
      console.error(response.statusText);
      return {
        ok: false,
        error: { message: `fetch failed with ${response.status}` },
      };
    }
    const json: unknown = await response.json();
    // kind から schema を引いているので、検証を通った値は Usages<K> である
    // (generic な添字アクセスでは TypeScript がこの対応を追えない)。
    const data = usagesSchemas[kind].parse(json) as Usages<K>;
    return { ok: true, data };
  } catch (err) {
    console.error(err);
    return {
      ok: false,
      error: { message: err instanceof Error ? err.message : 'Unknown error' },
    };
  }
};

// コードポイント順の比較。core の並び (Rust の String の順序) と同じ結果になる。
// `<` は UTF-16 のコード単位で比べるので、サロゲートペアを含む名前で順序が変わる。
// localeCompare はロケール依存で、大文字と小文字の並びも変わるので使わない。
const compareByCodePoint = (a: string, b: string): number => {
  const ia = a[Symbol.iterator]();
  const ib = b[Symbol.iterator]();
  for (;;) {
    const ca = ia.next();
    const cb = ib.next();
    if (ca.done || cb.done) return (ca.done ? 0 : 1) - (cb.done ? 0 : 1);
    const diff = ca.value.codePointAt(0)! - cb.value.codePointAt(0)!;
    if (diff !== 0) return diff;
  }
};

/**
 * コンテナ名を表示順に並べる。色はこの並びの位置で決まる (lib/colors.ts)。
 * JSON のキーの順序には依存しない。
 */
export const sortedContainerNames = (usages: Record<string, unknown>): string[] =>
  Object.keys(usages).sort(compareByCodePoint);

/** 1 系列を Chart.js の点列にする。x はエポックからのミリ秒。 */
export const toPoints = (
  series: UsageSeries,
): { x: number; y: number | null }[] =>
  series.t.map((t, i) => ({ x: t, y: series.v[i] }));
