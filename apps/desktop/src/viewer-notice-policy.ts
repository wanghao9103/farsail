// A window instance survives session reconnects. Closure is monotonic, including
// when multiple status reads or success callbacks complete out of order.
export class ViewerNoticeLease {
  readonly instance: string;
  private present = true;
  constructor(instance: string) {
    this.instance = instance;
  }
  isPresent() {
    return this.present;
  }
  async check(read: (instance: string) => Promise<boolean>) {
    if (!this.present) return false;
    try {
      if ((await read(this.instance)) === false) this.present = false;
    } catch {
      /* An unknown status is not proof of a closed window. */
    }
    return this.present;
  }
}
export async function inspectViewerNotice(
  lease: ViewerNoticeLease,
  read: (instance: string) => Promise<boolean>,
  current: () => boolean,
  closed: (lease: ViewerNoticeLease) => void,
) {
  if (!current()) return;
  if (!(await lease.check(read)) && current()) closed(lease);
}
export async function publishViewerNotice(
  lease: ViewerNoticeLease,
  read: (instance: string) => Promise<boolean>,
  current: () => boolean,
  publish: (lease: ViewerNoticeLease | null) => void,
) {
  if (!current()) return;
  const present = await lease.check(read);
  if (current()) publish(present ? lease : null);
}
