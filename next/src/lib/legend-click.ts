// 凡例のチップへのクリックを、シングル / ダブル / 捨てる、に振り分ける判定。
//
// 純粋な関数だけを置く (時刻は引数で受け取り、タイマーも React も触らない)。
// タイマーの予約と取り消し、FilterContext の呼び出しは MiniLegend.tsx が行う。
// import を持たないので、node から直接読み込んで確かめられる。

/** ダブルの判定に使う間隔。シングルの反映もこの時間だけ遅れる。 */
export const DOUBLE_CLICK_THRESHOLD_MS = 250;

/** いつ・どのチップが押されたか。`at` は単調に増える時刻 (ミリ秒) */
export type LegendClickMark = { name: string; at: number };

export type LegendClickState = {
  /** toggle を予約中のシングル (まだ反映していない)。無ければ null */
  pending: LegendClickMark | null;
  /** 直近に処理したダブル (2 回目のクリックの時刻)。無ければ null */
  lastDouble: LegendClickMark | null;
};

export const initialLegendClickState: LegendClickState = {
  pending: null,
  lastDouble: null,
};

export type LegendClick = LegendClickMark & {
  /** 押されたチップだけが表示されている状態か (ダブルで全表示に戻すかの判定に使う) */
  isOnlyVisible: boolean;
};

/**
 * クリック 1 回に対して、呼び出し側が行うこと。
 *
 * - `schedule-toggle`: `name` の toggle を DOUBLE_CLICK_THRESHOLD_MS 後に予約する。
 *   `flush` が null でなければ、別のチップの予約が残っている。その予約を取り消し、
 *   `flush` の toggle を**先に、その場で**行う (もうダブルにはなりえないため。
 *   遅らせたままにすると、あとから押したチップの操作と順序が入れ替わる)
 * - `isolate`: 予約を取り消し、`name` だけを表示する
 * - `clear`: 予約を取り消し、全表示に戻す
 * - `discard`: 何もしない (ダブルの直後の、同じチップへの余分なクリック)
 */
export type LegendClickAction =
  | { type: 'schedule-toggle'; name: string; flush: string | null }
  | { type: 'isolate'; name: string }
  | { type: 'clear' }
  | { type: 'discard' };

const within = (mark: LegendClickMark | null, click: LegendClickMark): boolean =>
  mark !== null &&
  mark.name === click.name &&
  click.at - mark.at < DOUBLE_CLICK_THRESHOLD_MS;

/**
 * クリック 1 回を処理し、次の状態と、呼び出し側が行うことを返す。
 *
 * 1. 同じチップのダブルを処理してから閾値内 → 捨てる。3 連打や、キーの押しっぱなしで
 *    余分なクリックが新しいシングルになり、単独表示にしたチップ自身を隠すのを防ぐ。
 *    捨てても期間は延ばさない (ダブルの時刻から数える)
 * 2. 同じチップのシングルが予約中で閾値内 → ダブル。単独表示中なら全表示に戻し、
 *    そうでなければそのチップだけを表示する
 * 3. それ以外 → シングル。toggle を予約する
 */
export const reduceLegendClick = (
  state: LegendClickState,
  click: LegendClick,
): { state: LegendClickState; action: LegendClickAction } => {
  const mark: LegendClickMark = { name: click.name, at: click.at };

  if (within(state.lastDouble, mark)) {
    return { state, action: { type: 'discard' } };
  }

  if (within(state.pending, mark)) {
    return {
      state: { pending: null, lastDouble: mark },
      action: click.isOnlyVisible
        ? { type: 'clear' }
        : { type: 'isolate', name: click.name },
    };
  }

  // 予約が残っているのは、別のチップか、同じチップで閾値を過ぎたのにタイマーが
  // まだ走っていない場合である。どちらも、先に反映させてから新しい予約を入れる。
  return {
    state: { pending: mark, lastDouble: state.lastDouble },
    action: {
      type: 'schedule-toggle',
      name: click.name,
      flush: state.pending?.name ?? null,
    },
  };
};

/** 予約していた toggle を反映した (タイマーが走った) あとの状態。 */
export const settleLegendToggle = (state: LegendClickState): LegendClickState => ({
  pending: null,
  lastDouble: state.lastDouble,
});
