/** Missing/invalid History dates stay unknown. Numeric strings are not dates:
 * Date otherwise interprets short ones as years/months.
 */
export function parseHistoryDate(timestamp: unknown): Date | null {
  if (
    !(timestamp instanceof Date) &&
    (typeof timestamp !== "string" || !timestamp.trim() || /^[+-]?\d+(\.\d+)?$/.test(timestamp.trim()))
  ) {
    return null;
  }
  const date = new Date(timestamp);
  return Number.isFinite(date.getTime()) ? date : null;
}
