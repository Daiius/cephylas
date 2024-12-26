//'use client'

import React from 'react';
import clsx from 'clsx';

import { ChevronRightIcon } from '@heroicons/react/24/solid';

import { 
  Disclosure as HeadlessDisclosure, 
  DisclosureButton, 
  DisclosurePanel,
} from '@headlessui/react';

const Disclosure: React.FC<
  React.ComponentProps<typeof HeadlessDisclosure>
  & { title?: string }
> = ({
  title,
  children,
  className,
  ...props
}) => {
  return (
    <HeadlessDisclosure
      as='div'
      className={clsx(className)}
      {...props}
    >
      <DisclosureButton className={clsx(
        'group flex flex-row items-center gap-1'
      )}>
        <ChevronRightIcon className={clsx(
          'size-4 group-data-[open]:rotate-90',
          'transition duration-200 ease-in-out'
        )}/>
        {title &&
          <div>{title}</div>
        }
      </DisclosureButton>
      <DisclosurePanel
        transition
        className={clsx(
          'transition duration-200 ease-in-out',
          'data-[closed]:opacity-0',
        )}
      >
        {children}
      </DisclosurePanel>
    </HeadlessDisclosure>
  );
}

export default Disclosure;

