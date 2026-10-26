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

  private open(r: Omit<Request, "resolve">): Promise<string[] | null> {
    return new Promise((resolve) => {
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
  }
}

export const dialogs = new Dialogs();
