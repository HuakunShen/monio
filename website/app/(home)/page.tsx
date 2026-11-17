import type { Metadata } from 'next';
import { MonioLanding } from '@/components/monio-landing';

export const metadata: Metadata = {
  title: 'Monio — Input, without limits',
  description: 'A pure Rust, cross-platform input library. Listen, grab, inject, and replay keyboard and pointer events through one consistent API.',
};

export default function HomePage() {
  return <MonioLanding />;
}
