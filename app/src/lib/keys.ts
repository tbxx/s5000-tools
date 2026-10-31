// Keyboard navigation shared by the views.
//
// - A rove group (`use:rove`): ← and → move between its controls; ↑ and ↓
//   are left to the control (knobs and number cells step their value).
//   Groups with `{ tab: "each" }` (panels, the parts table, the mixer's
//   strips) leave Tab visiting every control in reading order; others (the
//   keygroup tabs) are one Tab stop, entered at the control last used.
// - Panes (`data-pane`) are the large regions of a view; ⌘[ and ⌘] (Ctrl
//   elsewhere), or F6 and Shift+F6, move focus between them.

const CONTROLS = '[role="slider"], [role="spinbutton"], select, button, input, [data-rove-item]';

function items(group: HTMLElement): HTMLElement[] {
  return [...group.querySelectorAll<HTMLElement>(CONTROLS)].filter(
    (el) => !(el as HTMLButtonElement).disabled && el.closest("[data-rove]") === group && el.offsetParent !== null,
  );
}

/** ← and → move between `group`'s controls. Unless `tab` is "each", the
 *  group is also one Tab stop. */
export function rove(group: HTMLElement, opts: { tab?: "each" | "one" } = {}) {
  group.dataset.rove = "";
  let current: HTMLElement | null = null;

  function sync() {
    if (opts.tab === "each") return;
    const all = items(group);
    if (!current || !all.includes(current)) current = all[0] ?? null;
    for (const el of all) {
      const t = el === current ? 0 : -1;
      if (el.tabIndex !== t) el.tabIndex = t;
    }
  }

  function focusin(e: FocusEvent) {
    const el = (e.target as HTMLElement).closest<HTMLElement>(CONTROLS);
    if (el && items(group).includes(el) && el !== current) {
      current = el;
      sync();
    }
  }

  function keydown(e: KeyboardEvent) {
    if (e.altKey || e.metaKey || e.ctrlKey) return;
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    const target = e.target as HTMLElement;
    // Text fields keep ← and → for the caret.
    if (target instanceof HTMLInputElement && target.type !== "checkbox" && target.type !== "radio") return;
    const all = items(group);
    const i = all.indexOf(target.closest<HTMLElement>(CONTROLS)!);
    if (i < 0) return;
    const next = all[i + (e.key === "ArrowRight" ? 1 : -1)];
    if (!next) return;
    e.preventDefault();
    e.stopPropagation();
    current = next;
    sync();
    next.focus();
  }

  sync();
  // Groups can change often; sync once a frame.
  let queued = false;
  const observer = new MutationObserver(() => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      sync();
    });
  });
  observer.observe(group, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled", "tabindex"] });
  group.addEventListener("focusin", focusin);
  group.addEventListener("keydown", keydown);
  return {
    destroy() {
      observer.disconnect();
      group.removeEventListener("focusin", focusin);
      group.removeEventListener("keydown", keydown);
    },
  };
}

/** Moves focus to the next (1) or previous (-1) pane in the document. */
export function cyclePane(dir: 1 | -1) {
  const panes = [...document.querySelectorAll<HTMLElement>("[data-pane]")].filter((p) => p.offsetParent !== null);
  if (!panes.length) return;
  const at = panes.findIndex((p) => p.contains(document.activeElement));
  const pane = panes[(at + dir + panes.length) % panes.length] ?? panes[0];
  focusPane(pane);
}

function focusPane(pane: HTMLElement) {
  if (pane.tabIndex >= 0 && pane.matches("[tabindex]")) return pane.focus();
  const first = pane.querySelector<HTMLElement>(
    '[tabindex="0"], button:not([disabled]):not([tabindex="-1"]), select:not([disabled]):not([tabindex="-1"]), input:not([disabled]):not([tabindex="-1"])',
  );
  first?.focus();
}
