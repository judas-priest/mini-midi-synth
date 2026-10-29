//! Layout-contract tests for the egui panel registration order
//! (TECHDEBT #25/#26). egui runs headless here: `Context::run` computes
//! layout without a renderer or window.
//!
//! NOTE (egui 0.31): panels COLLAPSE TO CONTENT unless the content fills
//! them — a `TopBottomPanel` frame shrinks to `height_range.min` and a
//! `SidePanel` frame to its width_range.min if the content is tiny. The
//! docked panels here use ScrollArea::auto_shrink(false) exactly like the
//! real mod.rs code, otherwise geometry differs from production.
//!
//! This test MIRRORS the panel registration order of
//! `src/gui/mod.rs::update` (top panels omitted — irrelevant to the
//! bottom/right carving). If you reorder panels in mod.rs, update this
//! mirror.
#![cfg(feature = "gui")]

use eframe::egui;

const WIN: (f32, f32) = (1024.0, 640.0);

/// One headless frame with the same panel registration order as mod.rs.
/// `gm_width` pins the right panel width (exact_width bypasses the
/// resizable drag state so the assertion is deterministic).
fn frame(gm_width: f32) -> (egui::Rect, egui::Rect, egui::Rect, egui::Rect) {
    // (keyboard, drum_seq, gm_panel, central)
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
        //    drum sequencer, then the right panel, CentralPanel LAST.
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
        let gm = egui::SidePanel::right("preset_panel")
            .resizable(true)
            .exact_width(gm_width)
            .width_range(120.0..=260.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.label("gm");
                });
            });
        let central = egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("main");
        });
        rects = Some((kb.response.rect, seq.response.rect, gm.response.rect, central.response.rect));
    });
    rects.expect("ui closure must run")
}

/// #25/#26 contract: panels carve in registration order; the docked drum
/// sequencer sits ABOVE the keyboard strip and BELOW the central field;
/// nothing overlaps.
#[test]
fn panels_carve_no_overlap_seq_above_keyboard() {
    let (kb, seq, gm, central) = frame(160.0);

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
    // the right panel is carved AFTER the bottom panels: top..seq_top only
    assert!(
        (gm.max.y - seq.min.y).abs() < 0.5 && gm.min.y.abs() < 0.5,
        "gm panel must span top..seq_top: {gm:?}"
    );
    // central strictly left of the right panel (touching edge == reflow)
    assert!(central.max.x <= gm.min.x + 0.5, "central overlaps gm: {central:?} vs {gm:?}");
}

/// #26 contract: widening the GM panel shrinks the main field by the same
/// amount instead of overlapping it.
#[test]
fn gm_panel_resize_reflows_central() {
    let (_, _, gm, central) = frame(160.0);
    let (_, _, gm_wide, central_wide) = frame(260.0);

    assert!((gm.width() - 160.0).abs() < 0.5);
    assert!((gm_wide.width() - 260.0).abs() < 0.5);

    // central strictly to the left of the panel in both cases
    assert!(central.max.x <= gm.min.x + 0.5, "central overlaps gm: {central:?} vs {gm:?}");
    assert!(
        central_wide.max.x <= gm_wide.min.x + 0.5,
        "central overlaps gm: {central_wide:?} vs {gm_wide:?}"
    );
    // reflow, not overlap: +100 px of panel == -100 px of central width
    let d_gm = gm_wide.width() - gm.width();
    let d_c = central.width() - central_wide.width();
    assert!((d_gm - d_c).abs() < 0.5, "central must reflow 1:1 with panel resize: dg={d_gm} dc={d_c}");
}
