// core の手前に置く Cloudflare Access を通るための、サービストークンのヘッダ。
//
// 🔒 サーバ専用である。クライアントコンポーネントから import しない
// (import してよいのは fetchers.ts と fetchers-legacy.ts だけ)。
// 🔒 トークンの値を、ログ・エラーメッセージ・戻り値 (FetchResult や props) に含めない。
// このモジュールが値を渡す先は、fetch の headers だけである。
// 🔒 ヘッダを付ける fetch では、リダイレクトを追わない。独自のヘッダは別オリジンへの
// リダイレクトにも転送されるので、追うとトークンが core 以外の宛先へ渡りうる。
// headers と redirect を別々に指定せず、必ず accessFetchInit() の戻り値をそのまま渡す。

export type AccessHeaders = {
  'CF-Access-Client-Id': string;
  'CF-Access-Client-Secret': string;
};

/**
 * - `unset`: どちらも未設定。ヘッダを付けない (Access を導入する前の動作)
 * - `ok`: 両方が設定されている。ヘッダを付ける
 * - `partial`: 片方だけ設定されている。設定の誤りなので、ヘッダを付けない
 * - `invalid`: 両方あるが、ヘッダの値として送れない文字を含む。ヘッダを付けない
 */
export type AccessHeadersResult =
  | { status: 'ok'; headers: AccessHeaders }
  | { status: 'unset' | 'partial' | 'invalid'; headers: Record<string, never> };

// ヘッダの値として送れる文字 (空白と制御文字を除く ASCII)。
// これ以外を fetch に渡すと例外になり、その文言に値そのものが入りうる。
// 値がログや FetchResult に出ないよう、fetch に渡す前にここで弾く。
const HEADER_VALUE = /^[\x21-\x7e]+$/;

/**
 * 環境変数の 2 つの値から、core への fetch に付けるヘッダを組み立てる (純粋な関数)。
 * 前後の空白は取り除く。未設定と、空白だけの値は区別しない。
 */
export const buildAccessHeaders = (
  clientId: string | undefined,
  clientSecret: string | undefined,
): AccessHeadersResult => {
  const id = clientId?.trim() ?? '';
  const secret = clientSecret?.trim() ?? '';

  if (id === '' && secret === '') return { status: 'unset', headers: {} };
  if (id === '' || secret === '') return { status: 'partial', headers: {} };
  if (!HEADER_VALUE.test(id) || !HEADER_VALUE.test(secret)) {
    return { status: 'invalid', headers: {} };
  }
  return {
    status: 'ok',
    headers: { 'CF-Access-Client-Id': id, 'CF-Access-Client-Secret': secret },
  };
};

/** core への fetch に渡すオプション。ヘッダを付けるときは、必ずリダイレクトを追わない。 */
export type AccessFetchInit =
  | { headers: AccessHeaders; redirect: 'manual' }
  | Record<string, never>;

/**
 * ヘッダの有無と、リダイレクトを追うかどうかを連動させる (純粋な関数)。
 * ヘッダを付けないときは何も指定しない (fetch の既定どおりリダイレクトを追う)。
 */
export const toAccessFetchInit = (result: AccessHeadersResult): AccessFetchInit =>
  result.status === 'ok' ? { headers: result.headers, redirect: 'manual' } : {};

// 警告はプロセスにつき 1 回だけ出す (リクエストのたびに出すと、互換の経路で数十行になる)。
let warned = false;

/**
 * core への fetch に渡すオプションを返す。core へのリクエストは、必ずこれを通す。
 * 環境変数はサーバ側でだけ読む (`NEXT_PUBLIC_` を付けない)。
 * 設定に誤りがあるときは、ヘッダを付けず、サーバ側のログに警告を出す (値は出さない)。
 *
 * ヘッダを付けたリクエストが 3xx を受けたときは、追わずにその応答が返る
 * (`ok` は false)。呼び出し側では「2xx 以外のステータス」として扱われる。
 */
export const accessFetchInit = (): AccessFetchInit => {
  const result = buildAccessHeaders(
    process.env.CF_ACCESS_CLIENT_ID,
    process.env.CF_ACCESS_CLIENT_SECRET,
  );
  if (!warned && (result.status === 'partial' || result.status === 'invalid')) {
    warned = true;
    console.warn(
      result.status === 'partial'
        ? 'accessFetchInit: only one of CF_ACCESS_CLIENT_ID / CF_ACCESS_CLIENT_SECRET is set; ' +
            'sending no Cloudflare Access headers'
        : 'accessFetchInit: CF_ACCESS_CLIENT_ID / CF_ACCESS_CLIENT_SECRET contain characters ' +
            'not allowed in a header value; sending no Cloudflare Access headers',
    );
  }
  return toAccessFetchInit(result);
};
