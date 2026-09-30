// Stroke recorder for the keybinding editor: a second stroke pressed within
// `windowMs` (or after `armChord()`) extends the first into a two-step chord.

export const CHORD_WINDOW_MS = 1200;

export class ChordRecorder {
  strokes: string[] = [];
  private last = 0;
  private armed = false;

  constructor(
    private windowMs = CHORD_WINDOW_MS,
    private now: () => number = () => Date.now(),
  ) {}

  /** Record a normalized stroke; returns the current sequence. */
  push(stroke: string): string[] {
    const t = this.now();
    const inWindow = this.strokes.length === 1 && (this.armed || t - this.last <= this.windowMs);
    this.strokes = inWindow ? [this.strokes[0], stroke] : [stroke];
    this.armed = false;
    this.last = t;
    return this.strokes;
  }

  /** Next stroke becomes the second step of a chord regardless of timing. */
  armChord() {
    if (this.strokes.length === 1) this.armed = true;
  }

  get chordArmed(): boolean {
    return this.armed;
  }

  clear() {
    this.strokes = [];
    this.armed = false;
    this.last = 0;
  }

  get key(): string {
    return this.strokes.join(' ');
  }
}
