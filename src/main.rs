//! Desktop binary. All logic lives in the library crate so that the Android
//! cdylib and the desktop executable share exactly one code path.

fn main() -> anyhow::Result<()> {
    mini_midi_synth::desktop_main()
}
