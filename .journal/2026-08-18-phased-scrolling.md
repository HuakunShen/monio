# 2026-08-18 — `scroll`, and what a phase does not do

`mouse_scroll` existed and was not exported. Rather than exporting it, this adds
a second entry point beside it, for the same reason `type_text` is not part of
`simulate`: a wheel notch and a trackpad gesture are different things and an
application treats them differently.

## The decision

`scroll(delta_x, delta_y, phase)`, with a seven-variant `ScrollPhase`. The
macOS implementation sets three fields the old path never touched:
`kCGScrollWheelEventIsContinuous`, `…ScrollPhase` (began/changed/ended) and
`…MomentumPhase` (begin/continue/end).

The alternative was widening `mouse_scroll` to take a phase. It reads better and
it is wrong the same way widening `Event` for text would have been: `simulate`'s
model is "one input event happened", and a phase is a statement about a
*sequence*. A caller that has no gesture — a script scrolling a window by a
hundred points — should not be made to invent one.

## What made this worth writing down

**A momentum phase does not generate momentum.** This is the natural assumption
and it is false. On a real trackpad the decaying deltas are synthesised by the
driver; the window server does not manufacture them from a label. An injector
that sets `MomentumPhase = begin` and stops sending produces exactly one event
and then silence.

I said the opposite out loud earlier in the day — that phases would let the
caller delete its own fling loop — and it was wrong. What the phase buys is that
the events the caller *does* send are treated as a trackpad's: rubber-banding at
a document edge, smooth rather than stepped scrolling, and scroll views that
know when a gesture ended. The decay stays the caller's.

Both surviving implementations of this agree: Mos and LinearMouse both
synthesise the decay themselves and tag it.

## Sub-point deltas

`CGEventCreateScrollWheelEvent2` takes `i32`, so 0.6 points is zero points. The
fixed-point fields (`…FixedPtDeltaAxis1/2`) are set from the `f64` so an
application reading those sees the real value, but the integer fields are what
most read — so the doc comment tells callers to carry their own remainder. The
caller in this repository already does exactly that for pointer motion and did
*not* for scroll, which is part of why slow scrolling felt like nothing was
happening.

## The tap, which differs from text on purpose

Text goes to the session tap because a Unicode payload is not hardware. A scroll
*is* hardware-shaped — nothing about it needs a layout or an input method — so
it goes to the HID tap, upstream, where the trackpad's own events enter. The
asymmetry between the two functions is deliberate and is the same one KDE
Connect's macOS backend arrived at.

## Left undone

Windows, X11 and Wayland return `NotSupported` naming what each would need.
Windows has no phase concept in `SendInput` at all; X11 has notches only;
libei's `ei_scroll` plus its axis-stop is the closest thing to a real answer and
nobody here can test it.
