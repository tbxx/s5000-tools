# Capturing ak.Sys traffic

Everything in [protocol.md](protocol.md) was found by recording what ak.Sys
sends and receives. To extend it (e.g. disk operations), capture the same way.

## Setup

ak.Sys needs 32-bit Windows (XP or 7) with Akai's 32-bit USB driver. On an
Apple Silicon Mac this works in [UTM](https://mac.getutm.app) with x86
emulation:

- **System:** x86_64, 2 CPU cores. Leave "Force Multicore" **off**: with it
  on, attaching the S5000 crashed Windows.
- **USB:** USB 2.0, with USB sharing on. Pass the S5000 through from UTM's
  toolbar.
- **Disk:** an existing VMware VM can be reused. Merge its snapshots into one
  disk with `vmware-vdiskmanager -r <disk>.vmdk -t 0 out.vmdk` and import it.
  If Windows was installed on VMware's LSI SAS controller, attach the disk to
  QEMU's `mptsas1068` with custom QEMU arguments instead of IDE, or Windows
  will not boot (STOP 0x7B).

## Recording

1. Install [USBPcap](https://desowin.org/usbpcap/) in Windows.
2. Run `USBPcapCMD.exe` as administrator. Pick the root hub that lists the
   S5000 and write the capture to a shared folder.
3. Do **one** thing per capture and keep it short. Leave a couple of seconds
   between actions and write down what you did, in order.
4. Use distinctive values (e.g. 77, not 1), and where possible save the edited
   item to a file afterwards. Diffing the saved file against the original
   shows where each value is stored.

## Decoding

```
python3 tools/usbpcap_dump.py capture.pcap --quiet
python3 tools/panel_frames.py capture.pcap frames/
```

`--quiet` hides the constant polling and collapses bulk data.

Captures contain whatever was in the sampler, including samples and programs
that may belong to someone else. Don't commit them.
