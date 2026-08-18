# 2026-08-18 — `type_text`, and why it is not part of `simulate`

`TODO.md` opened this morning with "not implemented on any platform" and an
estimate of *days* for macOS. It took an afternoon. The estimate was wrong in a
way worth recording, because the reason is not "it was easier than expected" —
the API really is three calls — but that the survey had guessed at the wrong
difficulty entirely.

## The decision: a separate entry point

The TODO's design note argued for this before any code existed, and building it
confirmed the argument rather than complicating it:

> Text injection should be a separate API from `key_press`/`key_release`, not a
> widening of them. They authorize differently and they fail differently.

Concretely, `simulate.rs` was not touched. `EventType::KeyTyped` still has no
injection arm, and that is now correct rather than an omission: `KeyTyped` is a
capture-side concept — what a hook observed — and giving it an injection arm
would have made one enum mean two different things depending on which direction
it was travelling.

The alternative considered and rejected was widening `Event` so that
`simulate(&Event::key_typed('好'))` worked. It reads well and it is wrong: a
`char` cannot carry `👨‍👩‍👧‍👦`, which is seven scalar values and one grapheme, and a
per-character API forces the caller to make the chunking mistake this
implementation exists to avoid.

## What actually cost the time

Not the API. `CGEventCreateKeyboardEvent`, `CGEventKeyboardSetUnicodeString`,
`CGEventPost` — three calls, and `objc2-core-graphics` already binds all three.

The time went to two failure modes that are silent, undocumented, and invisible
to any test that lives in this crate:

**Truncation.** `CGEventKeyboardSetUnicodeString` stops caring past roughly
twenty UTF-16 units. Nothing errors; the tail is simply not typed. In the field
this looks like "long messages lose their end", which reads as a network bug.
Chunked at 16.

**Split surrogate pairs.** With chunking in place, a boundary that lands between
the two halves of an emoji produces two U+FFFD replacement characters. Also
silent. The chunker steps back one unit when the last one is a high surrogate.

Both are checked by the only kind of test that can see them from inside this
crate — one that walks the chunker's arithmetic and asserts every chunk is valid
UTF-16 on its own and that the pieces reassemble.

## Why the real test is not in this repository

`CGEventPost` returns void. There is no acknowledgement, no error channel, and
no way to observe from this process whether anything was typed. So a test that
proves `type_text` works has to drive an application and read it back, which
makes it the caller's test.

The caller wrote one: `xross/poc/remote-input/host/scripts/typing-check.sh`
opens a scratch TextEdit document, sends `你好 world 🌍 — the quick brown fox
jumps over the lazy dog, twice over.` over a WebSocket, reads the document back
through AppleScript and closes it without saving. Chinese that no keycode can
produce, an emoji outside the basic plane, and enough ASCII to cross several
chunk boundaries. It passes.

That split is worth stating as a rule rather than an accident: **an injection
API can only be unit-tested up to the point where it hands the event to the
window server.** Everything past that is the caller's integration test, and a
crate that pretends otherwise is testing its own arithmetic and calling it
coverage.

## The session tap

The one design detail the TODO flagged and it was right to. `simulate::post_event`
posts at `HIDEventTap`, upstream of everything, which is what makes a synthetic
click indistinguishable from a real one. A Unicode payload is not hardware —
nothing scanned it — and text posted there does not behave.

KDE Connect's macOS backend does both taps in one function: modifiers to the HID
tap (`macosremoteinput.mm:173`), Unicode to the session tap (`:180-189`). This
follows it. The asymmetry is not a wart; a text commit and a key press really are
different kinds of event.

Events are still tagged through `provenance`, so this crate's hooks can tell
injected text from a person typing. Skipping that would have made text the one
injected event type that lies to `InputOrigin`.

## Left undone, deliberately

Windows, X11, Wayland and HarmonyOS return `Error::NotSupported` with the API
each would need in the message. Writing Windows blind — no machine here can
compile it, let alone run it — would have produced code that looks finished and
has never executed, which is worse than a refusal that says what is missing.
`TODO.md`'s table is now a work list rather than a survey.

The Wayland row still has no answer, and after twelve years neither KDE Connect
nor GSConnect has one either. It stays marked as research.
