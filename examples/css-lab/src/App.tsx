// One page per area of CSS, opened by path (/box, /text...; ROUTE=/text in gpuijs). Each cell names
// the feature it shows, so a difference against the browser points straight at a property.
import type { CSSProperties, ReactNode } from 'react';

import './lab.css';

type CellProps = { name: string; s?: CSSProperties; className?: string; children?: ReactNode; wide?: boolean; tall?: boolean };

function Cell({ name, s, className, children, wide, tall }: CellProps) {
  return (
    <div className={['cell', wide && 'wide', tall && 'tall'].filter(Boolean).join(' ')}>
      <div className="name">{name}</div>
      {children ?? <div className={className ?? 'demo'} style={s} />}
    </div>
  );
}

const lorem = 'The quick brown fox jumps over the lazy dog near the riverbank at dawn.';
const items = (n: number, s?: (i: number) => CSSProperties) =>
  Array.from({ length: n }, (_, i) => (
    <div key={i} style={{ background: ['#f87171', '#60a5fa', '#34d399', '#fbbf24', '#a78bfa'][i % 5], minWidth: 18, minHeight: 14, fontSize: 10, ...s?.(i) }}>
      {i + 1}
    </div>
  ));
const box = (s: CSSProperties, children?: ReactNode) => (
  <div style={{ background: '#e5e7eb', width: '100%', height: 64, ...s }}>{children}</div>
);

const pages: Record<string, ReactNode> = {
  box: (
    <>
      <Cell name="border solid 4px" s={{ border: '4px solid #2563eb' }} />
      <Cell name="border dashed 3px" s={{ border: '3px dashed #2563eb' }} />
      <Cell name="border dotted 3px" s={{ border: '3px dotted #2563eb' }} />
      <Cell name="border double 6px" s={{ border: '6px double #2563eb' }} />
      <Cell name="side colors" s={{ borderWidth: 5, borderStyle: 'solid', borderColor: '#ef4444 #22c55e #3b82f6 #eab308' }} />
      <Cell name="border-top only" s={{ borderTop: '6px solid #111827' }} />
      <Cell name="radius 12px" s={{ borderRadius: 12 }} />
      <Cell name="radius per corner" s={{ borderRadius: '0 20px 4px 28px' }} />
      <Cell name="radius 50% (ellipse)" s={{ borderRadius: '50%' }} />
      <Cell name="radius 40px/16px" s={{ borderRadius: '40px / 16px' }} />
      <Cell name="radius + border" s={{ borderRadius: 16, border: '4px solid #1d4ed8' }} />
      <Cell name="pill" s={{ borderRadius: 9999, height: 28 }} />
      <Cell name="box-shadow" s={{ boxShadow: '0 4px 10px rgba(0,0,0,.35)' }} />
      <Cell name="shadow spread" s={{ boxShadow: '0 0 0 4px #f59e0b' }} />
      <Cell name="shadow inset" s={{ boxShadow: 'inset 0 0 12px 2px rgba(0,0,0,.5)' }} />
      <Cell name="shadow multiple" s={{ boxShadow: '4px 4px 0 #ef4444, -4px -4px 0 #22c55e' }} />
      <Cell name="outline + offset" s={{ outline: '2px dashed #dc2626', outlineOffset: 4 }} />
      <Cell name="opacity .4" s={{ opacity: 0.4 }} />
      <Cell name="padding" s={{ padding: '8px 24px', backgroundClip: 'content-box', background: '#2563eb', border: '1px solid #111' }} />
      <Cell name="margin (child)">{box({}, <div className="swatch" style={{ margin: '10px 0 0 30px' }} />)}</Cell>
      <Cell name="negative margin">{box({}, <div className="swatch" style={{ marginTop: -6, marginLeft: -6 }} />)}</Cell>
      <Cell name="content-box sizing" s={{ boxSizing: 'content-box', width: 80, padding: 10, border: '4px solid #111' }} />
      <Cell name="border-box sizing" s={{ boxSizing: 'border-box', width: 80, padding: 10, border: '4px solid #111' }} />
      <Cell name="rgba bg over stripes">{box({ background: 'repeating-linear-gradient(90deg,#111 0 8px,#fff 8px 16px)' }, <div style={{ height: '100%', background: 'rgba(37,99,235,.5)' }} />)}</Cell>
      <Cell name="hsl / oklch colors" s={{ background: 'linear-gradient(90deg, hsl(200 80% 50%), oklch(70% 0.2 30))' }} />
      <Cell name="color-mix()" s={{ background: 'color-mix(in srgb, #ef4444 50%, #3b82f6)' }} />
      <Cell name="currentColor border" s={{ color: '#9333ea', border: '4px solid currentColor' }} />
      <Cell name="transparent border" s={{ border: '10px solid transparent', background: '#16a34a' }} />
      <Cell name="width 0 border-left" s={{ borderLeft: '0 solid red', borderRight: '8px solid #111' }} />
      <Cell name="var() custom prop" s={{ ['--c' as string]: '#db2777', background: 'var(--c)' }} />
    </>
  ),
  text: (
    <>
      <Cell name="font-size 12/16/24"><span style={{ fontSize: 12 }}>Aa </span><span style={{ fontSize: 16 }}>Aa </span><span style={{ fontSize: 24 }}>Aa</span></Cell>
      <Cell name="weights 300→800"><div><span style={{ fontWeight: 300 }}>Lt </span><span style={{ fontWeight: 400 }}>Rg </span><span style={{ fontWeight: 600 }}>Sb </span><span style={{ fontWeight: 800 }}>Xb</span></div></Cell>
      <Cell name="italic / oblique"><i>italic</i> <span style={{ fontStyle: 'oblique' }}>oblique</span></Cell>
      <Cell name="monospace / serif"><div style={{ fontFamily: 'ui-monospace, monospace' }}>mono 0O1l</div><div style={{ fontFamily: 'Georgia, serif' }}>serif Gg</div></Cell>
      <Cell name="line-height 1 vs 2"><div style={{ display: 'flex', gap: 6 }}><div style={{ lineHeight: 1, fontSize: 11, background: '#e0e7ff', width: 50 }}>one two three</div><div style={{ lineHeight: 2, fontSize: 11, background: '#e0e7ff', width: 50 }}>one two three</div></div></Cell>
      <Cell name="letter-spacing 3px / -1px"><div style={{ letterSpacing: 3 }}>SPACED</div><div style={{ letterSpacing: -1 }}>tightened</div></Cell>
      <Cell name="word-spacing 10px"><div style={{ wordSpacing: 10 }}>one two three</div></Cell>
      <Cell name="text-align left/center/right"><div style={{ textAlign: 'left' }}>left</div><div style={{ textAlign: 'center' }}>center</div><div style={{ textAlign: 'right' }}>right</div></Cell>
      <Cell name="text-align justify"><div className="lorem" style={{ textAlign: 'justify' }}>{lorem}</div></Cell>
      <Cell name="underline / line-through / overline"><span style={{ textDecoration: 'underline' }}>under</span> <span style={{ textDecoration: 'line-through' }}>strike</span> <span style={{ textDecoration: 'overline' }}>over</span></Cell>
      <Cell name="decoration color/style/thickness"><span style={{ textDecoration: 'underline wavy #dc2626', textDecorationThickness: 2 }}>wavy</span> <span style={{ textDecoration: 'underline dotted #2563eb' }}>dotted</span> <span style={{ textDecoration: 'underline', textUnderlineOffset: 5 }}>offset</span></Cell>
      <Cell name="text-transform"><div style={{ textTransform: 'uppercase' }}>upper case</div><div style={{ textTransform: 'capitalize' }}>capital each word</div><div style={{ textTransform: 'lowercase' }}>LOWER</div></Cell>
      <Cell name="white-space: nowrap"><div style={{ whiteSpace: 'nowrap', overflow: 'hidden', width: '100%' }}>{lorem}</div></Cell>
      <Cell name="text-wrap: balance / nowrap"><div style={{ width: '100%', fontSize: 12 }}><div style={{ textWrap: 'balance', textAlign: 'center' } as CSSProperties}>{lorem}</div><div style={{ textWrap: 'nowrap', overflow: 'hidden' } as CSSProperties}>{lorem}</div></div></Cell>
      <Cell name="white-space: pre"><div style={{ whiteSpace: 'pre', fontSize: 11 }}>{'a   b\n  c\td'}</div></Cell>
      <Cell name="white-space: pre-wrap"><div style={{ whiteSpace: 'pre-wrap', fontSize: 11 }}>{'keep   spaces\nand lines'}</div></Cell>
      <Cell name="white-space: pre-line"><div style={{ whiteSpace: 'pre-line', fontSize: 11 }}>{'collapse   spaces\nkeep lines'}</div></Cell>
      <Cell name="text-overflow: ellipsis"><div className="ellipsis">{lorem}</div></Cell>
      <Cell name="line-clamp 2"><div className="clamp lorem">{lorem} {lorem}</div></Cell>
      <Cell name="text-indent 20px"><div className="lorem" style={{ textIndent: 20 }}>{lorem}</div></Cell>
      <Cell name="text-shadow"><div style={{ fontSize: 20, fontWeight: 700, textShadow: '2px 2px 0 #f59e0b' }}>Shadow</div></Cell>
      <Cell name="small-caps"><div style={{ fontVariant: 'small-caps' }}>Small Caps</div></Cell>
      <Cell name="sub / sup">H<sub>2</sub>O and x<sup>2</sup></Cell>
      <Cell name="inline bg + padding"><div className="lorem">a <span style={{ background: '#fde68a', padding: '2px 6px', borderRadius: 4 }}>highlighted span</span> in text that wraps along</div></Cell>
      <Cell name="mixed sizes inline"><div>small <span style={{ fontSize: 22 }}>BIG</span> small</div></Cell>
      <Cell name="inline bold/color/link"><div className="lorem">plain <b>bold</b> <span style={{ color: '#dc2626' }}>red</span> <a href="#x">a link</a></div></Cell>
      <Cell name="word-break: break-all"><div style={{ wordBreak: 'break-all', fontSize: 12 }}>Supercalifragilisticexpialidocious</div></Cell>
      <Cell name="overflow-wrap: anywhere"><div style={{ overflowWrap: 'anywhere', fontSize: 12, width: 90 }}>https://example.com/a/very/long/path</div></Cell>
      <Cell name="direction: rtl"><div style={{ direction: 'rtl' }}>start → end</div></Cell>
      <Cell name="writing-mode vertical"><div style={{ writingMode: 'vertical-rl', height: 56 }}>縦書き</div></Cell>
      <Cell name="font-variant-numeric"><div style={{ fontVariantNumeric: 'tabular-nums' }}>1111</div><div>1111</div></Cell>
    </>
  ),
  flex: (
    <>
      <Cell name="row gap 4">{box({ display: 'flex', gap: 4 }, items(4))}</Cell>
      <Cell name="column">{box({ display: 'flex', flexDirection: 'column' }, items(3))}</Cell>
      <Cell name="row-reverse">{box({ display: 'flex', flexDirection: 'row-reverse' }, items(4))}</Cell>
      <Cell name="column-reverse">{box({ display: 'flex', flexDirection: 'column-reverse' }, items(3))}</Cell>
      <Cell name="wrap">{box({ display: 'flex', flexWrap: 'wrap', gap: 2 }, items(12, () => ({ width: 22 })))}</Cell>
      <Cell name="wrap-reverse">{box({ display: 'flex', flexWrap: 'wrap-reverse', gap: 2 }, items(12, () => ({ width: 22 })))}</Cell>
      <Cell name="justify center">{box({ display: 'flex', justifyContent: 'center' }, items(3))}</Cell>
      <Cell name="justify end">{box({ display: 'flex', justifyContent: 'flex-end' }, items(3))}</Cell>
      <Cell name="space-between">{box({ display: 'flex', justifyContent: 'space-between' }, items(3))}</Cell>
      <Cell name="space-around">{box({ display: 'flex', justifyContent: 'space-around' }, items(3))}</Cell>
      <Cell name="space-evenly">{box({ display: 'flex', justifyContent: 'space-evenly' }, items(3))}</Cell>
      <Cell name="align center">{box({ display: 'flex', alignItems: 'center' }, items(3))}</Cell>
      <Cell name="align end">{box({ display: 'flex', alignItems: 'flex-end' }, items(3))}</Cell>
      <Cell name="align stretch">{box({ display: 'flex', alignItems: 'stretch' }, items(3))}</Cell>
      <Cell name="align baseline">{box({ display: 'flex', alignItems: 'baseline' }, items(3, (i) => ({ fontSize: 10 + i * 6 })))}</Cell>
      <Cell name="align-content between">{box({ display: 'flex', flexWrap: 'wrap', alignContent: 'space-between' }, items(10, () => ({ width: 30 })))}</Cell>
      <Cell name="grow 1 / 2">{box({ display: 'flex' }, items(3, (i) => ({ flexGrow: i })))}</Cell>
      <Cell name="shrink 0 vs 1">{box({ display: 'flex' }, items(3, (i) => ({ width: 80, flexShrink: i === 0 ? 0 : 1 })))}</Cell>
      <Cell name="basis 50%">{box({ display: 'flex' }, items(3, (i) => ({ flexBasis: i === 0 ? '50%' : 'auto' })))}</Cell>
      <Cell name="order">{box({ display: 'flex' }, items(4, (i) => ({ order: 3 - i })))}</Cell>
      <Cell name="align-self">{box({ display: 'flex', alignItems: 'flex-start' }, items(3, (i) => ({ alignSelf: ['auto', 'center', 'flex-end'][i] })))}</Cell>
      <Cell name="margin-left auto">{box({ display: 'flex' }, items(3, (i) => (i === 2 ? { marginLeft: 'auto' } : {})))}</Cell>
      <Cell name="margin auto (center)">{box({ display: 'flex' }, <div className="swatch" style={{ margin: 'auto' }} />)}</Cell>
      <Cell name="min-width 0 truncation">{box({ display: 'flex', gap: 4 }, <><div className="swatch" style={{ flexShrink: 0 }} /><div className="ellipsis" style={{ minWidth: 0 }}>{lorem}</div></>)}</Cell>
      <Cell name="row-gap / column-gap">{box({ display: 'flex', flexWrap: 'wrap', rowGap: 10, columnGap: 2 }, items(8, () => ({ width: 26 })))}</Cell>
      <Cell name="inline-flex">{box({}, <span>a <span style={{ display: 'inline-flex', gap: 2 }}>{items(2)}</span> b</span>)}</Cell>
      <Cell name="nested flex">{box({ display: 'flex', gap: 4, padding: 4 }, <><div style={{ display: 'flex', flexDirection: 'column', flex: 1, gap: 2 }}>{items(2)}</div><div style={{ flex: 2, display: 'flex', alignItems: 'center', justifyContent: 'center', background: '#c4b5fd' }}>x</div></>)}</Cell>
      <Cell name="flex: 1 1 0">{box({ display: 'flex' }, items(3, (i) => ({ flex: '1 1 0', fontSize: 9 + i * 4 })))}</Cell>
      <Cell name="column justify center">{box({ display: 'flex', flexDirection: 'column', justifyContent: 'center', alignItems: 'center' }, items(2))}</Cell>
      <Cell name="max-width item">{box({ display: 'flex' }, items(2, (i) => ({ flexGrow: 1, maxWidth: i ? 40 : undefined })))}</Cell>
    </>
  ),
  grid: (
    <>
      <Cell name="repeat(3, 1fr)">{box({ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 2 }, items(6))}</Cell>
      <Cell name="40px 1fr 2fr">{box({ display: 'grid', gridTemplateColumns: '40px 1fr 2fr', gap: 2 }, items(3))}</Cell>
      <Cell name="minmax(30px, 1fr)">{box({ display: 'grid', gridTemplateColumns: 'minmax(30px, 1fr) 60px', gap: 2 }, items(4))}</Cell>
      <Cell name="auto-fill minmax(30px,1fr)">{box({ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(30px, 1fr))', gap: 2 }, items(5))}</Cell>
      <Cell name="auto-fit minmax(30px,1fr)">{box({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(30px, 1fr))', gap: 2 }, items(2))}</Cell>
      <Cell name="auto-fit minmax(min(100%,30px),1fr)">{box({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(min(100%, 30px), 1fr))', gap: 2 }, items(5))}</Cell>
      <Cell name="template rows 1fr 2fr">{box({ display: 'grid', gridTemplateRows: '1fr 2fr', gap: 2 }, items(2))}</Cell>
      <Cell name="template areas">{box({ display: 'grid', gridTemplateAreas: '"a a b" "c d b"', gap: 2 }, ['a', 'b', 'c', 'd'].map((a, i) => <div key={a} style={{ gridArea: a, background: ['#f87171', '#60a5fa', '#34d399', '#fbbf24'][i], fontSize: 10 }}>{a}</div>))}</Cell>
      <Cell name="span 2 columns">{box({ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 2 }, items(4, (i) => (i === 0 ? { gridColumn: 'span 2' } : {})))}</Cell>
      <Cell name="row span 2">{box({ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 2 }, items(5, (i) => (i === 0 ? { gridRow: 'span 2' } : {})))}</Cell>
      <Cell name="lines 2 / -1">{box({ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 2 }, items(2, (i) => (i === 0 ? { gridColumn: '2 / -1' } : {})))}</Cell>
      <Cell name="auto-flow column">{box({ display: 'grid', gridTemplateRows: 'repeat(2, 1fr)', gridAutoFlow: 'column', gap: 2 }, items(6))}</Cell>
      <Cell name="auto-flow dense">{box({ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gridAutoFlow: 'dense', gap: 2 }, items(5, (i) => (i === 1 ? { gridColumn: 'span 2' } : {})))}</Cell>
      <Cell name="justify-items center">{box({ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', justifyItems: 'center' }, items(3))}</Cell>
      <Cell name="align-items end">{box({ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', alignItems: 'end' }, items(3))}</Cell>
      <Cell name="place-self center">{box({ display: 'grid' }, <div className="swatch" style={{ placeSelf: 'center' }} />)}</Cell>
      <Cell name="justify-content between">{box({ display: 'grid', gridTemplateColumns: 'repeat(3, 24px)', justifyContent: 'space-between' }, items(3))}</Cell>
      <Cell name="align-content center">{box({ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', alignContent: 'center' }, items(3))}</Cell>
      <Cell name="grid-auto-rows 20px">{box({ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gridAutoRows: 20, gap: 2 }, items(6))}</Cell>
      <Cell name="auto columns (content)">{box({ display: 'grid', gridTemplateColumns: 'auto auto 1fr', gap: 2 }, items(3, (i) => ({ padding: `0 ${i * 6}px` })))}</Cell>
      <Cell name="gap 8 / 2">{box({ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', rowGap: 8, columnGap: 2 }, items(6))}</Cell>
      <Cell name="inline-grid">{box({}, <span>a <span style={{ display: 'inline-grid', gridTemplateColumns: 'repeat(2, 12px)', gap: 1 }}>{items(4)}</span> b</span>)}</Cell>
      <Cell name="fit-content(50px)">{box({ display: 'grid', gridTemplateColumns: 'fit-content(50px) 1fr', gap: 2 }, items(2, (i) => (i === 0 ? { whiteSpace: 'normal' } : {})))}</Cell>
      <Cell name="order in grid">{box({ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 2 }, items(3, (i) => ({ order: -i })))}</Cell>
      <Cell name="named lines">{box({ display: 'grid', gridTemplateColumns: '[a] 1fr [b] 2fr [c]', gap: 2 }, <div style={{ gridColumn: 'b / c', background: '#a78bfa' }}>b→c</div>)}</Cell>
    </>
  ),
  position: (
    <>
      <Cell name="relative top/left">{box({}, <div className="swatch" style={{ position: 'relative', top: 10, left: 20 }} />)}</Cell>
      <Cell name="absolute corners">{box({ position: 'relative' }, <><div className="swatch" style={{ position: 'absolute', top: 0, left: 0 }} /><div className="swatch" style={{ position: 'absolute', right: 0, bottom: 0, background: '#16a34a' }} /></>)}</Cell>
      <Cell name="absolute inset 0 + margin auto">{box({ position: 'relative' }, <div className="swatch" style={{ position: 'absolute', inset: 0, margin: 'auto' }} />)}</Cell>
      <Cell name="absolute % insets">{box({ position: 'relative' }, <div style={{ position: 'absolute', left: '25%', right: '25%', top: '20%', bottom: '20%', background: '#f97316' }} />)}</Cell>
      <Cell name="translate(-50%,-50%) center">{box({ position: 'relative' }, <div className="swatch" style={{ position: 'absolute', left: '50%', top: '50%', transform: 'translate(-50%, -50%)' }} />)}</Cell>
      <Cell name="z-index stacking">{box({ position: 'relative' }, <><div className="swatch" style={{ position: 'absolute', left: 10, top: 10, zIndex: 2 }} /><div className="swatch" style={{ position: 'absolute', left: 24, top: 20, background: '#2563eb', zIndex: 1 }} /><div className="swatch" style={{ position: 'absolute', left: 38, top: 30, background: '#16a34a' }} /></>)}</Cell>
      <Cell name="rounded-full avatar"><div style={{ width: 64, height: 64, borderRadius: 9999, overflow: "hidden", display: "flex" }}><img src="/photo.png" style={{ width: "100%", height: "100%", objectFit: "cover" }} /></div></Cell>
      <Cell name="overflow hidden + radius">{box({ overflow: 'hidden', borderRadius: 20 }, <div style={{ width: 200, height: 100, background: 'linear-gradient(45deg,#ef4444,#3b82f6)' }} />)}</Cell>
      <Cell name="overflow scroll">{box({ overflow: 'auto' }, <div style={{ height: 200, background: 'linear-gradient(#fde68a,#f87171)' }}>scroll me</div>)}</Cell>
      <Cell name="overflow-x hidden">{box({ overflowX: 'hidden', whiteSpace: 'nowrap' }, lorem)}</Cell>
      <Cell name="overflow clip">{box({ overflow: 'clip' }, <div className="swatch" style={{ marginTop: 50 }} />)}</Cell>
      <Cell name="rotate 15deg">{box({ display: 'grid', placeItems: 'center' }, <div className="swatch" style={{ transform: 'rotate(15deg)', width: 40 }} />)}</Cell>
      <Cell name="scale 1.5">{box({ display: 'grid', placeItems: 'center' }, <div className="swatch" style={{ transform: 'scale(1.5)' }} />)}</Cell>
      <Cell name="skewX 20deg">{box({ display: 'grid', placeItems: 'center' }, <div className="swatch" style={{ transform: 'skewX(20deg)', width: 40 }} />)}</Cell>
      <Cell name="origin top left + rotate">{box({}, <div className="swatch" style={{ transform: 'rotate(20deg)', transformOrigin: 'top left', marginLeft: 20 }} />)}</Cell>
      <Cell name="translate + rotate">{box({}, <div className="swatch" style={{ transform: 'translateX(40px) rotate(45deg)', marginTop: 12 }} />)}</Cell>
      <Cell name="rotate text">{box({ display: 'grid', placeItems: 'center' }, <div style={{ transform: 'rotate(-10deg)', fontWeight: 700 }}>tilted text</div>)}</Cell>
      <Cell name="skew(-15deg, 5deg)">{box({ display: 'grid', placeItems: 'center' }, <div className="swatch" style={{ transform: 'skew(-15deg, 5deg)', width: 60 }} />)}</Cell>
      <Cell name="sticky in scroll box">{box({ overflow: 'auto' }, <><div style={{ position: 'sticky', top: 0, background: '#111827', color: '#fff', fontSize: 11 }}>sticky header</div><div style={{ height: 120 }}>content</div></>)}</Cell>
      <Cell name="badge ::after absolute"><div className="badge demo" style={{ width: 40, height: 28 }} /></Cell>
      <Cell name="individual translate/scale" s={{ translate: '20px 6px', scale: '0.8', width: 60 }} />
      <Cell name="inset shorthand %">{box({ position: 'relative' }, <div style={{ position: 'absolute', inset: '10% 40% 10% 10%', background: '#0ea5e9' }} />)}</Cell>
      <Cell name="visibility hidden">{box({ display: 'flex', gap: 4 }, items(3, (i) => (i === 1 ? { visibility: 'hidden' } : {})))}</Cell>
      <Cell name="display none">{box({ display: 'flex', gap: 4 }, items(3, (i) => (i === 1 ? { display: 'none' } : {})))}</Cell>
      <Cell name="display contents">{box({ display: 'flex', gap: 4 }, <div style={{ display: 'contents' }}>{items(3)}</div>)}</Cell>
      <Cell name="perspective rotateY">{box({ perspective: 200, display: 'grid', placeItems: 'center' }, <div className="swatch" style={{ transform: 'rotateY(45deg)', width: 50 }} />)}</Cell>
    </>
  ),
  backgrounds: (
    <>
      <Cell name="linear to right" s={{ background: 'linear-gradient(to right, #ef4444, #3b82f6)' }} />
      <Cell name="linear 45deg" s={{ background: 'linear-gradient(45deg, #f59e0b, #10b981)' }} />
      <Cell name="linear 5 stops" s={{ background: 'linear-gradient(90deg, red, orange, yellow, green, blue)' }} />
      <Cell name="hard stops" s={{ background: 'linear-gradient(90deg, #111 0 33%, #fff 33% 66%, #dc2626 66%)' }} />
      <Cell name="to bottom right" s={{ background: 'linear-gradient(to bottom right, #a855f7, #ec4899)' }} />
      <Cell name="transparent fade" s={{ background: 'linear-gradient(to right, #2563eb, transparent)' }} />
      <Cell name="radial circle" s={{ background: 'radial-gradient(circle, #fde047, #ea580c)' }} />
      <Cell name="radial ellipse at top left" s={{ background: 'radial-gradient(ellipse at top left, #22d3ee, #1e3a8a)' }} />
      <Cell name="conic-gradient" s={{ background: 'conic-gradient(red, yellow, lime, aqua, blue, magenta, red)' }} />
      <Cell name="repeating stripes" s={{ background: 'repeating-linear-gradient(45deg, #111 0 6px, #facc15 6px 12px)' }} />
      <Cell name="pattern (bg-size)" s={{ background: 'radial-gradient(#2563eb 2px, transparent 3px) 0 0 / 12px 12px, #eff6ff' }} />
      <Cell name="multiple backgrounds" s={{ background: 'linear-gradient(rgba(255,255,255,.6), rgba(255,255,255,0)), linear-gradient(90deg, #ef4444, #3b82f6)' }} />
      <Cell name="image cover" s={{ background: 'url(/photo.png) center / cover' }} />
      <Cell name="image contain" s={{ background: 'url(/photo.png) center / contain no-repeat #111' }} />
      <Cell name="image 40px repeat" s={{ background: 'url(/photo.png) 0 0 / 40px auto repeat' }} />
      <Cell name="image no-repeat right" s={{ background: 'url(/photo.png) right bottom / 50px no-repeat #fde68a' }} />
      <Cell name="clip text"><div style={{ fontSize: 26, fontWeight: 800, background: 'linear-gradient(90deg,#ef4444,#3b82f6)', WebkitBackgroundClip: 'text', backgroundClip: 'text', color: 'transparent' }}>GRADIENT</div></Cell>
      <Cell name="clip padding-box" s={{ border: '8px dashed rgba(0,0,0,.4)', backgroundClip: 'padding-box', background: '#22c55e' }} />
      <Cell name="clip content-box" s={{ padding: 10, backgroundClip: 'content-box', backgroundColor: '#22c55e', border: '1px solid #111' }} />
      <Cell name="bg-attachment / origin" s={{ background: 'linear-gradient(#f43f5e,#f43f5e) no-repeat 0 0 / 20px 20px', backgroundOrigin: 'content-box', padding: 12, border: '1px solid #111' }} />
      <Cell name="gradient + radius + border" s={{ background: 'linear-gradient(135deg,#6366f1,#ec4899)', borderRadius: 14, border: '3px solid #111' }} />
      <Cell name="oklch gradient" s={{ background: 'linear-gradient(90deg in oklch, oklch(70% .2 30), oklch(70% .2 250))' }} />
      <Cell name="img object-fit cover"><img src="/photo.png" style={{ width: 120, height: 56, objectFit: 'cover' }} /></Cell>
      <Cell name="img object-fit contain"><img src="/photo.png" style={{ width: 120, height: 56, objectFit: 'contain', background: '#111' }} /></Cell>
      <Cell name="img natural size 60px w"><img src="/photo.png" style={{ width: 60 }} /></Cell>
      <Cell name="img border-radius 50%"><img src="/photo.png" style={{ width: 56, height: 56, borderRadius: '50%', objectFit: 'cover' }} /></Cell>
      <Cell name="img object-position left"><img src="/photo.png" style={{ width: 120, height: 56, objectFit: 'cover', objectPosition: 'left top' }} /></Cell>
      <Cell name="svg inline"><svg width="120" height="56" viewBox="0 0 120 56"><circle cx="28" cy="28" r="24" fill="#f43f5e" /><rect x="60" y="8" width="50" height="40" rx="8" fill="none" stroke="#2563eb" strokeWidth="4" /></svg></Cell>
    </>
  ),
  effects: (
    <>
      {(['blur(3px)', 'grayscale(1)', 'brightness(1.6)', 'contrast(2)', 'sepia(1)', 'hue-rotate(90deg)', 'invert(1)', 'saturate(3)', 'drop-shadow(3px 3px 0 #111)'] as const).map((f) => (
        <Cell key={f} name={`filter ${f}`}><img src="/photo.png" style={{ width: 84, height: 56, filter: f }} /></Cell>
      ))}
      <Cell name="backdrop-filter blur">{box({ position: 'relative', background: 'repeating-linear-gradient(45deg,#111 0 6px,#facc15 6px 12px)' }, <div style={{ position: 'absolute', inset: '10px 20px', backdropFilter: 'blur(4px)', background: 'rgba(255,255,255,.3)' }} />)}</Cell>
      <Cell name="mix-blend multiply">{box({ background: 'linear-gradient(90deg,#fde047,#22d3ee)', display: 'grid', placeItems: 'center' }, <div className="swatch" style={{ mixBlendMode: 'multiply', width: 60 }} />)}</Cell>
      <Cell name="mix-blend screen">{box({ background: 'linear-gradient(90deg,#1e3a8a,#7c2d12)', display: 'grid', placeItems: 'center' }, <div className="swatch" style={{ mixBlendMode: 'screen', width: 60 }} />)}</Cell>
      <Cell name="clip-path circle" s={{ clipPath: 'circle(40%)', background: '#ec4899' }} />
      <Cell name="clip-path polygon" s={{ clipPath: 'polygon(50% 0, 100% 100%, 0 100%)', background: '#8b5cf6' }} />
      <Cell name="clip-path inset round" s={{ clipPath: 'inset(8px 16px round 10px)', background: '#0ea5e9' }} />
      <Cell name="mask-image fade" s={{ maskImage: 'linear-gradient(to right, #000, transparent)', WebkitMaskImage: 'linear-gradient(to right, #000, transparent)', background: '#111827' }} />
      <Cell name="nested opacity">{box({ opacity: 0.5, background: '#dc2626', padding: 8 }, <div style={{ opacity: 0.5, background: '#111', height: '100%' }} />)}</Cell>
      <Cell name="big blur shadow" s={{ boxShadow: '0 20px 30px -10px rgba(37,99,235,.8)', borderRadius: 10 }} />
      <Cell name="filter on text"><div style={{ filter: 'blur(1px)', fontWeight: 700, fontSize: 18 }}>blurred text</div></Cell>
      <Cell name="opacity + transform">{box({ display: 'grid', placeItems: 'center' }, <div className="swatch" style={{ opacity: 0.5, transform: 'rotate(30deg) scale(1.4)' }} />)}</Cell>
      <Cell name="filter drop-shadow (shape)"><svg width="80" height="56" style={{ filter: 'drop-shadow(2px 4px 2px rgba(0,0,0,.5))' }}><polygon points="40,4 76,52 4,52" fill="#f59e0b" /></svg></Cell>
      <Cell name="cursor pointer (n/a)" s={{ cursor: 'pointer' }} />
      <Cell name="accent-color"><input type="checkbox" defaultChecked style={{ accentColor: '#dc2626' }} /> <input type="radio" defaultChecked style={{ accentColor: '#16a34a' }} /></Cell>
      <Cell name="caret-color"><input defaultValue="type" style={{ caretColor: '#dc2626', width: 110 }} /></Cell>
    </>
  ),
  sizing: (
    <>
      <Cell name="width 50%">{box({}, <div style={{ width: '50%', height: 20, background: '#f97316' }} />)}</Cell>
      <Cell name="max-width 60px">{box({}, <div style={{ maxWidth: 60, height: 20, background: '#f97316' }} />)}</Cell>
      <Cell name="min-width 150% (overflow)">{box({ overflow: 'hidden' }, <div style={{ minWidth: '150%', height: 20, background: 'linear-gradient(90deg,#f97316,#111)' }} />)}</Cell>
      <Cell name="height 50%">{box({}, <div style={{ height: '50%', background: '#f97316' }} />)}</Cell>
      <Cell name="calc(100% - 40px)">{box({}, <div style={{ width: 'calc(100% - 40px)', height: 20, background: '#f97316' }} />)}</Cell>
      <Cell name="calc(50% + 2rem)">{box({}, <div style={{ width: 'calc(50% + 2rem)', height: 20, background: '#f97316' }} />)}</Cell>
      <Cell name="clamp(20px, 50%, 80px)">{box({}, <div style={{ width: 'clamp(20px, 50%, 80px)', height: 20, background: '#f97316' }} />)}</Cell>
      <Cell name="min(80px, 30%)">{box({}, <div style={{ width: 'min(80px, 30%)', height: 20, background: '#f97316' }} />)}</Cell>
      <Cell name="max(80px, 30%)">{box({}, <div style={{ width: 'max(80px, 30%)', height: 20, background: '#f97316' }} />)}</Cell>
      <Cell name="fit-content">{box({}, <div style={{ width: 'fit-content', background: '#fde68a' }}>fits me</div>)}</Cell>
      <Cell name="min-content">{box({}, <div style={{ width: 'min-content', background: '#fde68a' }}>min content width</div>)}</Cell>
      <Cell name="max-content (overflows)">{box({ overflow: 'hidden' }, <div style={{ width: 'max-content', background: '#fde68a' }}>{lorem}</div>)}</Cell>
      <Cell name="aspect-ratio 16/9">{box({ height: 'auto' }, <div style={{ aspectRatio: '16 / 9', width: 80, background: '#14b8a6' }} />)}</Cell>
      <Cell name="aspect-ratio 1 + height">{box({}, <div style={{ aspectRatio: 1, height: '100%', background: '#14b8a6' }} />)}</Cell>
      <Cell name="em / rem / ch">{box({ fontSize: 10 }, <><div style={{ width: '6em', height: 8, background: '#6366f1' }} /><div style={{ width: '6rem', height: 8, background: '#22c55e', marginTop: 2 }} /><div style={{ width: '10ch', height: 8, background: '#f43f5e', marginTop: 2 }} /></>)}</Cell>
      <Cell name="vw / vh">{box({ overflow: 'hidden' }, <div style={{ width: '5vw', height: '5vh', background: '#6366f1' }} />)}</Cell>
      <Cell name="min(100%, 60px) / clamp()">{box({}, <><div style={{ width: 'min(100%, 60px)', height: 14, background: '#6366f1' }} /><div style={{ width: 'clamp(20px, 50%, 200px)', height: 14, marginTop: 4, background: '#f97316' }} /></>)}</Cell>
      <Cell name="min-height 100%">{box({}, <div style={{ minHeight: '100%', width: 30, background: '#6366f1' }} />)}</Cell>
      <Cell name="max-height + overflow">{box({ height: 'auto' }, <div style={{ maxHeight: 40, overflow: 'hidden', fontSize: 11 }}>{lorem} {lorem}</div>)}</Cell>
      <Cell name="percent padding">{box({}, <div style={{ width: 60, paddingTop: '25%', background: '#6366f1' }} />)}</Cell>
      <Cell name="block auto width + margin">{box({}, <div style={{ margin: '0 20px', height: 20, background: '#f97316' }} />)}</Cell>
      <Cell name="inline-block width">{box({}, <><span style={{ display: 'inline-block', width: 40, height: 20, background: '#f97316' }} /> text <span style={{ display: 'inline-block', width: 30, height: 30, background: '#2563eb', verticalAlign: 'middle' }} /></>)}</Cell>
      <Cell name="width: auto in abs">{box({ position: 'relative' }, <div style={{ position: 'absolute', left: 8, top: 8, background: '#fde68a' }}>shrink-wraps</div>)}</Cell>
      <Cell name="stretch: abs left+right">{box({ position: 'relative' }, <div style={{ position: 'absolute', left: 8, right: 8, top: 8, background: '#fde68a' }}>stretched</div>)}</Cell>
      <Cell name="margin 0 auto center">{box({}, <div style={{ width: 50, height: 20, margin: '0 auto', background: '#f97316' }} />)}</Cell>
      <Cell name="float left">{box({ fontSize: 11 }, <><div className="swatch" style={{ float: 'left', marginRight: 4 }} />{lorem}</>)}</Cell>
      <Cell name="columns: 2">{box({ columns: 2, columnGap: 8, fontSize: 10 }, lorem + ' ' + lorem)}</Cell>
    </>
  ),
  tables: (
    <>
      <Cell name="display: table / table-cell"><div style={{ display: 'table', width: '100%', borderSpacing: 4, fontSize: 12 }}><div style={{ display: 'table-cell', background: '#e0e7ff', verticalAlign: 'middle', height: 50 }}>middle</div><div style={{ display: 'table-cell', background: '#fde68a' }}>a longer second cell</div></div></Cell>
      <Cell name="collapse, th, zebra, num" wide>
        <table className="t">
          <caption>Caption</caption>
          <thead><tr><th>Name</th><th>Qty</th><th>Price</th></tr></thead>
          <tbody>
            {[['Apple', 3, '1.20'], ['Banana', 12, '0.30'], ['Cherry', 100, '12.00'], ['Date', 7, '3.40']].map(([n, q, p]) => (
              <tr key={n as string}><td>{n}</td><td className="num">{q}</td><td className="num">{p}</td></tr>
            ))}
          </tbody>
        </table>
      </Cell>
      <Cell name="colspan / rowspan" wide>
        <table className="t">
          <tbody>
            <tr><td colSpan={2}>colspan 2</td><td rowSpan={2}>rowspan 2</td></tr>
            <tr><td>a</td><td>b</td></tr>
            <tr><td>c</td><td colSpan={2}>colspan 2</td></tr>
          </tbody>
        </table>
      </Cell>
      <Cell name="separate + spacing" wide>
        <table className="sep"><tbody><tr><td>one</td><td>two</td><td>three</td></tr><tr><td>four</td><td>five</td><td>six</td></tr></tbody></table>
      </Cell>
      <Cell name="table-layout fixed" wide>
        <table className="fixed"><tbody><tr><td style={{ width: '20%' }}>20%</td><td>{lorem}</td><td style={{ width: 50 }}>50px</td></tr></tbody></table>
      </Cell>
      <Cell name="vertical-align in cells" wide>
        <table className="t"><tbody><tr><td style={{ height: 50, verticalAlign: 'top' }}>top</td><td style={{ verticalAlign: 'middle' }}>middle</td><td style={{ verticalAlign: 'bottom' }}>bottom</td><td>multi<br />line<br />cell</td></tr></tbody></table>
      </Cell>
      <Cell name="auto widths by content" wide>
        <table className="t"><tbody><tr><td>x</td><td>a much longer cell of text</td><td>mid size</td></tr></tbody></table>
      </Cell>
      <Cell name="ul disc / ol decimal" wide>
        <div style={{ display: 'flex', gap: 16, fontSize: 12 }}>
          <ul style={{ margin: 0, paddingLeft: 18 }}><li>one</li><li>two</li><li>three</li></ul>
          <ol style={{ margin: 0, paddingLeft: 18 }}><li>one</li><li>two</li><li>three</li></ol>
          <ul style={{ margin: 0, paddingLeft: 18, listStyle: 'square' }}><li>square</li><li>square</li></ul>
          <ol style={{ margin: 0, paddingLeft: 22, listStyle: 'lower-roman' }}><li>roman</li><li>roman</li></ol>
        </div>
      </Cell>
      <Cell name="list-style none / inside" wide>
        <div style={{ display: 'flex', gap: 16, fontSize: 12 }}>
          <ul style={{ margin: 0, padding: 0, listStyle: 'none' }}><li>no</li><li>markers</li></ul>
          <ul style={{ margin: 0, padding: 0, listStylePosition: 'inside', background: '#e5e7eb' }}><li>inside</li><li>inside</li></ul>
        </div>
      </Cell>
      <Cell name="hr / details" wide>
        <div style={{ fontSize: 12 }}>above<hr />below<details><summary>summary</summary>hidden body</details></div>
      </Cell>
    </>
  ),
  selectors: (
    <>
      <Cell name="::before content"><div className="before">arrow before</div></Cell>
      <Cell name="::after content"><div className="after">checked after</div></Cell>
      <Cell name=":nth-child(odd) / first / last"><div className="zebra lorem"><div>one</div><div>two</div><div>three</div><div>four</div></div></Cell>
      <Cell name="+ and ~ siblings"><div className="sib lorem"><div className="on">on</div><div>next</div><div>later</div></div></Cell>
      <Cell name=":has(> .mark)"><div className="hasx lorem" style={{ padding: 4 }}><span className="mark">has mark</span></div></Cell>
      <Cell name=":not(.skip)"><div className="notx lorem"><div>a</div><div className="skip">skip</div><div>c</div></div></Cell>
      <Cell name="counters"><div className="counted lorem"><div>first</div><div>second</div><div>third</div></div></Cell>
      <Cell name="[attr] [attr^=]"><div className="attr lorem"><div data-state="on">state on</div><div data-kind="prefix">prefix kind</div></div></Cell>
      <Cell name="::placeholder color"><input className="placeholder" placeholder="placeholder" style={{ width: 110 }} /></Cell>
      <Cell name="checkbox / radio checked"><span><input type="checkbox" defaultChecked /><input type="checkbox" /><input type="radio" defaultChecked /><input type="radio" /></span></Cell>
      <Cell name="progress / meter"><span style={{ display: 'grid', gap: 4 }}><progress value={70} max={100}>70 %</progress><meter value={0.3} /><meter min={0} max={100} low={30} high={70} optimum={90} value={20} /></span></Cell>
      <Cell name="input text"><input defaultValue="value" style={{ width: 110 }} /></Cell>
      <Cell name="textarea"><textarea defaultValue={'two\nlines'} style={{ width: 110, height: 40 }} /></Cell>
      <Cell name="default input / textarea"><span><input defaultValue="size 20" /><textarea defaultValue="cols 20, rows 2" /></span></Cell>
      <Cell name="field-sizing: content"><textarea defaultValue={'grows\nwith\nits text'} style={{ fieldSizing: 'content', width: 110 } as CSSProperties} /></Cell>
      <Cell name="select"><select defaultValue="b"><option value="a">Alpha</option><option value="b">Bravo</option></select></Cell>
      <Cell name="button default"><button>Button</button> <button disabled>Disabled</button></Cell>
      <Cell name="checkbox / radio"><input type="checkbox" defaultChecked /> <input type="checkbox" /> <input type="radio" name="r" defaultChecked /> <input type="radio" name="r" /></Cell>
      <Cell name="range"><input type="range" defaultValue={30} style={{ width: 110 }} /></Cell>
      <Cell name="progress / meter"><progress value={40} max={100} /> <meter value={0.6} /></Cell>
      <Cell name="styled input"><input defaultValue="rounded" style={{ width: 110, border: '2px solid #2563eb', borderRadius: 8, padding: '4px 8px', background: '#eff6ff' }} /></Cell>
      <Cell name="styled button"><button style={{ background: '#111827', color: '#fff', border: 0, borderRadius: 6, padding: '6px 12px', fontWeight: 600 }}>Styled</button></Cell>
      <Cell name="kbd / code / mark"><span style={{ fontSize: 12 }}><kbd>⌘K</kbd> <code>code()</code> <mark>mark</mark></span></Cell>
      <Cell name="blockquote / small"><blockquote style={{ margin: 0, borderLeft: '3px solid #9ca3af', paddingLeft: 6, fontSize: 12 }}>quoted <small>small</small></blockquote></Cell>
      <Cell name="h1–h3 defaults"><div style={{ all: 'initial' } as CSSProperties}><h3 style={{ margin: 0 }}>H3 heading</h3></div></Cell>
      <Cell name="@media (min-width)"><div className="lorem mq">media query</div></Cell>
    </>
  )
};

const order = Object.keys(pages);

export function App() {
  const name = location.pathname.slice(1) || 'box';
  return (
    <div className="page">
      <h1>{name} · {order.map((p) => (p === name ? `[${p}]` : p)).join(' ')}</h1>
      <div className="cells">{pages[name] ?? <div>no page “{name}”</div>}</div>
    </div>
  );
}
