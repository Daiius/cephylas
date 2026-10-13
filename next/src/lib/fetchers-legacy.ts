// 🧹 移行のための一時的なコードである。
// 本番の core が `/usages/{kind}` を持つ版に更新されたら、このファイルと、
// fetchers.ts からの呼び出し (応答が 2xx 以外のときの分岐) を削除する。
//
// `/usages/{kind}` を持たない古い core に対して、コンテナ別の API
// (`/containers` と `/containers/{name}/...`) から同じ内容を組み立てる。
// 返す形は `/usages/{kind}` と同じ (コンテナ名 → 系列名 → { t, v }) なので、
// 呼び出し側は以降の処理を分岐させなくてよい。
//
// ここでは 'use cache' を宣言しない (呼び出す chart 側の cache に含まれる)。
import z from 'zod';

import { accessHeaders } from '@/lib/access-headers';

type LegacyKind = 'cpu' | 'memory' | 'io' | 'net';
type LegacySeries = { t: number[]; v: (number | null)[] };
export type LegacyUsages = Record<string, Record<string, LegacySeries>>;

const containersSchema = z.array(z.string());

// コンテナ別の API の 1 要素。種別によって持つ項目が違うので、すべて任意にする。
// time は ISO 8601 の文字列で、小数秒は 3 桁 (ミリ秒) のことも 9 桁 (ナノ秒) の
// こともある。値は整数のことも小数のこともあり、欠損は null または項目なし。
const pointsSchema = z.array(
  z.object({
    time: z.string().nullish(),
    percentage: z.number().nullish(),
    readkBps: z.number().nullish(),
    writekBps: z.number().nullish(),
    recvkBps: z.number().nullish(),
    sendkBps: z.number().nullish(),
  }),
);
type LegacyPoint = z.infer<typeof pointsSchema>[number];
type ValueField = Exclude<keyof LegacyPoint, 'time'>;

// 種別ごとの系列: `/usages` での系列名、コンテナ別 API のパス、値を持つ項目。
// io/read と io/write (net/recv と net/send) は同じ形の要素を返すが、core が
// 間引きの基準にする値が違うので、系列ごとに別のパスを叩く。
const seriesOf: Record<
  LegacyKind,
  readonly { name: string; path: string; field: ValueField }[]
> = {
  cpu: [{ name: 'percentage', path: 'cpu', field: 'percentage' }],
  memory: [{ name: 'percentage', path: 'memory', field: 'percentage' }],
  io: [
    { name: 'read', path: 'io/read', field: 'readkBps' },
    { name: 'write', path: 'io/write', field: 'writekBps' },
  ],
  net: [
    { name: 'recv', path: 'net/recv', field: 'recvkBps' },
    { name: 'send', path: 'net/send', field: 'sendkBps' },
  ],
};

// この経路のリクエストは、すべてここを通る (Cloudflare Access のヘッダもここで付ける)。
const getJson = async <T>(url: string, schema: z.ZodType<T>): Promise<T> => {
  const response = await fetch(url, { headers: accessHeaders() });
  if (!response.ok) {
    throw new Error(`fetch failed with ${response.status}`);
  }
  return schema.parse(await response.json());
};

// 時刻を解釈できない要素は捨てる。t と v は必ず同じ長さになる。
const toSeries = (points: LegacyPoint[], field: ValueField): LegacySeries => {
  const series: LegacySeries = { t: [], v: [] };
  for (const point of points) {
    const t = point.time == null ? NaN : Date.parse(point.time);
    if (Number.isNaN(t)) continue;
    series.t.push(t);
    series.v.push(point[field] ?? null);
  }
  return series;
};

/**
 * コンテナ別の API から、`/usages/{kind}` と同じ形のデータを組み立てる。
 * 1 本でも失敗したら例外を投げる (呼び出し側が全体を失敗として扱う)。
 * リクエストは `/containers` の 1 本と、コンテナ数 × 系列数の本数になる。
 */
export const fetchUsagesFromContainerApis = async (
  apiUrl: string,
  kind: LegacyKind,
): Promise<LegacyUsages> => {
  const names = await getJson(`${apiUrl}/containers`, containersSchema);
  const series = seriesOf[kind];

  const entries = await Promise.all(
    names.map(async (name) => {
      const fetched = await Promise.all(
        series.map(async ({ name: seriesName, path, field }) => {
          const points = await getJson(
            `${apiUrl}/containers/${name}/${path}`,
            pointsSchema,
          );
          return [seriesName, toSeries(points, field)] as const;
        }),
      );
      return [name, Object.fromEntries(fetched)] as const;
    }),
  );

  return Object.fromEntries(entries);
};
