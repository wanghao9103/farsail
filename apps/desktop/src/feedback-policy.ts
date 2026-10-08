// Pure state policies; no browser or native input is needed to verify them.
export class RefreshFailureGate {
  private last: string | null = null;
  shouldReport(error: string, foreground: boolean) {
    const repeated = this.last === error;
    this.last = error;
    return foreground || !repeated;
  }
  recovered() {
    this.last = null;
  }
}

export class DialogKeyboardGate {
  private held = new Map<string, boolean>();
  press(code: string, repeat = false) {
    if (!repeat) this.held.set(code, true);
    else if (!this.held.has(code)) this.held.set(code, false);
  }
  suppress(code: string, phase: "down" | "up", insideDialog: boolean) {
    const beganInDialog = this.held.get(code);
    if (beganInDialog === undefined) return false;
    if (phase === "up") this.held.delete(code);
    // Allow native button activation/navigation inside the dialog. Once it
    // closes, consume both repeats and release before focus can relay them.
    return !insideDialog || !beganInDialog;
  }
  reset() {
    this.held.clear();
  }
}
