import clsx from 'clsx';
import Link from 'next/link';

export default function About() {
  return (
    <div className={clsx(
      'relative p-4 min-h-screen bg-base-200',
      'flex flex-col items-center justify-center',
      'sm:flex-row',
      'text-lg leading-relaxed',
    )}>
      <Link
        href='/'
        className='btn btn-ghost btn-sm absolute top-3 left-3'
      >
        ← Charts
      </Link>
      <img
        className={clsx(
          'animate-appear',
          'w-1/2 max-w-xs mb-4 sm:mb-0 sm:mr-6',
        )}
        src='/cephonodes-hylas.svg'
        alt='cephylas icon'
      />
      <div className='flex flex-col items-center max-w-prose text-pretty text-center'>
        <p className='mb-2'>
          Simple docker container resource usage logger & visualizer.
        </p>
        <p>
          It comes from <span className='italic'>cephonodes hylas</span>,
          which is a very kawaii cute and lovely insect
          with transparent wings!
        </p>
      </div>
    </div>
  );
}

