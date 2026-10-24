/**
 * One local model generates at a time. oMLX shares this Mac's memory, so
 * several different local models answering at once can exhaust it; calls to
 * the model already running may join it, and cloud models are never gated.
 */
export class LocalModelGate {
  private active?: string;
  private running = 0;
  private waiters: Array<() => void> = [];

  async acquire(modelKey: string): Promise<() => void> {
    while (this.running > 0 && this.active !== modelKey) {
      await new Promise<void>((resolve) => this.waiters.push(resolve));
    }

    this.active = modelKey;
    this.running += 1;
    let released = false;

    return () => {
      if (released) return;
      released = true;
      this.running -= 1;

      if (this.running === 0) {
        this.active = undefined;
        this.waiters.splice(0).forEach((wake) => wake());
      }
    };
  }
}

export const localModelGate = new LocalModelGate();
