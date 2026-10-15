import { describe, expect, it } from 'vitest';
import { islandShowsLiveWords } from '@/pill/contracts';

describe('islandShowsLiveWords', () => {
  it('needs streaming on', () => {
    expect(islandShowsLiveWords({})).toBe(false);
    expect(islandShowsLiveWords({ transcription_mode: 'regular', island_live_words: true })).toBe(false);
  });
  it('shows words when streaming is on and the island switch is on or unset', () => {
    expect(islandShowsLiveWords({ transcription_mode: 'live_preview' })).toBe(true);
    expect(islandShowsLiveWords({ streaming_preview_enabled: true, island_live_words: true })).toBe(true);
  });
  it('stays compact when the island switch is off', () => {
    expect(islandShowsLiveWords({ transcription_mode: 'live_preview', island_live_words: false })).toBe(false);
  });
});
