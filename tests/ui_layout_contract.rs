//! Layout-contract tests for the egui panel registration order
//! (TECHDEBT #25/#26). egui runs headless here: `Context::run` computes
//! layout without a renderer or window.
//!
//! NOTE (egui 0.31): panels COLLAPSE TO CONTENT unless the content fills
//! them — a `TopBottomPanel` frame shrinks to `height_range.min` if the
//! content is tiny. The docked panels here use ScrollArea::auto_shrink(false)
//! exactly like the real mod.rs code, otherwise geometry differs from
//! production.
//!
//! This test MIRRORS the panel registration order of
//! `src/gui/mod.rs::update` (top panels omitted — irrelevant to the
//! bottom carving). If you reorder panels in mod.rs, update this
//! mirror. There is no right panel anymore: presets live in a
//! modal opened from the part row, so the central field spans the
//! full window width.
#![cfg(feature = "gui")]

use eframe::egui;

const WIN: (f32, f32) = (1024.0, 640.0);

/// One headless frame with the same panel registration order as mod.rs.
fn frame() -> (egui::Rect, egui::Rect, egui::Rect) {
    // (keyboard, drum_seq, central)
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.0);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIN.0, WIN.1))),
        ..Default::default()
    };
    let mut rects = None;
    // `Context::run` is #[must_use] — FullOutput not needed, rects captured
    let _ = ctx.run(input, |ctx| {
        // -- mirrors mod.rs: keyboard first (bottom-most), then the docked
        //    drum sequencer, CentralPanel LAST (no side panels anymore).
        let kb = egui::TopBottomPanel::bottom("keyboard_panel")
            .exact_height(90.0)
            .show(ctx, |ui| {
                ui.label("kb");
            });
        let seq = egui::TopBottomPanel::bottom("drum_seq_panel")
            .resizable(true)
            .default_height(460.0)
            .height_range(120.0..=640.0)
            .show(ctx, |ui| {
                // auto_shrink(false) fills the panel — same as mod.rs
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.label("seq");
                });
            });
        let central = egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("main");
        });
        rects = Some((kb.response.rect, seq.response.rect, central.response.rect));
    });
    rects.expect("ui closure must run")
}

/// #25/#26 contract: panels carve in registration order; the docked drum
/// sequencer sits ABOVE the keyboard strip and BELOW the central field;
/// nothing overlaps. The central field spans the full window width
/// (the presets side panel was removed — TC1).
#[test]
fn panels_carve_no_overlap_seq_above_keyboard() {
    let (kb, seq, central) = frame();

    // keyboard pinned to the very bottom
    assert!((kb.max.y - WIN.1).abs() < 0.5, "keyboard not at bottom: {kb:?}");
    // sequencer stacked directly on top of the keyboard
    assert!((seq.max.y - kb.min.y).abs() < 0.5, "seq must sit directly above keyboard: seq={seq:?} kb={kb:?}");
    // default height honored
    assert!((seq.height() - 460.0).abs() < 0.5, "seq height: {seq:?}");
    // central field ends where the sequencer begins
    assert!(
        (central.max.y - seq.min.y).abs() < 0.5,
        "central must end above the seq panel: {central:?} vs {seq:?}"
    );
    // no side panel: central spans the full window width
    assert!(
        central.min.x.abs() < 0.5 && (central.max.x - WIN.0).abs() < 0.5,
        "central must span full width without a right panel: {central:?}"
    );
}
