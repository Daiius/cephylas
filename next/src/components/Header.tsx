import { clsx }  from 'clsx';

import Link from 'next/link';

export const Header = ({
  className,
}: {
  className?: string;
}) => (
  <div 
    className={clsx(
      'h-12', 
      'bg-slate-200', 
      'flex flex-row items-center justify-center',
      className,
    )}
  >
    
    <Link
      href='/about'
    >
      <img 
        src='/cephonodes-hylas.svg' 
        width={36} 
        height={36}
        alt='cephylas icon'
      />
    </Link>
  </div>
);

