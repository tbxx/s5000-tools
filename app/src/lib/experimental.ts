// Disk operations other than listing disks and folders are implemented from
// Akai's spec but untested on hardware, and one documented disk command is
// known to restart an S5000 (docs/protocol.md §13). The first time a disk
// would be written to or loaded from, the user is asked to accept that.

import { dialogs } from "./dialogs.svelte";
import { app } from "./state.svelte";

export async function acceptDiskOps(): Promise<boolean> {
  if (app.settings.diskOpsAccepted) return true;
  const ok = await dialogs.confirm(
    "Disk operations are experimental",
    "Loading from, saving to and deleting on the sampler's disks follow Akai's specification but have not been tested on " +
      "hardware yet. One documented disk command is known to restart an S5000.\n\n" +
      "Keep a backup of anything on the disk you care about. Please report what happens at " +
      "github.com/tbxx/s5000-tools/issues.",
    "Continue",
  );
  if (ok) app.save({ diskOpsAccepted: true });
  return ok;
}
