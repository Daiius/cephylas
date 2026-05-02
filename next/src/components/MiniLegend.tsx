'use client'

import { clsx } from 'clsx';
import { useCallback, useEffect, useRef } from 'react';
import { useFilter } from './FilterContext';
import type { AppDataset } from './Chart';

const DOUBLE_CLICK_THRESHOLD_MS = 250;

/**
 * 各チャート上に表示する凡例。クリック/タップで表示/非表示を切替えられる。
 * Sidebar/drawer のチェックボックスと同じ FilterContext を共有しているので
 * どちらで操作しても同期する。
 *
 * IO/Net のように 1 コンテナで複数 dataset (read/write) ある場合は
 * 同名コンテナを 1 行に集約する (line style での違いはチャートで読む)。
 *
 * ダブルクリック/タップで Grafana 風の isolate / restore:
 *   - 通常状態 → ダブル → そのチップだけ表示 (他全部 hidden)
 *   - 既に単独表示中 → ダブル → 全表示に戻す
 * シングルクリックの toggle は double 検知のため DOUBLE_CLICK_THRESHOLD_MS だけ遅延発火する。
 */
export const MiniLegend = ({ datasets }: { datasets: AppDataset[] }) => {
  const { hidden, toggle, isolate, clear } = useFilter();

  const seen = new Set<string>();
  const uniq: Array<{ name: string; color: string }> = [];
  for (const d of datasets) {
    const name = d.containerName;
    if (!name || seen.has(name)) continue;
    seen.add(name);
    uniq.push({
      name,
      color: typeof d.borderColor === 'string' ? d.borderColor : '#888',
    });
  }

  // 直近の単発クリック (まだ toggle が発火していないもの) を覚えておく。
  // 同じ name に閾値内で 2 回目が来たら double として扱い、
  // 違う name のクリックが来たり、閾値を超えれば pending を放置 (timer が toggle を発火) する。
  const pendingRef = useRef<{ name: string; t: number; timer: number } | null>(null);

  // unmount 時に pending をクリア
  useEffect(() => () => {
    if (pendingRef.current) window.clearTimeout(pendingRef.current.timer);
  }, []);

  const handleClick = useCallback(
    (name: string) => {
      const now = performance.now();
      const last = pendingRef.current;

      if (last && last.name === name && now - last.t < DOUBLE_CLICK_THRESHOLD_MS) {
        window.clearTimeout(last.timer);
        pendingRef.current = null;

        const allNames = uniq.map((u) => u.name);
        const visible = allNames.filter((n) => !hidden.has(n));
        const isOnlyThisVisible = visible.length === 1 && visible[0] === name;
        if (isOnlyThisVisible) clear();
        else isolate(name, allNames);
        return;
      }

      const timer = window.setTimeout(() => {
        toggle(name);
        if (pendingRef.current?.timer === timer) pendingRef.current = null;
      }, DOUBLE_CLICK_THRESHOLD_MS);
      pendingRef.current = { name, t: now, timer };
    },
    [uniq, hidden, toggle, isolate, clear],
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
