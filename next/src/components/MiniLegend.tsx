'use client'

import { clsx } from 'clsx';
import { useCallback, useEffect, useRef } from 'react';
import { useFilter } from './FilterContext';
import type { AppDataset } from './Chart';
import type { ColorOf } from './ContainerColorContext';
import {
  DOUBLE_CLICK_THRESHOLD_MS,
  initialLegendClickState,
  reduceLegendClick,
  settleLegendToggle,
  type LegendClickState,
} from '@/lib/legend-click';

/**
 * 各チャート上に表示する凡例。クリック/タップで表示/非表示を切替えられる。
 * 4 枚のチャートの凡例が同じ FilterContext を共有しているので、
 * どのチャートで操作しても全チャートに反映される。
 *
 * IO/Net のように 1 コンテナで複数 dataset (read/write) ある場合は
 * 同名コンテナを 1 行に集約する (line style での違いはチャートで読む)。
 *
 * ダブルクリック/タップで Grafana 風の isolate / restore:
 *   - 通常状態 → ダブル → そのチップだけ表示 (他全部 hidden)
 *   - 既に単独表示中 → ダブル → 全表示に戻す
 * シングルクリックの toggle は double 検知のため DOUBLE_CLICK_THRESHOLD_MS だけ遅延発火する。
 * ダブルの直後 (同じ間隔) の同じチップへのクリックは捨てる (3 連打で、単独表示にした
 * チップ自身を隠さないため)。判定は lib/legend-click.ts の純粋な関数にある。
 */
export const MiniLegend = ({
  datasets,
  colorOf,
}: {
  datasets: AppDataset[];
  /** Chart が線に当てているのと同じ関数 (ContainerColorContext) */
  colorOf: ColorOf;
}) => {
  const { hidden, toggle, isolate, clear } = useFilter();

  const seen = new Set<string>();
  const uniq: Array<{ name: string; color: string }> = [];
  for (const d of datasets) {
    const name = d.containerName;
    if (!name || seen.has(name)) continue;
    seen.add(name);
    uniq.push({ name, color: colorOf(name).border });
  }

  // クリックの判定の状態 (予約中のシングルと、直近のダブル) と、予約中の toggle のタイマー。
  // 描画には使わないので ref で持つ。タイマーは同時に 1 本だけである。
  const clickStateRef = useRef<LegendClickState>(initialLegendClickState);
  const timerRef = useRef<number | null>(null);

  const cancelTimer = useCallback(() => {
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = null;
  }, []);

  // unmount 時に予約を取り消す
  useEffect(() => cancelTimer, [cancelTimer]);

  const handleClick = useCallback(
    (name: string) => {
      const allNames = uniq.map((u) => u.name);
      const visible = allNames.filter((n) => !hidden.has(n));
      const { state, action } = reduceLegendClick(clickStateRef.current, {
        name,
        at: performance.now(),
        isOnlyVisible: visible.length === 1 && visible[0] === name,
      });
      clickStateRef.current = state;

      switch (action.type) {
        case 'discard':
          return;
        case 'isolate':
          cancelTimer();
          isolate(action.name, allNames);
          return;
        case 'clear':
          cancelTimer();
          clear();
          return;
        case 'schedule-toggle':
          cancelTimer();
          if (action.flush !== null) toggle(action.flush);
          timerRef.current = window.setTimeout(() => {
            timerRef.current = null;
            clickStateRef.current = settleLegendToggle(clickStateRef.current);
            toggle(action.name);
          }, DOUBLE_CLICK_THRESHOLD_MS);
          return;
      }
    },
    [uniq, hidden, toggle, isolate, clear, cancelTimer],
  );

  if (uniq.length === 0) return null;

  return (
    <ul className='flex flex-wrap gap-1 px-2 py-1 text-sm leading-none'>
      {uniq.map(({ name, color }) => {
        const isHidden = hidden.has(name);
        return (
          <li key={name}>
            <button
              type='button'
              onClick={() => handleClick(name)}
              // Enter の押しっぱなし (キーリピート) をクリックの連打として扱わない。
              onKeyDown={(e) => {
                if (e.repeat && e.key === 'Enter') e.preventDefault();
              }}
              aria-pressed={!isHidden}
              className={clsx(
                'flex items-center gap-2 px-2.5 py-1.5 rounded-selector cursor-pointer touch-manipulation',
                'border border-transparent',
                'hover:bg-base-300/60 active:bg-base-300',
                'transition-opacity',
                isHidden && 'opacity-40 line-through',
              )}
            >
              <span
                aria-hidden
                className='inline-block w-3 h-3 rounded-full shrink-0'
                style={{ backgroundColor: color }}
              />
              <span className='truncate max-w-[12rem]' title={name}>{name}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
};
