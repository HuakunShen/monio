import { spawn } from 'node:child_process';

// Adapt the managed preview runner's generic Vite-style flags to Next.js.
// With no flags, this also serves as an ordinary `npm run dev` command.
function option(name, fallback) {
  const index = process.argv.indexOf(name);
  return index < 0 ? fallback : (process.argv[index + 1] ?? fallback);
}

const host = option('--host', option('--hostname', '127.0.0.1'));
const port = option('--port', '3000');
const child = spawn(process.execPath, ['node_modules/next/dist/bin/next', 'dev', '--hostname', host, '--port', port], {
  cwd: process.cwd(),
  stdio: 'inherit',
  env: process.env,
});

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => child.kill(signal));
}
child.on('exit', (code, signal) => process.exit(code ?? (signal ? 1 : 0)));
