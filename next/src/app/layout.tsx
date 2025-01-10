
import type { Metadata } from "next";

import Html from '@/components/Html';

import "./globals.css";


export const metadata: Metadata = {
  title: "Cephylas Next.js",
  description: "Cephylas docker container log viewer",
  icons: {
    icon: '/cephylas/cephonodes-hylas.svg',
    shortcut: '/cephylas/cephonodes-hylas.svg',
    apple: '/cephylas/cephonodes-hylas.svg',
  }
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <Html>
      {children}
    </Html>
  );
}

