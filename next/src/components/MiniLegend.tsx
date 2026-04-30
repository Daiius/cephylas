'use client'

import { clsx } from 'clsx';
import { useFilter } from './FilterContext';
import type { AppDataset } from './Chart';

/**
 * 各チャート上に表示する read-only の凡例。
 * フィルタ操作はサイドバー/drawer 側で完結させ、ここは色対応の視認のみ。
 *
 * IO/Net のように 1 コンテナで複数 dataset (read/write) ある場合は
 * 同名コンテナを 1 行に集約する (line style で違いはチャートで読む)。
 */
export const MiniLegend = ({ datasets }: { datasets: AppDataset[] }) => {
  const { hidden } = useFilter();

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
    <ul className='flex flex-wrap gap-x-3 gap-y-1 px-2 py-1 text-xs leading-none'>
      {uniq.map(({ name, color }) => {
        const isHidden = hidden.has(name);
        return (
          <li
            key={name}
            className={clsx(
              'flex items-center gap-1.5',
              isHidden && 'opacity-30 line-through',
            )}
          >
            <span
              aria-hidden
              className='inline-block w-2.5 h-2.5 rounded-full shrink-0'
              style={{ backgroundColor: color }}
            />
            <span className='truncate max-w-[12rem]' title={name}>{name}</span>
          </li>
        );
      })}
    </ul>
  );
};
