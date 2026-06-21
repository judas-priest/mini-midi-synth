# Refactoring Mandate — Full Carte Blanche

## Directive
Full authority to refactor, fix, and improve the entire mini_midi_synth codebase.
No token limits, no time limits. Work until the job is done.

## Scope
- ALL Rust source files (src/synth/*, src/gui/*, src/*.rs)
- Presets validation
- Build system (Cargo.toml)
- Tools (tools/)
- Vendor dependencies

## Goals
1. Clean, idiomatic Rust code following best practices
2. Fix all bugs, warnings, clippy lints
3. Implement any stubbed/unfinished features
4. Ensure full compilation and test passing
5. Refactor oversized files where beneficial
6. Fix DSP correctness issues
7. Fix GUI issues
8. Validate preset files

## Rules
- Never overwrite existing presets (create new ones if needed)
- MIDI keyboard has no velocity (always 127) — don't make core timbre velocity-dependent
- Verify everything compiles and tests pass before committing
- Check work multiple times before moving on
- Don't trust sub-agents blindly — always verify their output

## Verification
- `cargo clippy -- -D warnings` must pass
- `cargo test` must pass
- `cargo build --release` must compile
- `cargo build --no-default-features` (headless) must compile
