import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
const css = readFileSync('src/pill.css','utf8');
describe('RecordingPill CSS contracts', () => {
  it('keeps the hidden guard authoritative for all island elements', () => { expect(css).toMatch(/\.pill-surface\[hidden\][^}]*\.pill-root \[hidden\][^}]*display:none!important/); });
  it('uses v3.2 double hairline, breathing period, target sizes and tentative alpha', () => { for (const token of ['#121316','rgba(0,0,0,.45)','rgba(255,255,255,.16)','0 12px 34px','3.2s','width:26px','rgba(255,255,255,.52)','Geist Mono Variable']) expect(css).toContain(token); });
  it('keeps card rows bounded, wraps subtext, and shows only back titles', () => {
    expect(css).toMatch(/\.feedback-card \{[^}]*max-width:360px/);
    expect(css).toMatch(/\.done \{[^}]*flex-direction:column/);
    expect(css).toMatch(/\.done-actions \{[^}]*justify-content:flex-start/);
    expect(css).toMatch(/\.card-bottom \{[^}]*flex-wrap:wrap/);
    expect(css).toMatch(/\.card-bottom small \{[^}]*white-space:normal;text-wrap:pretty/);
    expect(css).toMatch(/\.feedback-card\.back \{[^}]*align-content:start;align-items:start/);
    expect(css).toMatch(/\.feedback-card\.back > :not\(\.card-text\) \{ visibility:hidden/);
    expect(css).toMatch(/\.feedback-card\.back \.card-text \{ opacity:\.85;transform:translateY\(-7px\)/);
  });
  it('retains the ltr isolate within the rtl clipping box and turns off breathing for reduced motion', () => { expect(css).toMatch(/\.pill-preview-line \{ direction:ltr;unicode-bidi:isolate/); expect(css).toContain('prefers-reduced-motion:reduce'); expect(css).toContain('animation:none'); });
});
