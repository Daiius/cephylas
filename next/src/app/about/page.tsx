
import React from 'react';
import clsx from 'clsx';

export default async function About() {
  return (
    <div className={clsx(
      'p-4 min-h-screen bg-slate-200',
      'flex flex-col items-center', 
      'sm:flex sm:flex-row justify-center',
      'text-lg',
    )}>
      <img
        className={clsx(
          'animate-appear',
          'w-1/2 mb-2 sm:mb-0 sm:mr-2',
        )}
        src='/cephylas/cephonodes-hylas.svg'
        alt='cephylas icon'
      />
      <div className='flex flex-col items-center'>
        <div className='text-pretty text-center w-2/3 mb-2'>
          Simple docker container resource usage logger & visualizer.
        </div>
        <div className='text-pretty text-center w-2/3'>
          It comes from <span className='italic'>cephonodes hylas</span>,
          which is a very kawaii cute and lovely insect
          with transparent wings!
        </div>
      </div>
    </div>
  );
}

