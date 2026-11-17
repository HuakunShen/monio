'use client';

import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react';
import { ArrowRight, ArrowUpRight, Check, CircleDot, Command, Copy, Keyboard, Layers3, MousePointer2, Play, Radio, RotateCcw, ShieldCheck, Zap } from 'lucide-react';
import { KeyboardVisual } from './keyboard-visual';

type InputEvent = { id: number; type: string; detail: string; source: string };
type Point = { x: number; y: number };
const REPO = 'https://github.com/HuakunShen/monio';
const CRATE = 'https://crates.io/crates/monio';
const NAPI_REPO = 'https://github.com/HuakunShen/monio-napi';
const NAPI_NPM = 'https://www.npmjs.com/package/monio-napi';
const BASE_PATH = process.env.NEXT_PUBLIC_BASE_PATH ?? '';
const DOCS = `${BASE_PATH}/docs/`;
const PLATFORM_DOCS = `${BASE_PATH}/docs/platforms/`;
const INSTALL = 'monio = "0.2"';

function Mark({ size = 31 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 36 36" fill="none" aria-hidden="true">
      <rect x=".7" y=".7" width="34.6" height="34.6" rx="10" fill="#15251E" stroke="#517E61" strokeWidth="1.4" />
      <path d="M8.5 25V11L18 20.4L27.5 11V25" stroke="#C5FFD7" strokeWidth="3.3" strokeLinecap="round" strokeLinejoin="round" />
      <circle cx="18" cy="20.4" r="3.15" fill="#76DDA3" />
    </svg>
  );
}

function GitHubIcon({ size = 17 }: { size?: number }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 .8a11.2 11.2 0 0 0-3.54 21.82c.56.1.76-.25.76-.54v-2.08c-3.1.68-3.75-1.32-3.75-1.32-.5-1.3-1.25-1.65-1.25-1.65-1.02-.7.08-.69.08-.69 1.13.08 1.73 1.16 1.73 1.16 1 .72 2.63.51 3.27.39.1-.73.4-1.23.72-1.51-2.48-.28-5.08-1.24-5.08-5.53 0-1.22.44-2.22 1.15-3-.12-.28-.5-1.42.11-2.96 0 0 .94-.3 3.08 1.14A10.7 10.7 0 0 1 12 5.65c.95 0 1.9.13 2.8.38 2.14-1.45 3.07-1.14 3.07-1.14.61 1.54.23 2.68.12 2.96.72.78 1.14 1.78 1.14 3 0 4.3-2.6 5.25-5.09 5.52.4.35.77 1.04.77 2.09v3.62c0 .3.2.65.77.54A11.2 11.2 0 0 0 12 .8Z"/></svg>;
}

const initialEvents: InputEvent[] = [
  { id: 1, type: 'HookEnabled', detail: 'listener ready', source: 'PREVIEW' },
  { id: 2, type: 'MouseMoved', detail: 'x: 284, y: 164', source: 'PREVIEW' },
  { id: 3, type: 'KeyPressed', detail: 'KeyM', source: 'PREVIEW' },
];

export function MonioLanding() {
  const [events, setEvents] = useState(initialEvents);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [cursor, setCursor] = useState<Point>({ x: 48, y: 49 });
  const [trail, setTrail] = useState<Point[]>([{ x: 24, y: 64 }, { x: 31, y: 62 }, { x: 39, y: 55 }, { x: 48, y: 49 }]);
  const [dragging, setDragging] = useState(false);
  const [isDemo, setIsDemo] = useState(true);
  const [copied, setCopied] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const seqRef = useRef<ReturnType<typeof setTimeout>[]>([]);
  const pressedRef = useRef(false);
  const lastMouseLog = useRef(0);
  const idRef = useRef(4);
  const playgroundRef = useRef<HTMLDivElement>(null);

  const addEvent = useCallback((type: string, detail: string, source = 'LIVE') => {
    const id = idRef.current++;
    setEvents((old) => [...old.slice(-5), { id, type, detail, source }]);
  }, []);

  const triggerKey = useCallback((code: string, label = code) => {
    setIsDemo(false);
    setActiveKey(code);
    addEvent('KeyPressed', label || 'Space');
    seqRef.current.push(setTimeout(() => setActiveKey(null), 340));
  }, [addEvent]);

  useEffect(() => {
    if (!isDemo) return;
    const keyLoop = ['KeyM', null, 'KeyO', null, 'KeyN', null, 'KeyI', null, 'KeyO', null];
    let i = 0;
    const timer = setInterval(() => {
      setActiveKey(keyLoop[i % keyLoop.length]);
      i++;
    }, 640);
    return () => clearInterval(timer);
  }, [isDemo]);

  useEffect(() => () => seqRef.current.forEach(clearTimeout), []);

  const runSequence = (name: 'drag' | 'keyboard' | 'reset') => {
    seqRef.current.forEach(clearTimeout);
    seqRef.current = [];
    setIsDemo(false);
    if (name === 'reset') {
      setEvents([{ id: idRef.current++, type: 'HookEnabled', detail: 'listener ready', source: 'PREVIEW' }]);
      setActiveKey(null);
      setDragging(false);
      setTrail([]);
      return;
    }
    if (name === 'keyboard') {
      const keys: [string, string][] = [['KeyM', 'KeyM'], ['KeyO', 'KeyO'], ['KeyN', 'KeyN'], ['KeyI', 'KeyI'], ['KeyO', 'KeyO']];
      keys.forEach(([code, detail], i) => {
        seqRef.current.push(setTimeout(() => {
          setActiveKey(code);
          addEvent('KeyPressed', detail, 'DEMO');
          seqRef.current.push(setTimeout(() => setActiveKey(null), 180));
        }, i * 320));
      });
    } else {
      const points: Point[] = [{ x: 18, y: 58 }, { x: 29, y: 53 }, { x: 41, y: 44 }, { x: 52, y: 38 }, { x: 66, y: 29 }, { x: 78, y: 24 }];
      addEvent('MousePressed', 'button: Left', 'DEMO');
      setDragging(true);
      setTrail([points[0]]);
      points.forEach((point, i) => seqRef.current.push(setTimeout(() => {
        setCursor(point);
        setTrail((old) => [...old, point]);
        addEvent('MouseDragged', `x: ${Math.round(point.x * 5)}, y: ${Math.round(point.y * 3)}`, 'DEMO');
      }, (i + 1) * 180)));
      seqRef.current.push(setTimeout(() => {
        setDragging(false);
        addEvent('MouseReleased', 'button: Left', 'DEMO');
      }, (points.length + 1) * 180));
    }
  };

  const trackPointer = (event: PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const point = {
      x: Math.min(96, Math.max(4, ((event.clientX - rect.left) / rect.width) * 100)),
      y: Math.min(91, Math.max(9, ((event.clientY - rect.top) / rect.height) * 100)),
    };
    setCursor(point);
    setTrail((old) => [...old.slice(-24), point]);
    setIsDemo(false);
    const now = Date.now();
    if (now - lastMouseLog.current > (pressedRef.current ? 105 : 340)) {
      addEvent(pressedRef.current ? 'MouseDragged' : 'MouseMoved', `x: ${Math.round(point.x * 5)}, y: ${Math.round(point.y * 3)}`);
      lastMouseLog.current = now;
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.repeat || event.key === 'Tab') return;
    if (event.key === ' ' || event.key.startsWith('Arrow')) event.preventDefault();
    setIsDemo(false);
    setActiveKey(event.code);
    addEvent('KeyPressed', event.code);
  };

  const copyInstall = async () => {
    try {
      await navigator.clipboard.writeText(INSTALL);
      setCopied(true);
      setTimeout(() => setCopied(false), 1700);
    } catch {
      setCopied(false);
    }
  };

  const points = trail.map((point) => `${point.x * 5.6},${point.y * 2.5}`).join(' ');

  return (
    <main className="monio-site">
      <div className="top-glow" aria-hidden="true" />
      <header className="site-nav">
        <div className="nav-inner page-container">
          <a className="brand" href="#top" aria-label="Monio home"><Mark /><span>monio<span className="brand-dot">.</span></span></a>
          <nav className={`nav-links ${menuOpen ? 'open' : ''}`} aria-label="Main navigation">
            <a href="#playground" onClick={() => setMenuOpen(false)}>Playground</a>
            <a href="#capabilities" onClick={() => setMenuOpen(false)}>Capabilities</a>
            <a href="#platforms" onClick={() => setMenuOpen(false)}>Platforms</a>
            <a href={DOCS} onClick={() => setMenuOpen(false)}>Documentation</a>
          </nav>
          <div className="nav-actions">
            <a className="nav-github" href={REPO} target="_blank" rel="noopener noreferrer"><GitHubIcon size={17} /><span>GitHub</span><ArrowUpRight size={13}/></a>
            <a className="nav-get-started" href={DOCS}>Get started <ArrowUpRight size={15}/></a>
          </div>
          <button className="mobile-menu-button" type="button" onClick={() => setMenuOpen((old) => !old)} aria-expanded={menuOpen} aria-label="Toggle menu"><span/><span/><span/></button>
        </div>
      </header>

      <section className="hero page-container" id="top">
        <div className="hero-copy">
          <div className="hero-eyebrow"><span className="status-light"/> OPEN SOURCE INPUT INFRASTRUCTURE <span className="eyebrow-rule"/><span>v0.2.0</span></div>
          <h1>Every input.<br/><span className="gradient-words">One clean interface.</span></h1>
          <p className="hero-subtitle">Listen. Intercept. Simulate. Replay. A native, cross-platform input layer built in pure Rust—down to the difference between a mouse move and a drag.</p>
          <div className="hero-buttons">
            <a className="primary-cta" href="#playground">Try the playground <ArrowRight size={18}/></a>
            <a className="secondary-cta" href={DOCS}>Read the docs <ArrowUpRight size={16}/></a>
          </div>
          <div className="hero-proof"><span><span className="proof-icon">✳</span> Pure Rust</span><span><CircleDot size={12}/> Native hooks</span><span><Layers3 size={13}/> One API</span><a href={CRATE} target="_blank" rel="noopener noreferrer">crates.io <ArrowUpRight size={12}/></a><a href={NAPI_REPO} target="_blank" rel="noopener noreferrer">Node.js bindings <ArrowUpRight size={12}/></a></div>
        </div>
        <div className="hero-art" aria-hidden="true">
          <div className="hero-art-grid"/>
          <div className="orbit orbit-one"/><div className="orbit orbit-two"/><div className="orbit orbit-three"/>
          <div className="hero-crosshair crosshair-a">+</div><div className="hero-crosshair crosshair-b">+</div>
          <svg className="hero-curve" viewBox="0 0 600 480" preserveAspectRatio="none"><path d="M-10 340 C150 338 162 150 302 170 C410 187 416 64 610 66" fill="none" stroke="#8EF2B7" strokeWidth="1.3" strokeDasharray="4 9" opacity=".52"/><circle className="hero-curve-dot" cx="302" cy="170" r="5" fill="#B6FFD0"/></svg>
          <div className="hero-floating-command"><Command size={17}/><span>global input</span><span className="floating-led"/></div>
          <div className="hero-floating-event"><span className="tiny-label">EVENT / 004</span><strong>MouseDragged</strong><code>{'{ x: 384, y: 216 }'}</code></div>
          <div className="hero-mouse-assembly"><div className="hero-mouse-halo"/><svg viewBox="0 0 240 280" fill="none" className="hero-mouse-svg"><defs><linearGradient id="mouseShell" x1="20" y1="10" x2="200" y2="280"><stop stopColor="#526B65"/><stop offset=".32" stopColor="#263835"/><stop offset="1" stopColor="#0A1716"/></linearGradient><linearGradient id="mouseLeft" x1="0" y1="0" x2="1" y2="1"><stop stopColor="#7CF3B0" stopOpacity=".32"/><stop offset="1" stopColor="#7CF3B0" stopOpacity="0"/></linearGradient></defs><path d="M120 17 C54 17 31 72 31 129V174C31 240 62 270 120 270C177 270 209 241 209 174V129C209 72 186 17 120 17Z" fill="url(#mouseShell)" stroke="#7B9C8D" strokeWidth="1.4"/><path d="M120 18V118" stroke="#78998B" strokeWidth="1.3"/><path d="M31 118H209" stroke="#718D82" strokeWidth="1.2"/><path d="M120 18C64 18 35 68 31 118H120V18Z" fill="url(#mouseLeft)"/><rect x="114" y="66" width="12" height="40" rx="6" fill="#071410" stroke="#9EF4BB" strokeOpacity=".8"/><rect className="mouse-scroll-glow" x="117" y="74" width="6" height="17" rx="3" fill="#ACFFD0"/><path d="M67 235C80 251 97 258 120 258" stroke="#9AB3A5" strokeOpacity=".25" strokeLinecap="round"/></svg></div>
          <div className="hero-dimension dim-x">ΔX <span>+12.80</span></div><div className="hero-dimension dim-y">ΔY <span>−04.26</span></div>
        </div>
        <a className="hero-scroll" href="#playground"><span className="scroll-line"/> SCROLL TO EXPLORE <span>↓</span></a>
      </section>

      <div className="ticker-bar" aria-hidden="true"><div className="ticker-track">{Array.from({ length: 3 }, (_, i) => <span key={i}>KEYBOARD HOOKS <b>✳</b> POINTER CAPTURE <b>✳</b> EVENT GRABBING <b>✳</b> INPUT SIMULATION <b>✳</b> NATIVE PERFORMANCE <b>✳</b> DRAG DETECTION <b>✳</b></span>)}</div></div>

      <section className="playground-section section-padding" id="playground">
        <div className="page-container">
          <div className="section-intro two-col-heading">
            <div><div className="section-kicker"><span>01 /</span> INTERACTION LAB</div><h2>Don’t just read about input.<br/><em>Feel it.</em></h2></div>
            <p>This little lab runs in your browser. Move the pointer, hold and drag, or focus the panel and type. Watch how each action becomes a meaningful event.</p>
          </div>
          <div className="lab-frame">
            <div className="lab-toolbar"><div className="window-controls"><i/><i/><i/></div><div className="lab-title"><span className="lab-title-dot"/> monio / interactive lab</div><span className="lab-runtime">BROWSER-SCOPED DEMO</span></div>
            <div className="lab-content">
              <div className="lab-left">
                <div className="lab-keyboard-head"><span><Keyboard size={15}/> KEYBOARD INPUT</span><span>PRESS ANY KEY</span></div>
                <div className="keyboard-perspective"><KeyboardVisual pressedKey={activeKey} onKeyClick={triggerKey}/></div>
                <div className="lab-pointer-head"><span><MousePointer2 size={15}/> POINTER TRACKING</span><span>DRAG TO TEST</span></div>
                <div className={`pointer-field ${dragging ? 'pointer-dragging' : ''}`} ref={playgroundRef} tabIndex={0} role="application" aria-label="Interactive pointer and keyboard demo, click to focus and then type or drag" onKeyDown={onKeyDown} onKeyUp={() => setActiveKey(null)} onPointerMove={trackPointer} onPointerDown={(event) => { event.currentTarget.focus(); event.currentTarget.setPointerCapture(event.pointerId); pressedRef.current = true; setDragging(true); setIsDemo(false); setTrail([]); addEvent('MousePressed', 'button: Left'); }} onPointerUp={() => { if (pressedRef.current) addEvent('MouseReleased', 'button: Left'); pressedRef.current = false; setDragging(false); }} onPointerCancel={() => { pressedRef.current = false; setDragging(false); }}>
                  <div className="pointer-grid"/>
                  <div className="pointer-center-guide"><span>CAPTURE ZONE</span></div>
                  <svg className="pointer-trail" viewBox="0 0 560 250" preserveAspectRatio="none" aria-hidden="true"><polyline points={points} fill="none" stroke={dragging ? '#B6FFCE' : '#66B79F'} strokeWidth={dragging ? '2.7' : '1.6'} strokeLinecap="round" strokeLinejoin="round" opacity=".75"/><polyline points={points} fill="none" stroke="#74DCAC" strokeWidth="9" strokeLinecap="round" opacity=".05"/></svg>
                  <div className={`virtual-cursor ${dragging ? 'is-dragging' : ''}`} style={{ left: `${cursor.x}%`, top: `${cursor.y}%` }}><MousePointer2 size={24} fill={dragging ? '#C6FFDA' : '#96EAC0'}/><span>{dragging ? 'dragging' : 'pointer'}</span></div>
                  <div className="pointer-coordinates">X <strong>{Math.round(cursor.x * 5)}</strong><span>/</span> Y <strong>{Math.round(cursor.y * 3)}</strong></div>
                </div>
                <div className="lab-hints"><span><span className="hint-pulse"/> LIVE CAPTURE</span><span>Click the field to focus · Keyboard input works there</span></div>
              </div>
              <div className="lab-right">
                <div className="event-title"><span><Radio size={15}/> EVENT STREAM</span><span className="event-counter">{events.length.toString().padStart(2,'0')} EVENTS</span></div>
                <div className="event-log" role="log" aria-live="polite" aria-relevant="additions">
                  {events.map((event, i) => <div className="event-row" key={event.id}><span className="event-number">{String(event.id).padStart(3, '0')}</span><div><strong className={event.type === 'MouseDragged' ? 'drag-event' : ''}>{event.type}</strong><small>{event.detail}</small></div><span className={`event-source ${event.source === 'LIVE' ? 'source-live' : ''}`}>{event.source}</span><span className={`event-pip ${i === events.length - 1 ? 'latest' : ''}`}/></div>)}
                  {events.length <= 3 && <div className="signal-scope" aria-hidden="true"><div className="scope-heading"><span>SIGNAL PREVIEW</span><span>AWAITING INPUT</span></div><div className="scope-bars">{[12,24,16,36,22,50,34,15,42,52,26,38,18,45,23,32,14,27,41,18,30,13].map((height,i)=><i key={i} style={{height:`${height}px`,animationDelay:`${i*75}ms`}}/>)}</div><div className="scope-baseline"><span>◂ INPUT</span><span>READY ▸</span></div></div>}
                </div>
                <div className="event-actions"><span>PLAY A SEQUENCE</span><div><button type="button" onClick={() => runSequence('drag')}><Play size={12} fill="currentColor"/> Drag</button><button type="button" onClick={() => runSequence('keyboard')}><Play size={12} fill="currentColor"/> Keys</button><button type="button" onClick={() => runSequence('reset')} aria-label="Clear event log"><RotateCcw size={14}/></button></div></div>
                <div className="event-note"><CircleDot size={13}/><span>The lab demonstrates event semantics with DOM input. The Rust crate captures operating-system events in native applications.</span></div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section className="statement-section"><div className="page-container statement-inner"><span className="statement-star">✳</span><p>Most libraries see movement.<br/><span>Monio understands the action.</span></p><span className="statement-detail">MOVED ≠ DRAGGED<br/>IT MAKES A DIFFERENCE.</span></div></section>

      <section className="capabilities-section section-padding" id="capabilities"><div className="page-container">
        <div className="section-intro"><div className="section-kicker"><span>02 /</span> THE TOOLKIT</div><h2>Low-level control.<br/><em>High-level clarity.</em></h2><p>One consistent interface for the things desktop applications need to observe, capture, and produce input.</p></div>
        <div className="feature-grid">
          <article className="feature-card feature-large"><div className="feature-top"><span className="feature-index">01 / SIGNAL INTELLIGENCE</span><MousePointer2 size={18}/></div><div className="drag-viz"><div className="drag-grid"/><svg viewBox="0 0 460 175" aria-hidden="true"><path d="M36 132C90 129 107 32 194 59C275 88 293 37 405 42" fill="none" stroke="#344840" strokeWidth="14" strokeLinecap="round"/><path d="M36 132C90 129 107 32 194 59C275 88 293 37 405 42" fill="none" stroke="#ACFFCA" strokeWidth="2.5" strokeLinecap="round" strokeDasharray="3 8"/><circle cx="36" cy="132" r="7" fill="#A4F7BE"/><circle className="drag-viz-pulse" cx="194" cy="59" r="10" fill="#9BF6BC" opacity=".4"/><circle cx="405" cy="42" r="8" fill="#102119" stroke="#A4F7BE" strokeWidth="2"/></svg><span className="drag-viz-tag tag-start">PRESS</span><span className="drag-viz-tag tag-end">RELEASE</span><span className="drag-viz-center">MouseDragged</span></div><h3>A drag is not a move.</h3><p>Monio tracks button state so movement with a held button becomes <code>MouseDragged</code>—not another ambiguous mouse event.</p></article>
          <article className="feature-card feature-tall"><div className="feature-top"><span className="feature-index">02 / EVENT CONTROL</span><ShieldCheck size={18}/></div><div className="grab-viz"><div className="grab-route"><div><span>⌨</span><small>INPUT</small></div><span className="route-line"/><div className="route-engine"><Mark size={40}/><small>MONIO</small></div><span className="route-line blocked"/><div><span>▣</span><small>APP</small></div></div><div className="grab-filter">KEY F1 <span>CONSUMED</span><i/></div></div><h3>Listen—or take control.</h3><p>Observe global input or intercept it with <code>grab()</code>. Pass events through or consume them before other apps receive them.</p></article>
          <article className="feature-card"><div className="feature-top"><span className="feature-index">03 / INJECTION</span><Zap size={18}/></div><div className="mini-keys"><span>⌘</span><span className="mini-key-active">K</span><span>↵</span><i/></div><h3>Generate input.</h3><p>Simulate key presses, clicks, and absolute or relative pointer motion with native platform APIs.</p></article>
          <article className="feature-card"><div className="feature-top"><span className="feature-index">04 / TIME TRAVEL</span><RotateCcw size={18}/></div><div className="recorder-viz"><div className="recorder-play"><Play size={12} fill="currentColor"/></div>{[14,23,39,17,46,31,51,22,43,25,18,37,52,28,19,35,45,18,31,23].map((h,i) => <span key={i} style={{height:`${h}px`}}/>)}<b>01:24</b></div><h3>Record and replay.</h3><p>Capture macros, save them as JSON, and play events back with original or adjusted timing.</p></article>
          <article className="feature-card"><div className="feature-top"><span className="feature-index">05 / ASYNCHRONOUS</span><Layers3 size={18}/></div><div className="channel-viz"><span>Hook</span><i/><span>Channel</span><i/><span>Tokio</span></div><h3>Built for concurrency.</h3><p>Use callbacks, bounded standard channels, or Tokio streams so input processing fits your architecture.</p></article>
          <article className="feature-card"><div className="feature-top"><span className="feature-index">06 / PROVENANCE</span><CircleDot size={18}/></div><div className="origin-viz"><span className="origin-icon">◎</span><div><strong>ThisMonioSession</strong><small>SELF-INJECTED EVENT</small></div><span className="origin-check">✓</span></div><h3>Know what you injected.</h3><p>Session-specific provenance helps prevent remote-control feedback loops without pretending unknown input is physical.</p></article>
        </div>
      </div></section>

      <section className="code-section section-padding" id="code"><div className="page-container code-layout"><div className="code-copy"><div className="section-kicker"><span>03 /</span> DEVELOPER EXPERIENCE</div><h2>Native power.<br/><em>A Rust-sized API.</em></h2><p>No framework lock-in. No daemon required. Start with a listener, switch to interception, or inject events using a small, expressive surface.</p><div className="install-command"><span>cargo.toml</span><code>{INSTALL}</code><button type="button" onClick={copyInstall} aria-label="Copy installation snippet">{copied ? <Check size={17}/> : <Copy size={17}/>}</button></div><div className="code-links"><a href={DOCS}>Explore documentation <ArrowUpRight size={16}/></a><a href={CRATE} target="_blank" rel="noopener noreferrer">View on crates.io <ArrowUpRight size={16}/></a></div></div><div className="code-window"><div className="code-window-head"><div className="window-controls"><i/><i/><i/></div><span>src/main.rs</span><span>RUST</span></div><div className="code-pre"><div className="code-line"><span>01</span><code><i>use</i> <b>monio</b>::{'{'}listen, EventType{'}'};</code></div><div className="code-line"><span>02</span><code/></div><div className="code-line"><span>03</span><code><i>fn</i> <strong>main</strong>() -&gt; monio::Result&lt;()&gt; {'{'}</code></div><div className="code-line"><span>04</span><code>  <strong>listen</strong>(|event| {'{'}</code></div><div className="code-line"><span>05</span><code>    <i>match</i> event.event_type {'{'}</code></div><div className="code-line highlight-line"><span>06</span><code>      EventType::<em>MouseDragged</em> =&gt; {'{'}</code></div><div className="code-line"><span>07</span><code>        <strong>println!</strong>(<s>&quot;drag detected&quot;</s>);</code></div><div className="code-line"><span>08</span><code>      {'}'}</code></div><div className="code-line"><span>09</span><code>      EventType::<em>KeyPressed</em> =&gt; {'{'}</code></div><div className="code-line"><span>10</span><code>        <strong>println!</strong>(<s>&quot;key pressed&quot;</s>);</code></div><div className="code-line"><span>11</span><code>      {'}'}</code></div><div className="code-line"><span>12</span><code>      _ =&gt; {'{}'}</code></div><div className="code-line"><span>13</span><code>    {'}'}</code></div><div className="code-line"><span>14</span><code>  {'}'})</code></div><div className="code-line"><span>15</span><code>{'}'}</code></div></div><div className="code-window-footer"><span><span className="tiny-green-dot"/> READY TO COMPILE</span><span>monio 0.2</span></div></div></div></section>

      <section className="ecosystem-section section-padding" id="ecosystem"><div className="page-container"><div className="section-intro two-col-heading"><div><div className="section-kicker"><span>04 /</span> THE ECOSYSTEM</div><h2>One input engine.<br/><em>More ways to build.</em></h2></div><p>Use Monio directly in native Rust projects, or bring its input events into Node.js and Electron through the companion N-API binding.</p></div><div className="ecosystem-grid"><a className="ecosystem-card" href={CRATE} target="_blank" rel="noopener noreferrer"><div className="ecosystem-card-head"><span className="ecosystem-monogram">rs</span><span className="ecosystem-kind">CORE LIBRARY / RUST</span><ArrowUpRight size={20}/></div><h3>monio</h3><p>Native keyboard and mouse monitoring, interception, simulation and recording. Build directly on the Rust crate.</p><div className="ecosystem-install">cargo add monio</div><div className="ecosystem-card-link">Explore on crates.io <ArrowUpRight size={16}/></div></a><div className="ecosystem-card"><div className="ecosystem-card-head"><span className="ecosystem-monogram ecosystem-node">js</span><span className="ecosystem-kind">BINDINGS / NODE.JS</span><a href={NAPI_REPO} target="_blank" rel="noopener noreferrer" aria-label="Open monio-napi on GitHub"><ArrowUpRight size={20}/></a></div><h3>monio-napi</h3><p>Cross-platform input monitoring for JavaScript and TypeScript, powered by Monio and napi-rs. For Node.js and Electron on macOS, Windows, and Linux X11.</p><div className="ecosystem-install">npm install monio-napi</div><div className="ecosystem-card-links"><a href={NAPI_REPO} target="_blank" rel="noopener noreferrer">GitHub <ArrowUpRight size={16}/></a><a href={NAPI_NPM} target="_blank" rel="noopener noreferrer">npm package <ArrowUpRight size={16}/></a></div></div></div></div></section>

      <section className="platform-section section-padding" id="platforms"><div className="page-container"><div className="section-intro two-col-heading"><div><div className="section-kicker"><span>05 /</span> PLATFORM REACH</div><h2>One API.<br/><em>Different operating systems.</em></h2></div><p>Native backends meet each platform on its own terms. The core interface stays familiar; platform-specific permissions and capabilities remain explicit.</p></div><div className="platform-grid"><div className="platform-card"><div className="platform-symbol">⌘</div><h3>macOS</h3><p>Core Graphics event taps, pointer capture, gestures and media keys.</p><span><Check size={14}/> Native backend</span></div><div className="platform-card"><div className="platform-symbol windows-symbol"><span/><span/><span/><span/></div><h3>Windows</h3><p>Low-level hooks, Raw Input during grabbing, and SendInput.</p><span><Check size={14}/> Native backend</span></div><div className="platform-card"><div className="platform-symbol linux-symbol">◈</div><h3>Linux · X11</h3><p>XRecord for observing, XGrab with XI2 RawMotion for capture, and XTest for simulation.</p><span><Check size={14}/> Native X11 backend</span></div><div className="platform-card"><div className="platform-symbol linux-symbol">⌁</div><h3>Linux · Wayland</h3><p>evdev/uinput backend, subject to device permissions and compositor limits. Portal support is proof-of-concept.</p><span><CircleDot size={14}/> evdev backend</span></div><div className="platform-card platform-experimental"><div className="platform-symbol harmony-symbol">∞</div><h3>HarmonyOS PC / 2in1</h3><p>Compile-checked Input Kit backend. Native device validation remains pending.</p><span><CircleDot size={14}/> Experimental</span></div></div><p className="platform-footnote">Support differs by backend and feature. macOS requires Accessibility permission; Linux evdev requires device access. <a href={PLATFORM_DOCS}>See the compatibility notes <ArrowUpRight size={13}/></a></p></div></section>

      <section className="final-cta"><div className="page-container final-cta-inner"><div className="cta-orbit" aria-hidden="true"><span>✳</span></div><div className="section-kicker"><span>06 /</span> BUILD SOMETHING</div><h2>Take control of<br/><em>the input layer.</em></h2><p>For remote desktop, global shortcuts, automation tools, and whatever you build next.</p><div className="final-actions"><a className="primary-cta" href={DOCS}>Start building <ArrowRight size={18}/></a><a className="secondary-cta" href={REPO} target="_blank" rel="noopener noreferrer"><GitHubIcon size={17}/> Explore on GitHub <ArrowUpRight size={15}/></a></div></div></section>

      <footer className="site-footer"><div className="page-container footer-inner"><div className="footer-brand"><Mark size={27}/><span>monio<span className="brand-dot">.</span></span><small>Input, without limits.</small></div><div className="footer-links"><a href={DOCS}>Docs</a><a href={REPO} target="_blank" rel="noopener noreferrer">GitHub</a><a href={CRATE} target="_blank" rel="noopener noreferrer">crates.io</a><a href={NAPI_REPO} target="_blank" rel="noopener noreferrer">monio-napi</a><a href={NAPI_NPM} target="_blank" rel="noopener noreferrer">npm</a><a href="https://github.com/HuakunShen/monio/blob/main/LICENSE" target="_blank" rel="noopener noreferrer">Apache 2.0</a></div><div className="footer-credit">OPEN SOURCE. BUILT WITH RUST. <span>✳</span></div></div></footer>
    </main>
  );
}
