import z from 'zod';

import { coreFetch } from '@/lib/core-fetch';
import { sortContainerNames } from '@/lib/colors';
import { fetchUsagesFromContainerApis } from '@/lib/fetchers-legacy';

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
 *
 * 🧹 `/usages/{kind}` の応答が 2xx 以外のステータスだったときは、コンテナ別の API から
 * 同じ形を組み立てる (`/usages` を持たない古い core 向けの一時的な経路。
 * fetchers-legacy.ts)。古い core の手前で 404 以外 (403 など) が返る構成でも
 * 表示できるよう、ステータスは限定しない。
 * 次の場合は切り替えず、失敗を返す。
 * - 接続そのものの失敗 (fetch が例外を投げる)。同じ宛先なので試しても無駄である
 * - 期限切れ (core-fetch.ts)。応答のヘッダを待つ間でも、本文を読む間でも同じ。
 *   接続の失敗と同じく、同じ宛先が応答しないので試しても無駄である
 * - 2xx だが本文が不正 (JSON として読めない、検証の失敗)。`/usages` を持つ core の
 *   不具合であり、黙って別の経路に逃がさない
 */
export const fetchUsages = async <K extends UsageKind>(
  kind: K,
): Promise<FetchResult<Usages<K>>> => {
  try {
    // coreFetch が、期限と Cloudflare Access のサービストークン (設定されているとき
    // だけ) を付ける。トークンを付けるときはリダイレクトを追わないので、3xx は下の
    // 「2xx 以外」に入る。期限切れは例外になり (本文の読み取り中でも同じ)、下の catch で
    // 失敗として返る。互換の経路には入らない。
    const response = await coreFetch(`${API_URL}/usages/${kind}`);
    let json: unknown;
    if (response.ok) {
      json = await response.json();
    } else {
      // どちらの経路が使われているかを、サーバ側のログで確かめられるようにする。
      console.error(
        `fetchUsages(${kind}): /usages/${kind} responded with ${response.status}; ` +
          'falling back to the per-container API',
      );
      json = await fetchUsagesFromContainerApis(API_URL, kind);
    }
    // どちらの経路で得た値も、同じ schema で検証する。
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

/**
 * 応答に含まれるコンテナ名を表示順に並べる (データセットと凡例の並び)。
 * JSON のキーの順序には依存しない。色はここでは決めない
 * (画面全体の名前の一覧からクライアント側で決める。lib/colors.ts)。
 */
export const sortedContainerNames = (usages: Record<string, unknown>): string[] =>
  sortContainerNames(Object.keys(usages));

/** 1 系列を Chart.js の点列にする。x はエポックからのミリ秒。 */
export const toPoints = (
  series: UsageSeries,
): { x: number; y: number | null }[] =>
  series.t.map((t, i) => ({ x: t, y: series.v[i] }));
