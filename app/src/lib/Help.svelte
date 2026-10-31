<script lang="ts">
  // The Help window: how the app works, its views, Preferences and the
  // keyboard. Opened from the Help menu; `section` scrolls to one part.
  import { listen } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import { isMac, modLabel } from "./prefs.svelte";

  let { section }: { section?: string } = $props();

  const mod = modLabel;
  const opt = isMac ? "⌥" : "Alt+";
  const shift = isMac ? "⇧" : "Shift+";

  const toc = [
    ["start", "Getting started"],
    ["transfers", "Transfers"],
    ["panel", "Front panel"],
    ["meters", "Meters"],
    ["program", "Program editor"],
    ["multi", "Multi editor"],
    ["windows", "Windows"],
    ["prefs", "Preferences"],
    ["keys", "Keyboard"],
    ["simulator", "Simulator"],
    ["trouble", "Troubleshooting"],
  ];

  let body: HTMLElement;
  let current = $state("start");

  async function go(id: string) {
    current = id;
    await tick();
    body?.querySelector(`#${id}`)?.scrollIntoView({ block: "start" });
  }

  onMount(() => {
    if (section) void go(section);
    const stop = listen<string>("help-section", ({ payload }) => go(payload));
    return () => void stop.then((f) => f());
  });

  const keys: { group: string; rows: [string, string][] }[] = [
    {
      group: "Anywhere",
      rows: [
        [`${mod}1 … ${mod}5`, "Transfers, Front panel, Meters, Program, Multi"],
        [`${mod}${shift}2 … ${mod}${shift}5`, "Open the front panel, meters, a program or a multi in its own window"],
        [`${mod}${shift}1`, "Bring the main window forward"],
        [`${mod},`, "Preferences"],
        [`${mod}+ / ${mod}− / ${mod}0`, "Bigger, smaller, actual size"],
        [`F6 / ${shift}F6, or ${mod}] / ${mod}[`, "Next / previous pane"],
        ["Tab / " + shift + "Tab", "Next / previous panel or control group"],
        [`${mod}${shift}?`, "This help"],
        [`${mod}W`, "Close the window"],
      ],
    },
    {
      group: "Sampler tree and Mac folder",
      rows: [
        ["↑ / ↓", "Move; with " + shift + " extend the selection"],
        ["→ / ←", "Open / close a group or folder (tree)"],
        [`${mod}A`, "Select all"],
        ["Enter", "Copy the selection across (Get or Send)"],
        ["Space", "Play the selected sample on the sampler"],
        ["Delete / Backspace", "Delete the selection (asks first)"],
        ["Double-click", "Copy an item across; open a folder"],
        [`${opt}double-click`, "Open a program or multi in its own editor window"],
      ],
    },
    {
      group: "Editors",
      rows: [
        ["← / →", "Previous / next control in a panel"],
        ["↑ / ↓", `Change the value by one step (${shift} for bigger steps)`],
        ["Page Up / Page Down", "Change the value by a tenth of its range"],
        ["Home / End", "Lowest / highest value"],
        ["Enter or Space", "Type a value (knobs); Enter in a table cell"],
        [`${opt}↑ / ${opt}↓`, "Program: previous / next keygroup. Multi: the part above / below"],
        [`${opt}← / ${opt}→`, "Program: previous / next zone"],
      ],
    },
    {
      group: "Front panel",
      rows: [
        ["← / →", "Cursor keys"],
        ["0 – 9", "Number keys"],
        ["+ / −", "The + and − keys"],
        ["Enter", "Ent/Play"],
        ["Esc", "Exit"],
      ],
    },
  ];
</script>

<div class="help">
  <nav aria-label="Contents">
    <h2 class="sign">Help</h2>
    {#each toc as [id, title]}
      <button class:active={current === id} onclick={() => go(id)}>{title}</button>
    {/each}
  </nav>

  <article bind:this={body}>
    <section id="start">
      <h2>Getting started</h2>
      <p>
        s5000-tools edits and manages an Akai S5000 or S6000 over USB, as Akai's ak.Sys did: copy samples, programs
        and multis between the sampler, its disks and your computer, play the front panel, watch the output meters, and
        edit programs and multis live.
      </p>
      <ol>
        <li>Connect the sampler's USB port to the computer and switch it on.</li>
        <li>
          Quit ak.Sys, and release the sampler from any virtual machine: only one program can use it at a time.
        </li>
        <li>
          The app connects to it when it starts (see <button class="link" onclick={() => go("prefs")}>Preferences</button>).
          Otherwise choose it at the top right and press <b>Connect</b>. The top bar then shows its OS version and how
          much wave and CPU memory is free.
        </li>
      </ol>
      <p>
        Several samplers can be connected at once (<b>+ Add</b> in Transfers). The <b>active</b> one is what the front
        panel, meters and editors work on; <b>Make active</b> in its row changes it.
      </p>
      <p>
        The app follows the sampler: programs, samples and multis created, renamed or deleted on its own front panel
        show up within a moment.
      </p>
    </section>

    <section id="transfers">
      <h2>Transfers <kbd>{mod}1</kbd></h2>
      <p>
        On the left, a tree per sampler: its <b>Multis</b>, <b>Programs</b> and <b>Samples</b> in memory, and its
        <b>Disks</b> with their folders and files. On the right, a folder on your computer (<b>Choose…</b>).
      </p>
      <ul>
        <li>
          Drag between memory, disk folders, the computer's folder, the Finder or Explorer, and other samplers. Or select
          and use <b>Get →</b> and <b>← Send</b>, or double-click.
        </li>
        <li>
          Files go where their type belongs: <code>.wav</code> to Samples, <code>.akp</code> to Programs,
          <code>.akm</code> to Multis, wherever on a sampler you drop them.
        </li>
        <li>
          <b>Include dependencies</b>: a program brings the samples it uses, a multi its programs and their samples. A
          dialog lists them, with those already at the destination unticked.
        </li>
        <li><b>Replace existing</b>: overwrite items of the same name instead of skipping them.</li>
        <li>
          Dragging out to the Finder fetches the items first (they appear in the queue), then the drag starts.
        </li>
        <li>
          Disks: load files into memory, save items to a folder, create folders and delete. Changes on the sampler's
          disk cannot be undone.
        </li>
        <li>The ▶ button on a sample plays it on the sampler.</li>
        <li><b>New program</b>, <b>New multi</b>, <b>Rename</b> and <b>Delete</b> act on the selection.</li>
      </ul>
      <p>The <b>Queue</b> shows each transfer's progress; <b>Clear finished</b> tidies it.</p>
    </section>

    <section id="panel">
      <h2>Front panel <kbd>{mod}2</kbd></h2>
      <p>
        The sampler's live screen, with F1–F8 to its left and F9–F16 to its right, each beside its screen row. Click a
        row of the screen to press that side's key. The other buttons, the data wheel (drag round it, or scroll over it) and the LEDs
        are as on the sampler. The keyboard works too: see <button class="link" onclick={() => go("keys")}>Keyboard</button>.
      </p>
    </section>

    <section id="meters">
      <h2>Meters <kbd>{mod}3</kbd></h2>
      <p>
        Levels of the 16 outputs and the record inputs, in dB below full scale. Outputs follow the sampler's internal
        routing, so outputs without sockets still show signal. <b>Style</b> picks the look, from LED and plasma
        bargraphs to moving-coil VU meters.
      </p>
      <ul>
        <li><b>Peak hold</b> (LED styles) keeps each peak for 1.5 s.</li>
        <li>
          <b>0 VU =</b> (VU styles) is the digital level that reads 0 on the dial: −14, −18 or −20 dBFS. −18 suits
          most material.
        </li>
        <li>Outputs 9–16 need the output expander board; Preferences can hide them.</li>
      </ul>
    </section>

    <section id="program">
      <h2>Program editor <kbd>{mod}4</kbd></h2>
      <p>
        Every program, keygroup and zone setting, in panels coloured by function: pitch blue, amplitude red, filter
        green, LFOs purple. Choose a program in memory to edit it live: changes go to the sampler as you turn a knob or
        drag an envelope point. <b>Open .akp file…</b> shows a saved program, read-only.
      </p>
      <ul>
        <li>Drag a knob up or down; double-click it to type a value.</li>
        <li>
          The keyboard strip shows each keygroup's key range; click one, or use the numbered tabs, to edit that
          keygroup. Zone tabs pick the zone (sample layer).
        </li>
        <li>Values greyed out are not stored in an .akp file; connect to read them live.</li>
      </ul>
    </section>

    <section id="multi">
      <h2>Multi editor <kbd>{mod}5</kbd></h2>
      <p>
        A multi's settings and a table of its parts, like the sampler's Multi page: program, MIDI channel, level, pan,
        output, mute, solo, effects bus and send, transpose, fine tune and key range. Selecting a row shows that part
        as knobs on the right.
      </p>
      <ul>
        <li>Drag a number up or down (Shift for fine steps), or double-click it to type.</li>
        <li><b>Only parts with a program</b> hides empty parts.</li>
        <li><b>New multi…</b> makes one in memory with 32, 64 or 128 parts.</li>
        <li>
          An .akm file opens read-only, and lacks mute, solo and the multi number, which are only read from the
          sampler.
        </li>
      </ul>
    </section>

    <section id="windows">
      <h2>Windows</h2>
      <p>
        Everything works in the one main window, but any view can also have a window of its own, as in ak.Sys: the ⧉
        button beside the view tabs, or the Window menu.
      </p>
      <ul>
        <li>
          The <b>front panel</b> and <b>meters</b> move to their window; the main window's tab says so. <b>Back to
          main window</b> returns them.
        </li>
        <li>
          <b>Program</b> and <b>multi</b> windows: open as many as you like, one per program or multi. Opening one
          from a tab takes its program or multi along, and the tab is free for the next. {opt}double-click a program or
          multi in the tree to open it in a window.
        </li>
        <li>Windows open when the app quits reopen next time, where they were.</li>
        <li>Closing the main window quits the app.</li>
      </ul>
    </section>

    <section id="prefs">
      <h2>Preferences <kbd>{mod},</kbd></h2>
      <dl>
        <dt>Size</dt>
        <dd>
          Scales the whole interface, 80% to 200%, and grows or shrinks the window with it. {mod}+ and {mod}− change
          it from anywhere; {mod}0 resets it.
        </dd>
        <dt>Colour scheme</dt>
        <dd>
          <b>System</b> follows your computer's light or dark setting. Light, Dark, Studio (warm charcoal and amber),
          Ivory (warm paper), High contrast, and four with their own palettes: Le Corbusier, Mies, Neon 80s and
          Cyberpunk.
        </dd>
        <dt>Meters</dt>
        <dd>
          The meter style, with a live preview of each; <b>Show outputs 9–16</b> hides the outputs that need the
          output expander board.
        </dd>
        <dt>Connect to samplers automatically</dt>
        <dd>
          <b>Off</b>: connect with the menu at the top right. <b>When the app starts</b> (the default): every
          S5000/S6000 found on USB. <b>…and whenever one is switched on</b>: also any switched on or plugged in later.
          A sampler you disconnect stays disconnected until it is switched off or unplugged.
        </dd>
      </dl>
      <p>Everything is remembered between runs, as are the window's size and position and the folder on your computer.</p>
    </section>

    <section id="keys">
      <h2>Keyboard</h2>
      <p>
        Everything can be reached from the keyboard. Tab moves between panels; inside one, ← and → move between its
        controls and ↑ and ↓ change the value. A ring shows where the keyboard is.
      </p>
      {#each keys as k}
        <h3>{k.group}</h3>
        <table>
          <tbody>
            {#each k.rows as [key, what]}
              <tr><td><kbd>{key}</kbd></td><td>{what}</td></tr>
            {/each}
          </tbody>
        </table>
      {/each}
    </section>

    <section id="simulator">
      <h2>Simulator</h2>
      <p>
        Choose <b>Simulator</b> instead of a sampler to try the app without hardware. It holds a few demo samples,
        programs, a multi and a disk with folders, and takes transfers. Its screen is blank, its meters are silent, and
        it does not take parameter edits.
      </p>
    </section>

    <section id="trouble">
      <h2>Troubleshooting</h2>
      <dl>
        <dt>The sampler is not listed, or will not connect</dt>
        <dd>
          Check the USB cable and that the sampler has finished starting. Quit ak.Sys and any other program using it,
          and release it from virtual machines. Then switch the sampler off and on.
        </dd>
        <dt>"Lost the sampler"</dt>
        <dd>It stopped answering: switched off, unplugged, or busy. <b>Reconnect</b> in its row tries again.</dd>
        <dt>A transfer failed</dt>
        <dd>
          The queue shows why. The sampler may be out of wave memory (top bar), or an item of that name may exist with
          Replace existing off.
        </dd>
        <dt>MIDI does nothing</dt>
        <dd>
          The app talks to the sampler over USB only; MIDI goes through its MIDI IN socket. Check the cable and the
          sampler's MIDI settings.
        </dd>
      </dl>
      <p>Report problems at <code>github.com/tbxx/s5000-tools/issues</code> (Help menu → Report a problem…).</p>
    </section>
  </article>
</div>

<style>
  .help {
    display: grid;
    grid-template-columns: 180px 1fr;
    height: 100%;
    min-height: 0;
  }
  nav {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--rule);
    overflow: auto;
  }
  nav h2 {
    margin: 0 0 6px;
    font-size: 14px;
    padding: 6px 12px;
  }
  nav button {
    border: none;
    background: transparent;
    text-align: left;
    font-weight: 400;
    padding: 4px 12px;
  }
  nav button.active {
    font-weight: 700;
    box-shadow: inset 3px 0 var(--accent);
  }
  article {
    overflow: auto;
    padding: 4px 24px 40px;
    line-height: 1.5;
    max-width: 760px;
    user-select: text;
    -webkit-user-select: text;
  }
  section {
    padding-top: 12px;
    border-bottom: 1px solid var(--rule-soft);
    padding-bottom: 12px;
  }
  h2 {
    font-size: 18px;
    margin: 8px 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  h3 {
    font-size: 13px;
    margin: 14px 0 6px;
  }
  dt {
    font-weight: 700;
    margin-top: 8px;
  }
  dd {
    margin: 2px 0 0 0;
  }
  kbd {
    font: 11px var(--mono);
    border: 1px solid var(--rule);
    padding: 0 4px;
    white-space: nowrap;
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  td {
    padding: 4px 8px 4px 0;
    border-bottom: 1px solid var(--rule-soft);
    vertical-align: top;
  }
  td:first-child {
    width: 40%;
  }
  .link {
    border: none;
    background: none;
    padding: 0;
    font: inherit;
    color: var(--accent);
    text-decoration: underline;
  }
  @media (max-width: 600px) {
    .help {
      grid-template-columns: 1fr;
    }
    nav {
      display: none;
    }
  }
</style>
