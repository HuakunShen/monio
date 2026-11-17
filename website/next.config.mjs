import { createMDX } from 'fumadocs-mdx/next';

const withMDX = createMDX();
const isGithubPages = process.env.DEPLOY_TARGET === 'github-pages';

/** @type {import('next').NextConfig} */
const config = {
  output: 'export',
  reactStrictMode: true,
  ...(isGithubPages ? { basePath: '/monio', trailingSlash: true } : {}),
};

export default withMDX(config);
