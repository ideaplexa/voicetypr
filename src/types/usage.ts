/** Content-free aggregates over the entire local history store. */
export interface UsageDay {
  date: string;
  words: number;
  dictations: number;
  audio_ms: number;
}
export interface UsageApp {
  name: string;
  words: number;
}
export interface UsageStats {
  /** Local YYYY-MM-DD of the oldest history entry. */
  first_use: string | null;
  total_words: number;
  total_dictations: number;
  total_audio_ms: number;
  polished_dictations: number;
  /** Active local days, ascending. */
  days: UsageDay[];
  /** Top 8 apps by words; remaining apps folded into Other. */
  apps: UsageApp[];
}

/** get_usage_stats args: inclusive local start day filters ONLY apps; end is today. */
export interface UsageStatsArgs {
  since?: string | null;
}
