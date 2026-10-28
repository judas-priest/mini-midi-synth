//! Chord name detection from held notes (template matching).
//!
//! Pardo/Birmingham-style: pitch-class set, per-candidate-root exact
//! template match, bass-note tie-break. Pure function, no deps.

const TEMPLATES: &[(&str, &[u8])] = &[
    ("", &[0, 4, 7]),
    ("m", &[0, 3, 7]),
    ("dim", &[0, 3, 6]),
    ("aug", &[0, 4, 8]),
    ("sus4", &[0, 5, 7]),
    ("sus2", &[0, 2, 7]),
    ("6", &[0, 4, 7, 9]),
    ("m6", &[0, 3, 7, 9]),
    ("7", &[0, 4, 7, 10]),
    ("maj7", &[0, 4, 7, 11]),
    ("m7", &[0, 3, 7, 10]),
    ("m7b5", &[0, 3, 6, 10]),
    ("dim7", &[0, 3, 6, 9]),
    ("7sus4", &[0, 5, 7, 10]),
    ("9", &[0, 4, 7, 10, 2]),
    ("maj9", &[0, 4, 7, 11, 2]),
    ("m9", &[0, 3, 7, 10, 2]),
    ("add9", &[0, 4, 7, 2]),
    ("6/9", &[0, 4, 7, 9, 2]),
];

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Detect a chord name from held notes. Returns None for fewer than 3
/// distinct pitch classes or when no template covers the set.
pub fn detect(notes: &[u8]) -> Option<String> {
    let mut pcs = [false; 12];
    let mut count = 0;
    let bass_pc = notes.iter().min().map(|n| n % 12)?;
    for n in notes {
        if !pcs[(*n % 12) as usize] { count += 1; }
        pcs[(*n % 12) as usize] = true;
    }
    if count < 3 { return None; }

    // Candidate roots, bass pitch class first (tie-break preference).
    let mut roots = [0u8; 12];
    let mut n_roots = 0;
    for off in 0..12u8 {
        roots[n_roots] = (bass_pc + off) % 12;
        n_roots += 1;
    }

    let mut best: Option<(u8, u8, &'static str)> = None; // (score, root, suffix)
    for &root in &roots[..n_roots] {
        for (sfx, iv) in TEMPLATES {
            // Full coverage: every held pc must be an interval of the template
            // rooted at `root`; every template interval must be held.
            let mut covered = 0u8;
            let mut extra = false;
            for pc in 0..12u8 {
                if !pcs[pc as usize] { continue; }
                let iv_of = (pc + 12 - root) % 12;
                if iv.contains(&iv_of) { covered += 1; } else { extra = true; }
            }
            if extra { continue; }
            if covered as usize != iv.len() { continue; } // full template required
            // Score: template size (richer match wins), bass-root bonus.
            let mut score = iv.len() as u8 * 10;
            if root == bass_pc { score += 5; }
            if best.map(|(bs, _, _)| score > bs).unwrap_or(true) {
                best = Some((score, root, sfx));
            }
        }
    }
    let (_, root, sfx) = best?;
    let name = NOTE_NAMES[root as usize];
    if (bass_pc + 12 - root) % 12 == 0 {
        Some(format!("{name}{sfx}"))
    } else {
        Some(format!("{name}{sfx}/{}", NOTE_NAMES[bass_pc as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::detect;
    #[test]
    fn chord_major() { assert_eq!(detect(&[60, 64, 67]), Some("C".into())); }
    #[test]
    fn chord_major7() { assert_eq!(detect(&[60, 64, 67, 71]), Some("Cmaj7".into())); }
    #[test]
    fn chord_minor() { assert_eq!(detect(&[57, 60, 64]), Some("Am".into())); }
    #[test]
    fn chord_inversion_slash() { assert_eq!(detect(&[64, 67, 72]), Some("C/E".into())); }
    #[test]
    fn chord_slash_f_over_c() { assert_eq!(detect(&[60, 65, 69]), Some("F/C".into())); }
    #[test]
    fn chord_seventh() { assert_eq!(detect(&[55, 59, 62, 65]), Some("G7".into())); }
    #[test]
    fn chord_two_notes_none() { assert_eq!(detect(&[60, 64]), None); }
    #[test]
    fn chord_one_note_none() { assert_eq!(detect(&[60]), None); }
    #[test]
    fn chord_empty_none() { assert_eq!(detect(&[]), None); }
    #[test]
    fn chord_unmatched_none() { assert_eq!(detect(&[60, 61, 62]), None); }
}
