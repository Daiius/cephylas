import React from 'react';
import clsx from 'clsx';

import Link from 'next/link';

const Header: React.FC<
  React.ComponentProps<'div'>
> = ({
  className,
  ...props
}) => (
  <div 
    className={clsx(
      'h-12', 
      'bg-slate-200', 
      'flex flex-row items-center justify-center',
      className,
    )}
    {...props}
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

export default Header;

