import { afterEach, describe, expect, it, vi } from 'vitest';
import { AppIcon, appColour } from '@/pill/app-icon';
import { pillIconPng } from '@/ui-preview/pill-icon';

const tile = document.createElement('div');
let icon: AppIcon;
function mount() {
  tile.innerHTML = '<span class="letter"></span><img hidden alt="" />';
  icon = new AppIcon(tile);
  return { image: tile.querySelector('img')!, letter: tile.querySelector<HTMLElement>('.letter')! };
}
afterEach(() => { icon.destroy(); vi.restoreAllMocks(); });
describe('app icon fallback', () => {
  it.each([null, '', '/missing.png', 42, {}, 'data:image/png;base64,invalid!'])('keeps the letter tile for invalid response %s', async value => {
    const { image, letter } = mount();
    icon.set('Editor', 'key', async () => value); await Promise.resolve();
    expect(image.hidden).toBe(true); expect(image.hasAttribute('src')).toBe(false);
    expect(letter.hidden).toBe(false); expect(letter.textContent).toBe('E');
    expect(tile.style.backgroundColor).not.toBe('');
  });
  it('keeps the tile visible until a valid image loads and restores it on error', async () => {
    const { image, letter } = mount();
    icon.set('Editor', 'key', async () => pillIconPng); await Promise.resolve();
    expect(image.hidden).toBe(true); expect(letter.hidden).toBe(false);
    vi.spyOn(image, 'naturalWidth', 'get').mockReturnValue(64);
    image.dispatchEvent(new Event('load'));
    expect(image.hidden).toBe(false); expect(letter.hidden).toBe(true);
    image.dispatchEvent(new Event('error'));
    expect(image.hidden).toBe(true); expect(letter.hidden).toBe(false);
    expect(image.hasAttribute('src')).toBe(false);
  });
  it('keeps the tile when the icon command rejects', async () => {
    const { image, letter } = mount();
    icon.set('Terminal', 'key', async () => { throw new Error('unavailable'); });
    await Promise.resolve(); await Promise.resolve();
    expect(image.hidden).toBe(true); expect(letter.textContent).toBe('T'); expect(letter.hidden).toBe(false);
  });
  it('ignores late responses from the previous app and after destruction', async () => {
    const { image, letter } = mount();
    let resolve!: (value: unknown) => void;
    icon.set('Editor', 'old', () => new Promise(done => { resolve = done; }));
    icon.set('Terminal', null, async () => null); resolve(pillIconPng); await Promise.resolve();
    expect(image.hasAttribute('src')).toBe(false); expect(letter.textContent).toBe('T');
    icon.set('Editor', 'new', () => new Promise(done => { resolve = done; }));
    icon.destroy(); resolve(pillIconPng); await Promise.resolve(); expect(image.hasAttribute('src')).toBe(false);
  });
  it('uses a deterministic fixed palette for app names', () => {
    mount(); expect(appColour('Editor')).toBe(appColour('Editor'));
    expect(new Set(Array.from({length:50},(_,i) => appColour(`App ${i}`))).size).toBe(6);
  });
});
