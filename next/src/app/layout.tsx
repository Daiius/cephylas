
import type { Metadata } from 'next'
import clsx from 'clsx'
import { ReactNode } from 'react'

import './globals.css'


export const metadata: Metadata = {
  title: 'Cephylas Next.js',
  description: 'Cephylas docker container log viewer',
  icons: {
    icon: '/cephonodes-hylas.svg',
    shortcut: '/cephonodes-hylas.svg',
    apple: '/cephonodes-hylas.svg',
  }
}

export default function RootLayout({
  children,
}: Readonly<{
  children: ReactNode,
}>) {
  return (
  <html lang='en' data-theme='light'>
    <body
      suppressHydrationWarning
      className={clsx(
        'antialiased',
        'min-h-screen bg-base-100',
      )}
    >
      {children}
    </body>
  </html>
  )
}

