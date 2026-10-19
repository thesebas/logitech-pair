# h800pair

Pair Logitech wireless headsets (H800, H600, UE3500) with their USB Nano
receiver, from the command line. macOS/Linux/Windows.

The pairing protocol was reverse-engineered from Logitech's 2012
"Wireless Headset Pairing Assistant" (`Pairing.exe`): the receiver exposes a
vendor HID interface (usage page `0xFFA0`) that takes 3-byte output reports
`[0x02, CMD, 0x00]` — `0x08` opens the pairing window, `0x80` polls status,
`0x01` closes it. Status byte: `0x20` idle · `0x04` window open · `0x10`
linking · `0x40` connected.

## Usage

```sh
# one-off, no install:
uv run h800pair.py

# or from a GitHub clone:
uvx --from git+https://github.com/YOURUSER/h800pair h800pair
```

Plug in the receiver, run the script, and follow the printed steps:
turn the headset on, set its device-selection switch to the free slot
if it was paired before (an already-paired slot will not re-pair), then
enter pairing mode with the model-specific ritual:

| Model  | Ritual (hold until the light blinks fast)      |
|--------|------------------------------------------------|
| H800   | next-track + volume-up                         |
| H600   | volume-up + mic-mute                           |
| UE3500 | power button held 9 seconds                    |

The script exits `0` as soon as the headset pairs (usually within a few
seconds), or `1` with an error after the timeout (default 90 s, change with
`--seconds`). Use `--debug` to see raw HID frames.

Note: opening the pairing window drops an existing headset link.

## Known limitations

- Only the 2012 wireless-headset Nano receiver family (`0x0A29`/`0x0A2D`)
  — not Unifying or Logi Bolt receivers.
- The join-state protocol details come from static analysis of the exe plus
  live probes; error sub-states (codes 0x65–0x69) are not decoded.
