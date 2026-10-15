/**
 * Island row zones (plan 082): the app icon anchors the left, the main thing sits in the middle
 * and the time or action takes the right. Pure arithmetic; nothing here reads the DOM.
 */
export const ZONE = {
  /** Right edge of the app icon inside the content box. */
  iconEdge: 30,
  /** Space between the middle zone and the icon / the timer. */
  gap: 8,
  /** Row padding right of the timer. */
  pad: 12,
  /** The surface border, both sides. */
  border: 2,
  /** One digit of the 12 px mono timer. */
  mono: 7.2,
  /** Result glyph (check, clipboard, mark) plus its gap to the text. */
  glyph: 26,
  /** Where a result glyph sits when the group is as wide as the row. */
  glyphMin: 34,
} as const;

/** The timer reserves four digits and grows with longer times (10:00, 1:00:00). */
export const timerChars = (text: string) => Math.max(4, text.length);

/**
 * The waveform strip: it starts one gap right of the icon and ends one gap left of the timer,
 * so the wave is centred between them. `width` is the surface width.
 */
export function waveZone(width: number, timerText: string) {
  const x = ZONE.iconEdge + ZONE.gap;
  const right = width - ZONE.border - ZONE.pad - timerChars(timerText) * ZONE.mono - ZONE.gap;
  return { x, width: Math.max(0, right - x) };
}

/**
 * Left edge of a result glyph whose group (glyph + text) is centred in a row of `rowWidth`,
 * never closer to the app icon than `glyphMin`.
 */
export const groupLeft = (rowWidth: number, groupWidth: number) =>
  Math.max(ZONE.glyphMin, Math.round((rowWidth - groupWidth) / 2));
