'use client'

import { clsx } from 'clsx';
import { useFilter } from './FilterContext';
import { borderColorFor } from '@/lib/colors';

export const Sidebar = ({
  containers,
}: {
  containers: readonly string[];
}) => {
  const { hidden, toggle, setAll, clear } = useFilter();

  return (
    <aside className='bg-base-200 min-h-full w-72 p-3 flex flex-col gap-2'>
      <div className='flex items-center justify-between px-1'>
        <h2 className='text-sm font-semibold'>Containers</h2>
        <span className='badge badge-sm badge-ghost'>
          {containers.length - hidden.size}/{containers.length}
        </span>
      </div>

      <div className='flex gap-2'>
        <button
          type='button'
          className='btn btn-xs btn-outline flex-1'
          onClick={() => clear()}
        >
          show all
        </button>
        <button
          type='button'
          className='btn btn-xs btn-outline flex-1'
          onClick={() => setAll(containers)}
        >
          hide all
        </button>
      </div>

      <ul className='flex flex-col'>
        {containers.map((name, i) => {
          const isHidden = hidden.has(name);
          const color = borderColorFor(i);
          return (
            <li key={name}>
              <label
                className={clsx(
                  'flex items-center gap-2 px-1 py-1 rounded-selector cursor-pointer',
                  'hover:bg-base-300/60',
                )}
              >
                <input
                  type='checkbox'
                  className='checkbox checkbox-xs'
                  style={{ color }}
                  checked={!isHidden}
                  onChange={() => toggle(name)}
                />
                <span
                  aria-hidden
                  className='inline-block w-3 h-3 rounded-selector shrink-0'
                  style={{ backgroundColor: color }}
                />
                <span
                  className={clsx(
                    'truncate text-sm',
                    isHidden && 'opacity-40 line-through',
                  )}
                  title={name}
                >
                  {name}
                </span>
              </label>
            </li>
          );
        })}
      </ul>
    </aside>
  );
};
