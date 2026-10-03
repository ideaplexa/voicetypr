/** The v3.2 island glyph set. Static markup only; user text is always textContent. */
export type IslandIcon = 'star' | 'bang' | 'chevron' | 'close' | 'clock' | 'mic' | 'engine' | 'globe';
const paths: Record<IslandIcon,string> = {
  star: '<path d="M8 1.4C8.6 5.5 10.5 7.4 14.6 8 10.5 8.6 8.6 10.5 8 14.6 7.4 10.5 5.5 8.6 1.4 8 5.5 7.4 7.4 5.5 8 1.4Z"/>',
  bang: '<path d="M8 2.6v6.6"/><circle cx="8" cy="12.9" r=".9" fill="currentColor" stroke="none"/>',
  chevron: '<path d="M4.4 6.2 8 9.8l3.6-3.6"/>',
  close: '<path d="M4.6 4.6l6.8 6.8M11.4 4.6l-6.8 6.8"/>',
  clock: '<circle cx="8" cy="8" r="6.2"/><path d="M8 4.6V8l2.3 1.6"/>',
  mic: '<rect x="5.6" y="1.8" width="4.8" height="8" rx="2.4"/><path d="M3.4 7.6a4.6 4.6 0 0 0 9.2 0M8 12.2v2"/>',
  engine: '<path d="M2.6 7v2M5.3 5v6M8 2.8v10.4M10.7 5v6M13.4 7v2"/>',
  globe: '<circle cx="8" cy="8" r="6.2"/><path d="M1.8 8h12.4M8 1.8c2.1 2.2 2.1 10.2 0 12.4M8 1.8c-2.1 2.2-2.1 10.2 0 12.4"/>',
};
export const icon = (name: IslandIcon, size: number) => `<svg width="${size}" height="${size}" viewBox="0 0 16 16" fill="${name === 'star' ? 'currentColor' : 'none'}" stroke="${name === 'star' ? 'none' : 'currentColor'}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths[name]}</svg>`;
