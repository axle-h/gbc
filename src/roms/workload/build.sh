#!/bin/sh
# Assemble workload.gb from workload.asm with rgbds. The ROM is committed; run this after editing.
set -eu
cd "$(dirname "$0")"
rgbasm -o workload.o workload.asm
rgblink -o workload.gb workload.o
rgbfix -v -s -m 0x13 -r 0x03 -k 01 -l 0x33 -t "GBC WORKLOAD" -p 0xFF workload.gb
rm workload.o
