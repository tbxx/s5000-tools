// Promise-based confirm and prompt dialogs, shown by Dialog.svelte.

export interface Field {
  label: string;
  value: string;
  /** Choices for a select; a text input otherwise. */
  options?: string[];
}

interface Request {
  title: string;
  text: string;
  ok: string;
  danger: boolean;
  fields: Field[];
  resolve: (values: string[] | null) => void;
}

class Dialogs {
  current = $state<Request | null>(null);
  // Where focus was when the dialog opened, to go back to when it closes;
  // otherwise it is left on the page and Tab starts again from the top.
  private returnTo: HTMLElement | null = null;

  private open(r: Omit<Request, "resolve">): Promise<string[] | null> {
    return new Promise((resolve) => {
      if (!this.current) this.returnTo = document.activeElement as HTMLElement | null;
      this.current = { ...r, resolve };
    });
  }

  async confirm(title: string, text: string, ok = "OK", danger = false): Promise<boolean> {
    return (await this.open({ title, text, ok, danger, fields: [] })) !== null;
  }

  ask(title: string, fields: Field[], ok = "OK", text = ""): Promise<string[] | null> {
    return this.open({ title, text, ok, danger: false, fields });
  }

  close(values: string[] | null) {
    this.current?.resolve(values);
    this.current = null;
    const el = this.returnTo;
    this.returnTo = null;
    if (el?.isConnected) requestAnimationFrame(() => el.focus({ preventScroll: true }));
  }
}

export const dialogs = new Dialogs();
