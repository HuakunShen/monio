'use client';

type KeyDefinition = { label: string; code: string; units?: number; gap?: number; secondary?: string };

const rows: KeyDefinition[][] = [
  [
    { label: 'esc', code: 'Escape' }, { label: 'F1', code: 'F1', gap: 0.55 }, { label: 'F2', code: 'F2' },
    { label: 'F3', code: 'F3' }, { label: 'F4', code: 'F4' }, { label: 'F5', code: 'F5', gap: 0.28 },
    { label: 'F6', code: 'F6' }, { label: 'F7', code: 'F7' }, { label: 'F8', code: 'F8' },
    { label: 'F9', code: 'F9', gap: 0.28 }, { label: 'F10', code: 'F10' }, { label: 'F11', code: 'F11' },
    { label: 'F12', code: 'F12' },
  ],
  [
    { label: '`', code: 'Backquote', secondary: '~' }, { label: '1', code: 'Digit1' }, { label: '2', code: 'Digit2' },
    { label: '3', code: 'Digit3' }, { label: '4', code: 'Digit4' }, { label: '5', code: 'Digit5' },
    { label: '6', code: 'Digit6' }, { label: '7', code: 'Digit7' }, { label: '8', code: 'Digit8' },
    { label: '9', code: 'Digit9' }, { label: '0', code: 'Digit0' }, { label: '-', code: 'Minus' },
    { label: '=', code: 'Equal' }, { label: 'delete', code: 'Backspace', units: 2 },
  ],
  [
    { label: 'tab', code: 'Tab', units: 1.5 }, { label: 'Q', code: 'KeyQ' }, { label: 'W', code: 'KeyW' },
    { label: 'E', code: 'KeyE' }, { label: 'R', code: 'KeyR' }, { label: 'T', code: 'KeyT' },
    { label: 'Y', code: 'KeyY' }, { label: 'U', code: 'KeyU' }, { label: 'I', code: 'KeyI' },
    { label: 'O', code: 'KeyO' }, { label: 'P', code: 'KeyP' }, { label: '[', code: 'BracketLeft' },
    { label: ']', code: 'BracketRight' }, { label: '\\', code: 'Backslash', units: 1.5 },
  ],
  [
    { label: 'caps lock', code: 'CapsLock', units: 1.8 }, { label: 'A', code: 'KeyA' }, { label: 'S', code: 'KeyS' },
    { label: 'D', code: 'KeyD' }, { label: 'F', code: 'KeyF' }, { label: 'G', code: 'KeyG' },
    { label: 'H', code: 'KeyH' }, { label: 'J', code: 'KeyJ' }, { label: 'K', code: 'KeyK' },
    { label: 'L', code: 'KeyL' }, { label: ';', code: 'Semicolon' }, { label: "'", code: 'Quote' },
    { label: 'return ↵', code: 'Enter', units: 2.2 },
  ],
  [
    { label: 'shift ⇧', code: 'ShiftLeft', units: 2.3 }, { label: 'Z', code: 'KeyZ' }, { label: 'X', code: 'KeyX' },
    { label: 'C', code: 'KeyC' }, { label: 'V', code: 'KeyV' }, { label: 'B', code: 'KeyB' },
    { label: 'N', code: 'KeyN' }, { label: 'M', code: 'KeyM' }, { label: ',', code: 'Comma' },
    { label: '.', code: 'Period' }, { label: '/', code: 'Slash' }, { label: 'shift ⇧', code: 'ShiftRight', units: 2.7 },
  ],
  [
    { label: 'fn', code: 'Fn' }, { label: 'control', code: 'ControlLeft' }, { label: 'option', code: 'AltLeft' },
    { label: '⌘ command', code: 'MetaLeft', units: 1.3 }, { label: '', code: 'Space', units: 5.25 },
    { label: '⌘ command', code: 'MetaRight', units: 1.3 }, { label: 'option', code: 'AltRight' },
    { label: '◀', code: 'ArrowLeft' }, { label: '▲', code: 'ArrowUp' }, { label: '▶', code: 'ArrowRight' },
  ],
];

const unit = 44;
const gap = 4.5;
const rowY = [20, 74, 120, 166, 212, 258];

export function KeyboardVisual({ pressedKey, onKeyClick }: { pressedKey?: string | null; onKeyClick?: (code: string, label: string) => void }) {
  return (
    <svg className="monio-keyboard" viewBox="0 0 715 318" role="img" aria-label="Interactive six-row mechanical keyboard visualization; press a real keyboard key or click a keycap">
      <defs>
        <linearGradient id="keyboardBody" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="#303c3c"/><stop offset=".28" stopColor="#1f292b"/><stop offset="1" stopColor="#0e1417"/></linearGradient>
        <linearGradient id="keyTop" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="#313d3e"/><stop offset="1" stopColor="#212b2f"/></linearGradient>
        <linearGradient id="keyActive" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="#ccffe0"/><stop offset="1" stopColor="#76dba7"/></linearGradient>
        <filter id="keyGlow" x="-80%" y="-100%" width="260%" height="300%"><feGaussianBlur stdDeviation="5"/></filter>
      </defs>
      <rect x="3" y="4" width="709" height="309" rx="20" fill="#070c0d" stroke="#344142" strokeWidth="2"/>
      <rect x="7" y="8" width="701" height="297" rx="17" fill="url(#keyboardBody)" stroke="#536365" strokeOpacity=".45"/>
      <path d="M22 14 H694" stroke="#96b6af" strokeOpacity=".17" strokeWidth="1"/>
      {rows.map((row, rowIndex) => {
        let pos = 16;
        const keyHeight = rowIndex === 0 ? 34 : 39;
        return row.map((item) => {
          pos += (item.gap ?? 0) * unit;
          const width = (item.units ?? 1) * unit - gap;
          const x = pos;
          const y = rowY[rowIndex];
          pos += (item.units ?? 1) * unit;
          const active = pressedKey === item.code;
          return (
            <g key={item.code} className={`keyboard-key ${active ? 'is-pressed' : ''}`} onClick={() => onKeyClick?.(item.code, item.label)}>
              {active && <rect x={x + 1} y={y + 1} width={width - 2} height={keyHeight - 2} rx="6" fill="#8ff1bc" opacity=".65" filter="url(#keyGlow)"/>}
              <rect x={x} y={y + 3} width={width} height={keyHeight} rx="6" fill="#0b1214" stroke="#3a4a4c" strokeWidth=".6"/>
              <rect x={x + 1} y={y} width={width - 2} height={keyHeight - 3} rx="5.2" fill={active ? 'url(#keyActive)' : 'url(#keyTop)'} stroke={active ? '#b6ffcf' : '#536165'} strokeOpacity={active ? '1' : '.55'} strokeWidth=".7"/>
              <path d={`M${x + 5} ${y + 5} H${x + width - 5}`} stroke={active ? '#f1fff7' : '#718488'} strokeOpacity={active ? '.48' : '.22'}/>
              <text x={x + (item.label.length > 5 ? 6 : width / 2)} y={y + keyHeight * .59} textAnchor={item.label.length > 5 ? 'start' : 'middle'} fill={active ? '#174635' : '#aebdbf'} fontSize={item.label.length > 5 ? '8.3' : item.label.length === 0 ? '10' : '11'} fontFamily="ui-sans-serif, system-ui, sans-serif" fontWeight={active ? '700' : '500'}>{item.label}</text>
            </g>
          );
        });
      })}
      <circle cx="685" cy="289" r="2.3" fill="#93edb9" opacity=".8"/>
    </svg>
  );
}
