; The emulator's own test cartridge: a game-shaped workload with nothing borrowed from a game.
; Each frame it HALTs until VBlank, DMAs forty sprites, reads the pad, plays all four channels,
; calls into a banked routine and writes cartridge RAM. On a Super Game Boy it first transfers
; 512 palettes and paints with four of them.
;
; Tests break on the fixed addresses below, so moving a section is a change to `roms/mod.rs`.
; Rebuild with `build.sh` in this directory.

DEF rP1   EQU $FF00
DEF rTAC  EQU $FF07
DEF rIF   EQU $FF0F
DEF rNR10 EQU $FF10
DEF rNR11 EQU $FF11
DEF rNR12 EQU $FF12
DEF rNR13 EQU $FF13
DEF rNR14 EQU $FF14
DEF rNR21 EQU $FF16
DEF rNR22 EQU $FF17
DEF rNR23 EQU $FF18
DEF rNR24 EQU $FF19
DEF rNR30 EQU $FF1A
DEF rNR31 EQU $FF1B
DEF rNR32 EQU $FF1C
DEF rNR33 EQU $FF1D
DEF rNR34 EQU $FF1E
DEF rNR42 EQU $FF21
DEF rNR43 EQU $FF22
DEF rNR44 EQU $FF23
DEF rNR50 EQU $FF24
DEF rNR51 EQU $FF25
DEF rNR52 EQU $FF26
DEF rLCDC EQU $FF40
DEF rSCX  EQU $FF43
DEF rLY   EQU $FF44
DEF rDMA  EQU $FF46
DEF rBGP  EQU $FF47
DEF rOBP0 EQU $FF48
DEF rOBP1 EQU $FF49
DEF rWY   EQU $FF4A
DEF rWX   EQU $FF4B
DEF rIE   EQU $FFFF

; LCD on, window at $9C00 and on, tiles at $8000, BG at $9800 and on; sprites off.
DEF LCDC_NO_SPRITES EQU $F1
DEF LCDC_SPRITES    EQU $F3

DEF HRAM_DMA EQU $FF80

SECTION "VBlank interrupt", ROM0[$0040]
    jp VBlankHandler

SECTION "Timer interrupt", ROM0[$0050]
    jp TimerHandler

SECTION "Entry", ROM0[$0100]
    nop
    jp Start
    ds $0150 - @, 0

; A leaf routine called every frame: a = the next of 256 pseudo-random bytes. Clobbers b.
SECTION "Random", ROM0[$0150]
Random:
    ld a, [wRng]
    ld b, a
    add a
    add a
    add b
    inc a
    ld [wRng], a
    ret

; Everything one frame does after VBlank.
SECTION "Tick", ROM0[$0160]
Tick:
    call ReadJoypad
    call MoveSprites
    call Music
    ld a, 3
    ld [$2000], a
    call BankedTick
    ld a, 1
    ld [$2000], a
    ld a, $0A
    ld [$0000], a
    ld a, [wFrame]
    ld [$A000], a
    xor a
    ld [$0000], a
    ret

SECTION "Main", ROM0
Start:
    di
    ld sp, $E000
.waitVBlank
    ldh a, [rLY]
    cp 144
    jr c, .waitVBlank
    xor a
    ldh [rLCDC], a

    ld hl, $8000
    ld bc, $2000
    xor a
    call Fill
    ld hl, wVariables
    ld bc, wVariablesEnd - wVariables
    xor a
    call Fill

    ; The palette table as tile data, and a screen that shows tiles 0-255 in order: what a
    ; Super Game Boy reads a VRAM transfer from.
    ld hl, $8000
    ld de, SgbPalettes
    ld bc, $1000
    call Copy
    ld hl, $9800
    ld b, 0
    ld d, 18
.row
    ld e, 20
.column
    ld a, b
    ld [hl+], a
    inc b
    dec e
    jr nz, .column
    ld a, l
    add 12
    ld l, a
    jr nc, .sameHigh
    inc h
.sameHigh
    dec d
    jr nz, .row

    ld hl, $9C00
    ld bc, $0400
    ld a, 1
    call Fill

    ld hl, HRAM_DMA
    ld de, DmaRoutine
    ld bc, DmaRoutineEnd - DmaRoutine
    call Copy

    ld hl, wShadowOAM
    ld b, 0
.sprite
    ld a, b
    ld c, a
    add a
    add c
    add 16
    ld [hl+], a
    ld a, b
    add a
    add a
    add 8
    ld [hl+], a
    ld a, b
    ld [hl+], a
    ld a, b
    swap a
    and $F0
    ld [hl+], a
    inc b
    ld a, b
    cp 40
    jr nz, .sprite

    ld a, $E4
    ldh [rBGP], a
    ld a, $D2
    ldh [rOBP0], a
    ld a, $1B
    ldh [rOBP1], a
    ld a, 120
    ldh [rWY], a
    ld a, 7
    ldh [rWX], a

    ld a, $80
    ldh [rNR52], a
    ld a, $77
    ldh [rNR50], a
    ld a, $FF
    ldh [rNR51], a
    xor a
    ldh [rNR30], a
    ld hl, $FF30
    ld de, Wave
    ld bc, 16
    call Copy

    ld a, $04
    ldh [rTAC], a
    ld a, $05
    ldh [rIE], a
    xor a
    ldh [rIF], a
    ld a, LCDC_NO_SPRITES
    ldh [rLCDC], a
    ei

    ld b, 3
    call WaitFrames
    ld hl, PalTrn
    call SendPacket
    ld b, 4
    call WaitFrames
    ld hl, PalSet
    call SendPacket
    ld a, LCDC_SPRITES
    ldh [rLCDC], a

MainLoop:
    halt
    nop
    ldh a, [hVBlank]
    and a
    jr z, MainLoop
    xor a
    ldh [hVBlank], a
    call Tick
    jr MainLoop

; b = how many VBlanks to sleep through.
WaitFrames:
    halt
    nop
    ldh a, [hVBlank]
    and a
    jr z, WaitFrames
    xor a
    ldh [hVBlank], a
    dec b
    jr nz, WaitFrames
    ret

VBlankHandler:
    push af
    call HRAM_DMA
    ld a, [wScroll]
    ldh [rSCX], a
    ld a, [wFrame]
    inc a
    ld [wFrame], a
    ld a, 1
    ldh [hVBlank], a
    pop af
    reti

TimerHandler:
    push af
    ld a, [wTicks]
    inc a
    ld [wTicks], a
    pop af
    reti

; wJoy: down, up, left, right in bits 7-4; start, select, B, A in bits 3-0; set when held.
ReadJoypad:
    ld a, $20
    ldh [rP1], a
    ldh a, [rP1]
    ldh a, [rP1]
    cpl
    and $0F
    swap a
    ld b, a
    ld a, $10
    ldh [rP1], a
    ldh a, [rP1]
    ldh a, [rP1]
    ldh a, [rP1]
    ldh a, [rP1]
    cpl
    and $0F
    or b
    ld [wJoy], a
    ld a, $30
    ldh [rP1], a
    ret

; The pad steers sprite 0 and A jumps the scroll; the other sprites drift right.
MoveSprites:
    ld a, [wJoy]
    ld b, a
    ld hl, wShadowOAM
    bit 7, b
    jr z, .notDown
    inc [hl]
.notDown
    bit 6, b
    jr z, .notUp
    dec [hl]
.notUp
    inc hl
    bit 4, b
    jr z, .notRight
    inc [hl]
.notRight
    bit 5, b
    jr z, .notLeft
    dec [hl]
.notLeft
    bit 0, b
    jr z, .notA
    ld a, [wScroll]
    add 8
    ld [wScroll], a
.notA
    ld hl, wShadowOAM + 5
    ld de, 4
    ld c, 39
.drift
    inc [hl]
    add hl, de
    dec c
    jr nz, .drift
    ld a, [wScroll]
    inc a
    ld [wScroll], a
    ret

; Noise every 4 frames, both squares every 8, the wave every 16, new panning every 32.
Music:
    ld a, [wFrame]
    and 3
    jr nz, .squares
    call Random
    ldh [rNR43], a
    ld a, $A1
    ldh [rNR42], a
    ld a, $80
    ldh [rNR44], a
.squares
    ld a, [wFrame]
    and 7
    ret nz
    ld a, [wNote]
    inc a
    and 15
    ld [wNote], a
    call NoteFrequency
    ld a, $12
    ldh [rNR10], a
    ld a, $80
    ldh [rNR11], a
    ld a, $F3
    ldh [rNR12], a
    ld a, e
    ldh [rNR13], a
    ld a, d
    or $80
    ldh [rNR14], a
    ld a, [wNote]
    add 4
    call NoteFrequency
    ld a, $50
    ldh [rNR21], a
    ld a, $C5
    ldh [rNR22], a
    ld a, e
    ldh [rNR23], a
    ld a, d
    or $C0
    ldh [rNR24], a
    ld a, [wFrame]
    and 15
    jr nz, .panning
    ld a, [wNote]
    add 8
    call NoteFrequency
    ld a, $80
    ldh [rNR30], a
    xor a
    ldh [rNR31], a
    ld a, $20
    ldh [rNR32], a
    ld a, e
    ldh [rNR33], a
    ld a, d
    or $80
    ldh [rNR34], a
.panning
    ld a, [wFrame]
    and 31
    ret nz
    call Random
    or $11
    ldh [rNR51], a
    ret

; a = a note index, wrapped to the table; de = its 11-bit frequency.
NoteFrequency:
    and 15
    add a
    ld l, a
    ld h, 0
    ld bc, Notes
    add hl, bc
    ld a, [hl+]
    ld e, a
    ld d, [hl]
    ret

; hl = a 16-byte packet, sent through P1 bit by bit: a reset pulse, 128 bits low first, a 0 to stop.
SendPacket:
    xor a
    ldh [rP1], a
    ld a, $30
    ldh [rP1], a
    ld b, 16
.byte
    ld a, [hl+]
    ld d, a
    ld e, 8
.bit
    ld a, $10
    bit 0, d
    jr nz, .pulse
    ld a, $20
.pulse
    ldh [rP1], a
    ld a, $30
    ldh [rP1], a
    srl d
    dec e
    jr nz, .bit
    dec b
    jr nz, .byte
    ld a, $20
    ldh [rP1], a
    ld a, $30
    ldh [rP1], a
    ret

; hl = destination, bc = length, a = the byte.
Fill:
    ld d, a
.loop
    ld a, d
    ld [hl+], a
    dec bc
    ld a, b
    or c
    jr nz, .loop
    ret

; hl = destination, de = source, bc = length.
Copy:
    ld a, [de]
    inc de
    ld [hl+], a
    dec bc
    ld a, b
    or c
    jr nz, Copy
    ret

DmaRoutine:
    ld a, HIGH(wShadowOAM)
    ldh [rDMA], a
    ld a, 40
.wait
    dec a
    jr nz, .wait
    ret
DmaRoutineEnd:

Notes:
    dw 1547, 1650, 1714, 1798, 1783, 1714, 1650, 1602
    dw 1673, 1750, 1798, 1750, 1714, 1650, 1602, 1547

Wave:
    db $01, $23, $45, $67, $89, $AB, $CD, $EF, $FE, $DC, $BA, $98, $76, $54, $32, $10

PalTrn:
    db $0B << 3 | 1
    ds 15, 0

; System palettes 0-3, cancelling the mask.
PalSet:
    db $0A << 3 | 1
    dw 0, 1, 2, 3
    db $40
    ds 6, 0

SECTION "SGB palettes", ROMX, BANK[1]
; 512 four-colour palettes, none of them grey.
SgbPalettes:
FOR I, 512
    dw $001F, $03E0, $7C00, $0210 + I
ENDR

SECTION "Bank 2", ROMX, BANK[2]
    db $22

SECTION "Banked tick", ROMX[$4000], BANK[3]
BankedTick:
    ld a, [BankedValue]
    ld b, a
    ld a, [wBankedSum]
    add b
    ld [wBankedSum], a
    ret
BankedValue:
    db $33

SECTION "Shadow OAM", WRAM0[$C100]
wShadowOAM:
    ds 160

SECTION "Variables", WRAM0
wVariables:
wFrame: db
wTicks: db
wRng: db
wJoy: db
wNote: db
wScroll: db
wBankedSum: db
wVariablesEnd:

SECTION "HRAM variables", HRAM[$FFF0]
hVBlank: db
