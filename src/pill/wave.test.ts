import { expect, it } from 'vitest';
import { WaveBuffer } from '@/pill/wave';
it('starts empty and emits only elapsed 70 ms slices', () => { const w=new WaveBuffer(); expect(w.count).toBe(0); w.sample(.8); w.advance(.069,.069); expect(w.count).toBe(0); w.advance(.07,.001); expect(w.count).toBe(1); expect(w.times[0]).toBe(.07); expect(w.levels[0]).toBeCloseTo(.8); });
it('keeps the peak of the interval instead of the final sample', () => { const w=new WaveBuffer(); w.sample(.8);w.sample(.2);w.advance(.07,.07); expect(w.levels[0]).toBeCloseTo(.8);w.sample(0);w.advance(.14,.07);expect(w.levels[1]).toBe(0); });
it('is deterministic for the same timestamped levels', () => { const a=new WaveBuffer(),b=new WaveBuffer(); for(let i=0;i<120;i++) {const level=(i%9)/10; for(const w of [a,b]) {w.sample(level);w.advance(i/60,1/60);} } expect(a.times).toEqual(b.times);expect(a.levels).toEqual(b.levels); });
it('bounds storage and resets without inventing silence history', () => { const w=new WaveBuffer();w.advance(20,1/60);expect(w.count).toBe(96);expect(w.times.length).toBe(96);w.reset();expect(w.count).toBe(0); });
it('uses the lab 30 ms attack and 180 ms release', () => { const w=new WaveBuffer();w.sample(1);w.advance(.03,.03);expect(w.envelope).toBeCloseTo(1-Math.exp(-1));const peak=w.envelope;w.sample(0);w.advance(.21,.18);expect(w.envelope).toBeCloseTo(peak*Math.exp(-1)); });
