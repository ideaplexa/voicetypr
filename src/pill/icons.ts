/** The v3.2 island glyph set. Static markup only; user text is always textContent. */
export type IslandIcon = 'star' | 'bang' | 'chevron' | 'close' | 'clock' | 'mic' | 'engine' | 'globe' | 'mic-off' | 'clipboard' | 'quiet' | 'cpu' | 'gear' | 'back';
const paths: Record<IslandIcon,string> = {
  back: '<path d="M10 3.5 5.5 8 10 12.5"/>',
  gear: '<path d="m6.5 1.5-.5 2-1.5.9-2-.5-1.5 2.6 1.5 1.4v1.8L1 11.1l1.5 2.6 2-.5 1.5.9.5 2h3l.5-2 1.5-.9 2 .5 1.5-2.6-1.5-1.4V7.9L15 6.5l-1.5-2.6-2 .5-1.5-.9-.5-2Z"/><circle cx="8" cy="8.5" r="2.3"/>',
  star: '<path d="M8 1.4C8.6 5.5 10.5 7.4 14.6 8 10.5 8.6 8.6 10.5 8 14.6 7.4 10.5 5.5 8.6 1.4 8 5.5 7.4 7.4 5.5 8 1.4Z"/>',
  bang: '<path d="M8 2.6v6.6"/><circle cx="8" cy="12.9" r=".9" fill="currentColor" stroke="none"/>',
  chevron: '<path d="M4.4 6.2 8 9.8l3.6-3.6"/>',
  close: '<path d="M4.6 4.6l6.8 6.8M11.4 4.6l-6.8 6.8"/>',
  clock: '<circle cx="8" cy="8" r="6.2"/><path d="M8 4.6V8l2.3 1.6"/>',
  mic: '<rect x="5.6" y="1.8" width="4.8" height="8" rx="2.4"/><path d="M3.4 7.6a4.6 4.6 0 0 0 9.2 0M8 12.2v2"/>',
  engine: '<path d="M2.6 7v2M5.3 5v6M8 2.8v10.4M10.7 5v6M13.4 7v2"/>',
  'mic-off': '<path d="M2 2l12 12M5.6 5.6V4.2a2.4 2.4 0 0 1 4.8 0V8M3.4 7.6a4.6 4.6 0 0 0 7.6 3.5M12.6 7.6v.8M8 12.2v2"/>',
  clipboard: '<rect x="3.5" y="3.2" width="9" height="11" rx="2"/><rect x="5.5" y="1.8" width="5" height="3" rx="1"/><path d="M6 8h4M6 10.5h3"/>',
  quiet: '<circle cx="8" cy="8" r="5.8" stroke-dasharray="2 2.5"/>',
  cpu: '<rect x="4" y="4" width="8" height="8" rx="1.5"/><path d="M6 1.5V4M10 1.5V4M6 12v2.5M10 12v2.5M1.5 6H4M1.5 10H4M12 6h2.5M12 10h2.5"/>',
  globe: '<circle cx="8" cy="8" r="6.2"/><path d="M1.8 8h12.4M8 1.8c2.1 2.2 2.1 10.2 0 12.4M8 1.8c-2.1 2.2-2.1 10.2 0 12.4"/>',
};
export const icon = (name: IslandIcon, size: number) => `<svg width="${size}" height="${size}" viewBox="0 0 16 16" fill="${name === 'star' ? 'currentColor' : 'none'}" stroke="${name === 'star' ? 'none' : 'currentColor'}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths[name]}</svg>`;
