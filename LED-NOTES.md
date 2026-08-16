# LED ring: what the renderer can and cannot do

The ring is driven by a BIO core running a program generated from
`src/bio/lightgenes/main.c`. Patterns are selected with `LedManagerOp::SetPattern` and live in
`src/leds.rs`.

## How a pattern reaches the ring

The app sends a **non-blocking** scalar. The handler must therefore read it with
`scalar_message()`, not `scalar_message_mut()` - the mutable accessor returns `None` for a
plain `Scalar` and only matches `BlockingScalar`. Using the wrong one means the handler body
never runs at all, with no error anywhere: the opcode dispatches, the log line never prints,
and the ring simply carries on. That cost most of a debugging session.

A pattern is installed as the gene and expressed, the same way `GeneTest` does it. Writing
straight at the engine with `force()` does not drive the ring.

`phenotype()` blends two strands with dominance rules, so `Diploid([p, p])` does **not**
express as `p`: `sat` and `chaser` are saturating adds, `hue_ratedir` collapses most inputs to
one value, and `nonlin` is computed from the *first strand's chaser*. `as_gene()` inverts all
of that, solving chaser and nonlin together because they share a term.

## Field meanings

Read from `main.c`, which does not use the Rust field names - `chaser` lands on the C struct's
`lin` by byte order.

| Rust | Effect |
|---|---|
| `cd_period` | brightness cycles around the ring; 0 is uniform, higher is more banding |
| `cd_rate` | **inverted**: 0 is fast (0.6s), 255 is slow (7s) |
| `cd_dir` | above 128 travels one way, below the other |
| `sat` | saturation; low values read as white, not pale colour |
| `hue_ratedir` | low nibble is drift rate 0..15; high nibble above 10 reverses |
| `hue_base` / `hue_bound` | the hue range the ring spans |
| `chaser` | **below 88** overlays a white dot running round the ring |
| `nonlin` | above 127 squares the brightness curve: dimmer, steeper falloff |

## Two limits that are not tunable

**There is no brightness floor.** `v = 127 * (1 + cos(spacetime))` reaches zero once per cycle
for every LED, and at zero the pixel is black whatever the saturation. Roughly a quarter of
each cycle is visibly dark. No gene value changes this. A pattern can only choose *where* the
darkness is: `cd_period 0` puts it on every LED at once (reads as breathing), `cd_period >= 1`
spreads the phase so it travels instead (reads as banding).

**The white dot and the brightness wave run on different clocks.** The dot steps once per
render frame off `loop_state`; the wave runs off `tau`, derived from the millisecond clock.
The dot laps in about 20 frames, quicker than the fastest brightness cycle, so a trailing wave
can never track it. `comet` therefore does not try.

## Open option: add a brightness floor

Wanted, not yet done: rainbow, ember and bird all read better with no fully dark pixels, and
that needs a change to the renderer rather than to a pattern.

The renderer is regenerable. `libs/bio-lib/src/c/` in xous-core holds `build.zig` and
`clang2rustasm.py`; the README there documents the flow and needs only
`python3 -m pip install ziglang` (tested against zig 0.15.2). `src/bio/lightgenes/lightgenes.rs`
is the generated artifact and is marked do-not-edit.

The change itself is small - clamp the value into `FLOOR..255` instead of `0..254`:

```c
hsvC.v = FLOOR + (uint8_t)(((255 - FLOOR) * (1 + cos_term)) >> 1);
```

Why it has not been done: it is the highest-risk change in this project. It installs a
toolchain, regenerates assembly that drives hardware directly, and a bad result affects the
LED service rather than a screen that can simply be redrawn. Worth doing deliberately, with a
known-good build to fall back to, rather than at the end of a long session.
