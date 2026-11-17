# Monio website

The interactive landing page and developer documentation for [Monio](https://github.com/HuakunShen/monio), a pure Rust cross-platform input hook and simulation library.

Built with Next.js, React, and Fumadocs. **The published output is fully static**: no Node.js server is required at runtime. Fumadocs search is precomputed at build time and runs in the browser.

## Develop

```bash
npm install
npm run dev
```

Open <http://127.0.0.1:3000>. The interactive keyboard is an SVG component. The lab observes browser-scoped pointer and keyboard input; it does **not** claim to access operating-system hooks from the browser.

## Build

```bash
npm run build
```

Deploy the generated `out/` directory to any static web host.

## Structure

- `components/monio-landing.tsx` — the landing narrative and interactive event playground
- `components/keyboard-visual.tsx` — custom SVG keyboard with active keycap states
- `app/global.css` — site visual system, responsive layout, and animation rules
- `content/docs/*.mdx` — developer documentation authored in MDX
- `app/docs` — Fumadocs navigation and page layout
- `app/api/search` — pre-rendered static search index

Documentation content is based on the [upstream Monio README](https://github.com/HuakunShen/monio#readme). Keep platform capability statements in sync when the library changes.
