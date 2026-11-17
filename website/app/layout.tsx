import type { Metadata } from 'next';
import { Provider } from '@/components/provider';
import './global.css';

export const metadata: Metadata = {
  title: { default: 'Monio — Input, without limits', template: '%s | Monio' },
  description: 'Pure Rust cross-platform keyboard and mouse monitoring, interception, simulation, and replay.',
  metadataBase: new URL(process.env.DEPLOY_TARGET === 'github-pages'
    ? 'https://huakunshen.github.io/monio/'
    : 'https://monio-input.huakun-shen.chatgpt.site'),
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className="flex flex-col min-h-screen antialiased">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
