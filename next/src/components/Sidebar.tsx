'use client'

import { clsx } from 'clsx';
import { useFilter } from './FilterContext';
import { borderColorFor } from '@/lib/colors';

export const Sidebar = ({
  containers,
  className,
}: {
  containers: readonly string[];
  className?: string;
}) => {
  const { hidden, toggle, setAll, clear } = useFilter();

  return (
    <aside
      className={clsx(
        'shrink-0 bg-slate-200',
        // mobile: full width above the charts
        'w-full',
        // desktop: fixed width sidebar, sticky inside scroll container
        'md:w-60 md:sticky md:top-0 md:self-start md:max-h-screen md:overflow-y-auto',
        'border-b border-slate-300 md:border-b-0 md:border-r',
        className,
      )}
    >
      <details className='md:open:!block group' open>
        <summary
          className={clsx(
            'cursor-pointer select-none px-3 py-2 text-sm font-medium',
            'flex items-center justify-between',
            'md:cursor-default md:list-none',
          )}
        >
          <span>
            Containers
            <span className='ml-2 text-xs text-slate-500'>
              {containers.length - hidden.size}/{containers.length}
            </span>
          </span>
          <span className='md:hidden text-xs text-slate-500 group-open:hidden'>
            tap to expand
          </span>
        </summary>

        <div className='px-2 pb-2'>
          <div className='flex gap-1 mb-2 text-xs'>
            <button
              type='button'
              onClick={() => clear()}
              className={clsx(
                'flex-1 rounded border border-slate-300 bg-white py-1',
                'hover:bg-slate-50 active:bg-slate-100',
              )}
            >
              show all
            </button>
            <button
              type='button'
              onClick={() => setAll(containers)}
              className={clsx(
                'flex-1 rounded border border-slate-300 bg-white py-1',
                'hover:bg-slate-50 active:bg-slate-100',
              )}
            >
              hide all
            </button>
          </div>

          <ul className='flex flex-col'>
            {containers.map((name) => {
              const isHidden = hidden.has(name);
              return (
                <li key={name}>
                  <label
                    className={clsx(
                      'flex items-center gap-2 px-1 py-1 rounded cursor-pointer',
                      'hover:bg-slate-100',
                      'text-sm',
                    )}
                  >
                    <input
                      type='checkbox'
                      className='shrink-0'
                      checked={!isHidden}
                      onChange={() => toggle(name)}
                    />
                    <span
                      aria-hidden
                      className='inline-block w-3 h-3 rounded-sm shrink-0'
                      style={{ backgroundColor: borderColorFor(name) }}
                    />
                    <span
                      className={clsx(
                        'truncate',
                        isHidden && 'text-slate-400 line-through',
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
        </div>
      </details>
    </aside>
  );
};
