# macOS Clippy cleanup

**Timestamp:** 2026-08-06 09:29 +08

## Core Decision/Topic

Keep Monio's macOS target compatible with the repository-wide zero-warning
policy without accepting an invalid automated Clippy rewrite.

## Options Considered

- Ignore the two macOS-only lints because Linux CI does not compile those
  files. Rejected: the parent repository must be able to run `-D warnings` on
  a developer Mac.
- Accept Clippy's `&[yes]` suggestion for the CoreFoundation boolean. Rejected:
  that removes the dereference and leaves the generic CoreFoundation type
  unresolved (`E0283`).
- Replace the unreadable click tuple with a named alias and document the one
  necessary lint allowance. Chosen: it removes the valid lint and preserves
  type-correct CoreFoundation ownership semantics.

## Final Decision & Rationale

The `SimClick` alias makes five positional click fields readable at their use
sites.  The remaining `borrow_deref_ref` allowance stays narrow and explains
why the proposed expression does not compile; it is not a blanket warning
suppression.

## Key Changes Made

- Added `SimClick` for the macOS simulated-click tuple.
- Added the documented local allowance for the non-compiling CoreFoundation
  borrow suggestion.

## Verification

- `CARGO_TARGET_DIR=/Users/hk/Dev/CrossCopy/target cargo clippy --all-targets --all-features -- -D warnings` passed on macOS.

## Future Considerations

If the objc2/CoreFoundation API later offers a type-inferable equivalent, remove
the local allowance and keep the zero-warning gate unchanged.
