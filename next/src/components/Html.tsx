
import React from 'react';
import clsx from 'clsx';

const Html: React.FC<
  React.ComponentProps<'body'>
> = ({
  children,
  className,
  ...props
}) => (
  <html 
    lang="en" 
  >
    <body
      suppressHydrationWarning
      className={clsx(
        'antialiased',
        'min-h-screen bg-slate-100',
        className,
      )}
      {...props}
    >
      {children}
    </body>
  </html>
);

export default Html;

