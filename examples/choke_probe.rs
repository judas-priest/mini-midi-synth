// Проверка exclusiveClass: semi-open (46) звонит -> closed (51) должен убить голос
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use std::fs::File;
use std::sync::Arc;

fn rms(b: &[f32]) -> f32 { (b.iter().map(|s| s*s).sum::<f32>() / b.len() as f32).sqrt() }

fn main() {
    let path = std::env::var("HOME").unwrap() + "/.local/share/mini_midi_synth/sf2/Metal_Drums_v2.sf2";
    let mut file = File::open(&path).unwrap();
    let sf = Arc::new(SoundFont::new(&mut file).unwrap());
    let st = SynthesizerSettings::new(44_100);
    let mut syn = Synthesizer::new(&sf, &st).unwrap();
    let (mut l, mut r) = (vec![0f32; 441], vec![0f32; 441]);
    syn.process_midi_message(9, 0x90, 46, 100); // semi-open
    for _ in 0..30 { syn.render(&mut l, &mut r); }
    let mut before = Vec::new();
    for _ in 0..5 { syn.render(&mut l, &mut r); for j in 0..441 { before.push((l[j]+r[j])*0.5); } }
    syn.process_midi_message(9, 0x90, 48, 1); // Hi Hat vel=1: kill by class, тихий
    let mut after = Vec::new();
    for _ in 0..5 { syn.render(&mut l, &mut r); for j in 0..441 { after.push((l[j]+r[j])*0.5); } }
    // контроль: то же без закрытого (просто звенит дальше)
    let mut syn2 = Synthesizer::new(&sf, &st).unwrap();
    syn2.process_midi_message(9, 0x90, 46, 100);
    for _ in 0..30 { syn2.render(&mut l, &mut r); }
    let mut ctl = Vec::new();
    for _ in 0..5 { syn2.render(&mut l, &mut r); for j in 0..441 { ctl.push((l[j]+r[j])*0.5); } }
    println!("semi-open ringing:  {:.5}", rms(&before));
    println!("ctl (46 звенит сам): {:.5}", rms(&ctl));
    println!("после тихого 48:      {:.5}", rms(&after));
    println!("choke works: {}", rms(&after) < rms(&ctl) * 0.7);
}
