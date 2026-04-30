'use client'

import { clsx } from 'clsx';
import { useFilter } from './FilterContext';
import type { AppDataset } from './Chart';

/**
 * 各チャート上に表示する凡例。クリック/タップで表示/非表示を切替えられる。
 * Sidebar/drawer のチェックボックスと同じ FilterContext を共有しているので
 * どちらで操作しても同期する。
 *
 * IO/Net のように 1 コンテナで複数 dataset (read/write) ある場合は
 * 同名コンテナを 1 行に集約する (line style での違いはチャートで読む)。
 */
export const MiniLegend = ({ datasets }: { datasets: AppDataset[] }) => {
  const { hidden, toggle } = useFilter();

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

  if (uniq.length === 0) return null;

  return (
    <ul className='flex flex-wrap gap-x-1 gap-y-1 px-2 py-1 text-xs leading-none'>
      {uniq.map(({ name, color }) => {
        const isHidden = hidden.has(name);
        return (
          <li key={name}>
            <button
              type='button'
              onClick={() => toggle(name)}
              aria-pressed={!isHidden}
              className={clsx(
                'flex items-center gap-1.5 px-1.5 py-1 rounded-selector cursor-pointer',
                'hover:bg-base-300/60 active:bg-base-300',
                'transition-opacity',
                isHidden && 'opacity-40 line-through',
              )}
            >
              <span
                aria-hidden
                className='inline-block w-2.5 h-2.5 rounded-full shrink-0'
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
