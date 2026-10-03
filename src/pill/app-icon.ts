const palette = ['#38566B', '#665078', '#45705B', '#7C593C', '#555C88', '#765365'] as const;

/** Stable app identity colour; only runs when the dictation context changes. */
export function appColour(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (Math.imul(hash, 31) + name.charCodeAt(i)) >>> 0;
  return palette[hash % palette.length];
}

export class AppIcon {
  private generation = 0;
  private readonly image: HTMLImageElement;
  private readonly letter: HTMLElement;
  constructor(private readonly tile: HTMLElement) {
    this.image = tile.querySelector<HTMLImageElement>('img')!;
    this.letter = tile.querySelector<HTMLElement>('.letter')!;
    this.set('', null, async () => null);
  }
  set(name: string, key: string | null, load: (key: string) => Promise<unknown>) {
    const generation = ++this.generation;
    this.image.onload = this.image.onerror = null;
    this.fallback();
    this.letter.textContent = Array.from(name.trim())[0]?.toUpperCase() ?? 'V';
    this.tile.style.backgroundColor = appColour(name);
    if (!key) return;
    void load(key).then(src => {
      if (generation !== this.generation || typeof src !== 'string' || !/^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(src)) return;
      this.image.onload = () => {
        if (generation !== this.generation) return;
        if (!this.image.naturalWidth) { this.fallback(); return; }
        this.image.hidden = false; this.letter.hidden = true;
      };
      this.image.onerror = () => { if (generation === this.generation) this.fallback(); };
      // Keep the fallback visible until decoding has succeeded, including cached images.
      this.image.src = src;
    }).catch(() => {});
  }
  private fallback() {
    this.image.hidden = true;
    this.image.removeAttribute('src');
    this.letter.hidden = false;
  }
  destroy() {
    this.generation++;
    this.image.onload = this.image.onerror = null;
  }
}
