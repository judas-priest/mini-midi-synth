use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use std::fs::File;
use std::sync::Arc;

fn rms(buf: &[f32]) -> f32 {
    let n = buf.len() as f32;
    (buf.iter().map(|s| s * s).sum::<f32>() / n).sqrt()
}

fn main() {
    let path = std::env::var("HOME").unwrap() + "/.local/share/mini_midi_synth/sf2/Metal_Drums_v2.sf2";
    let mut file = File::open(&path).unwrap();
    let sf = Arc::new(SoundFont::new(&mut file).unwrap());
    let settings = SynthesizerSettings::new(44_100);
    let names = ["", "inferno kick", "rim", "snare", "rim2", "sidestick", "snare2", "snare3",
        "tom10", "tom12", "tom16", "Semi-Open Hat", "Mid hat", "Hi Hat", "Dark Crash", "Cymbals"];
    let (mut l, mut r) = (vec![0f32; 441], vec![0f32; 441]);
    for (i, &note) in [46u8, 47, 48, 49, 50, 51].iter().enumerate() {
        let mut syn = Synthesizer::new(&sf, &settings).unwrap();
        syn.process_midi_message(9, 0x90, note as i32, 100);
        let mut ring = Vec::new();
        for _ in 0..100 { syn.render(&mut l, &mut r); for j in 0..441 { ring.push((l[j]+r[j])*0.5); } }
        let tail_natural = rms(&ring[ring.len()-441..]); // 0.99-1.0s без off
        syn.process_midi_message(9, 0x80, note as i32, 0);
        let mut tail = Vec::new();
        for _ in 0..50 { syn.render(&mut l, &mut r); for j in 0..441 { tail.push((l[j]+r[j])*0.5); } }
        let after_off = rms(&tail[..441]); // 0-10ms после off
        println!("{} note={} nat_tail(1s)={:.5} after_noteoff={:.5} ratio={}",
            names[[11u8,12,13,14,15,16][i] as usize], note, tail_natural, after_off,
            if after_off > 0.0 { tail_natural / after_off } else { 0.0 });
    }
}
