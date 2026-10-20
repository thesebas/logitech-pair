#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = ["hidapi>=0.15.0"]
# ///
"""Pair a Logitech wireless headset with a Nano receiver (VID 0x046D, PID 0x0A29/0x0A2D).

Works for the models this receiver family supports: H800, H600, UE3500.
The dongle protocol is identical for all of them; only the headset-side
button ritual differs (printed at runtime).

Protocol (reverse-engineered from Logitech's Pairing.exe, live-verified):
  Vendor HID interface on usage page 0xFFA0. Commands are 3-byte output
  reports [0x02, CMD, 0x00]; the dongle answers with [0x02, CMD, STATUS].
  CMD 0x08 = open pairing window, 0x80 = poll status, 0x01 = close/reset,
  0x00 = exe's wizard "next" (not needed).
  STATUS: 0x20 idle · 0x04 window open · 0x10 linking · 0x40 connected.

H800 pairing procedure (binary "havana" instructions): set the headset's
device-selection switch to the free slot (an already-paired slot will not
re-pair), then hold next-track + volume-up until the light blinks fast.
Model rituals: H800 next-track + volume-up ("havana"); H600 volume-up +
mic-mute ("sucre"); UE3500 power button held 9 seconds. Dongle protocol
is model-independent.

Run with: uv run h800pair.py [--seconds 90] [--debug]
"""
import argparse
import sys
import time

import hid

VENDOR, PIDS, USAGE_PAGE = 0x046D, (0x0A29, 0x0A2D), 0xFFA0
STATUS = {0x04: 'pairing window open, waiting for a headset',
          0x10: 'headset detected, linking...',
          0x40: 'headset paired and connected'}


def log(msg):
    print(f'\n{time.strftime("%H:%M:%S")}  {msg}', flush=True)


def debug_raw(debug, sent, data, n=None):
    if debug:
        arrow, extra = ('->', f' ({n} bytes)') if sent else ('<-', '')
        print(f'  [debug] {arrow} {data.hex(" ")}{extra}', flush=True)


def open_receiver():
    targets = [d for d in hid.enumerate(vendor_id=VENDOR)
               if d['product_id'] in PIDS and d['usage_page'] == USAGE_PAGE]
    if not targets:
        sys.exit('ERROR: Logitech headset receiver not found. Plug in the USB '
                 'Nano receiver and try again.')
    dev = hid.device()
    dev.open_path(targets[0]['path'])
    return dev


def main():
    ap = argparse.ArgumentParser(description='Pair a Logitech wireless headset (H800 etc.)')
    ap.add_argument('--seconds', type=int, default=90,
                    help='how long to keep the pairing window open (default 90)')
    ap.add_argument('--debug', action='store_true', help='show raw HID frames')
    args = ap.parse_args()

    dev = open_receiver()
    log('Found Logitech headset receiver.')

    state = None

    def poll():
        nonlocal state
        dev.write(bytes([0x02, 0x80, 0x00]))
        debug_raw(args.debug, False, bytes([0x02, 0x80, 0x00]))
        r = dev.read(64, timeout_ms=1000)
        if r:
            raw = bytes(r)
            debug_raw(args.debug, False, raw)
            if len(raw) >= 3 and raw[0] == 0x02:
                new = raw[2]
                if new != state:
                    state = new
                    log(f'Dongle: {STATUS.get(new, f"unknown state 0x{new:02x}")}')
        return state

    try:
        poll()
        dev.write(bytes([0x02, 0x08, 0x00]))
        debug_raw(args.debug, True, bytes([0x02, 0x08, 0x00]), 3)
        log('Pairing window is OPEN.')
        print("""
While the window is open:
  1. Turn the headset ON.
  2. If this headset was paired before, set its device-selection switch
     to the OTHER position first - an already-paired slot will NOT re-pair.
  3. Enter pairing mode (model-specific ritual, hold until the status
     light blinks fast):
       H800:   hold NEXT-TRACK + VOLUME-UP
       H600:   hold VOLUME-UP + MIC-MUTE
       UE3500: press and hold the POWER BUTTON for 9 seconds
  4. Keep the headset near the receiver until the light goes solid.""", flush=True)

        t0 = time.time()
        armed = False
        while time.time() - t0 < args.seconds:
            poll()
            if state in (0x04, 0x10):
                armed = True
            if state == 0x40 and armed:
                log(f'SUCCESS: headset paired and connected '
                    f'({time.time() - t0:.0f}s). You can close this window.')
                return 0
            time.sleep(3 if state == 0x10 else 1)

        log(f'ERROR: no headset paired within {args.seconds}s. '
            'Make sure the headset has battery charge and that you follow '
            'the pairing ritual above for your model, then retry.')
        return 1
    finally:
        try:
            dev.write(bytes([0x02, 0x01, 0x00]))  # close pairing window
            debug_raw(args.debug, True, bytes([0x02, 0x01, 0x00]), 3)
        except Exception:
            pass
        dev.close()


if __name__ == '__main__':
    sys.exit(main())
