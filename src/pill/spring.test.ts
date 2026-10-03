import { expect, it } from 'vitest';
import { Spring } from '@/pill/spring';
it('uses the exact lab response and damping', () => { const s = new Spring(12); expect(s.k).toBe((2*Math.PI/.34)**2); expect(s.z).toBe(.78); });
it('moves on the first frame without an opening dip', () => { const s = new Spring(12); s.to = 236; s.step(1/60); expect(s.x).toBeGreaterThan(12); });
it('settles exactly without an idle loop', () => { const s = new Spring(12); s.to=236; for(let i=0;i<300;i++) s.step(1/60); expect(s.x).toBe(236); expect(s.v).toBe(0); expect(s.step(1/60)).toBe(false); });
it('preserves position and velocity on an interrupted target', () => { const s=new Spring(12); s.to=300; s.step(.1); const x=s.x,v=s.v; s.to=236; expect(s.x).toBe(x); expect(s.v).toBe(v); for(let i=0;i<300;i++) s.step(1/60); expect(s.x).toBe(236); });
it('is deterministic across calls for the same input and snaps under reduced motion', () => { const a=new Spring(12),b=new Spring(12); a.to=b.to=300; for(let i=0;i<20;i++) { a.step(.016);b.step(.016); } expect(a.x).toBe(b.x); a.step(.016,true); expect(a.x).toBe(300); expect(a.v).toBe(0); });
