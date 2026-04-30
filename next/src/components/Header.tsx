import Link from 'next/link';

const HamburgerIcon = () => (
  <svg
    xmlns='http://www.w3.org/2000/svg'
    fill='none'
    viewBox='0 0 24 24'
    strokeWidth={1.8}
    stroke='currentColor'
    className='w-5 h-5'
    aria-hidden
  >
    <path strokeLinecap='round' strokeLinejoin='round' d='M3.75 6.75h16.5M3.75 12h16.5M3.75 17.25h16.5' />
  </svg>
);

export const Header = () => (
  <header className='navbar bg-base-200 min-h-12 sticky top-0 z-30'>
    <div className='navbar-start'>
      <label
        htmlFor='cephylas-drawer'
        aria-label='open sidebar'
        className='btn btn-square btn-ghost btn-sm lg:hidden'
      >
        <HamburgerIcon />
      </label>
    </div>
    <div className='navbar-center'>
      <Link href='/about' className='btn btn-ghost btn-circle'>
        <img
          src='/cephonodes-hylas.svg'
          width={36}
          height={36}
          alt='cephylas icon'
        />
      </Link>
    </div>
    <div className='navbar-end' />
  </header>
);
