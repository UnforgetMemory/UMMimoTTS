/**
 * Collects performance metrics from Playwright page interactions.
 * Tracks API response times, console errors, and user journey timings.
 */
export class MetricsCollector {
  private timings: Map<string, number[]> = new Map();
  private errors: string[] = [];
  private startTime: number = 0;

  /** Start timing */
  start() {
    this.startTime = Date.now();
    this.timings.clear();
    this.errors = [];
  }

  /** Record a named timing */
  record(name: string, ms: number) {
    const arr = this.timings.get(name) ?? [];
    arr.push(ms);
    this.timings.set(name, arr);
  }

  /** Record a console error */
  recordError(msg: string) {
    this.errors.push(msg);
  }

  /** Get elapsed time since start */
  elapsed(): number {
    return Date.now() - this.startTime;
  }

  /** Get all recorded errors */
  getErrors(): string[] {
    return [...this.errors];
  }

  /** Get timing stats for a named metric */
  getStats(name: string): { avg: number; p50: number; p95: number; count: number } | null {
    const arr = this.timings.get(name);
    if (!arr || arr.length === 0) return null;
    const sorted = [...arr].sort((a, b) => a - b);
    const avg = sorted.reduce((a, b) => a + b, 0) / sorted.length;
    const p50 = sorted[Math.floor(sorted.length * 0.5)];
    const p95 = sorted[Math.floor(sorted.length * 0.95)];
    return { avg, p50, p95, count: sorted.length };
  }

  /** Generate a summary report string */
  summary(): string {
    const lines: string[] = [];
    lines.push(`  elapsed: ${this.elapsed()}ms`);
    lines.push(`  errors: ${this.errors.length}`);
    for (const [name] of this.timings) {
      const s = this.getStats(name);
      if (s) {
        lines.push(`  ${name}: n=${s.count} avg=${s.avg.toFixed(0)}ms p50=${s.p50}ms p95=${s.p95}ms`);
      }
    }
    return lines.join('\n');
  }
}

/** Result of a single simulated user journey */
export interface UserJourneyResult {
  userId: number;
  success: boolean;
  duration: number;
  errors: string[];
  steps: Record<string, number>;
}
