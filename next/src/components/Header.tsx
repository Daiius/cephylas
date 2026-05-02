import Link from 'next/link';

export const Header = () => (
  <header className='navbar bg-base-200 min-h-12 sticky top-0 z-30 justify-center'>
    <Link href='/about' className='btn btn-ghost btn-circle'>
      <img
        src='/cephonodes-hylas.svg'
        width={36}
        height={36}
        alt='cephylas icon'
      />
    </Link>
  </header>
);
