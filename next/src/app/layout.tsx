
import type { Metadata } from "next";

import Html from '@/components/Html';

import "./globals.css";


export const metadata: Metadata = {
  title: "Cephylas Next.js",
  description: "Cephylas docker container log viewer",
  icons: {
    icon: '/cephonodes-hylas.svg',
    shortcut: '/cephonodes-hylas.svg',
    apple: '/cephonodes-hylas.svg',
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

