//! Audio hotplug reconnect policy (TECHDEBT #18).
//! Plan: docs/superpowers/plans/2026-10-29-metronome-hotplug.md

use std::time::{Duration, Instant};

use mini_midi_synth::audio::{ReconnectPolicy, ReconnectState, RECONNECT_BACKOFF};

#[test]
fn tc1_device_unchanged_is_not_a_change() {
    assert!(!ReconnectPolicy::device_changed("Pipelewire: headphones", Some("Pipelewire: headphones")));
}

#[test]
fn tc2_device_absent_is_transient_not_a_change() {
    // Default device momentarily gone (or name() failed): the live stream
    // keeps playing — must NOT trigger a reconnect by itself.
    assert!(!ReconnectPolicy::device_changed("speakers", None));
}

#[test]
fn tc3_device_changed_triggers() {
    assert!(ReconnectPolicy::device_changed("headphones", Some("speakers")));
}

#[test]
fn tc4_empty_name_edge_cases() {
    assert!(!ReconnectPolicy::device_changed("", Some("")));
    assert!(ReconnectPolicy::device_changed("", Some("x")));
    assert!(!ReconnectPolicy::device_changed("", None));
}

#[test]
fn tc5_first_failure_toasts_then_retries_are_silent() {
    let mut p = ReconnectPolicy::default();
    let t0 = Instant::now();
    // First failure: toast once, arm a +2 s retry.
    assert!(p.on_failure(t0), "first failure must request a toast");
    assert_eq!(p.state, ReconnectState::Retrying { at: t0 + RECONNECT_BACKOFF, attempts: 1 });
    // Duplicate/consecutive failures: silent, attempt counter grows, deadline
    // slides forward from each attempt.
    assert!(!p.on_failure(t0 + Duration::from_millis(500)), "retry failure must not toast");
    assert_eq!(p.state, ReconnectState::Retrying {
        at: t0 + Duration::from_millis(500) + RECONNECT_BACKOFF, attempts: 2,
    });
    assert!(!p.on_failure(t0 + Duration::from_secs(10)));
}

#[test]
fn tc6_retry_due_respects_backoff_deadline() {
    let mut p = ReconnectPolicy::default();
    let t0 = Instant::now();
    assert!(!p.retry_due(t0), "Connected must not retry");
    p.on_failure(t0);
    assert!(!p.retry_due(t0 + RECONNECT_BACKOFF - Duration::from_millis(1)));
    assert!(p.retry_due(t0 + RECONNECT_BACKOFF));
    assert!(p.retry_due(t0 + RECONNECT_BACKOFF + Duration::from_secs(60)));
}

#[test]
fn tc7_success_resets_and_new_failure_streak_toasts_again() {
    let mut p = ReconnectPolicy::default();
    let t0 = Instant::now();
    p.on_failure(t0);
    p.on_failure(t0 + Duration::from_millis(100));
    p.on_success();
    assert_eq!(p.state, ReconnectState::Connected);
    assert!(!p.retry_due(t0 + Duration::from_secs(5)), "retry bookkeeping cleared");
    // A later, unrelated failure streak starts toasting from scratch.
    assert!(p.on_failure(t0 + Duration::from_secs(30)), "new streak must toast once");
}

#[test]
fn tc8_many_failures_keep_counting_no_cap() {
    // Unbounded silent retries while the device is absent (documented tradeoff).
    let mut p = ReconnectPolicy::default();
    let mut now = Instant::now();
    let mut toasts = 0;
    for _ in 0..50 {
        if p.on_failure(now) { toasts += 1; }
        now += RECONNECT_BACKOFF;
    }
    assert_eq!(toasts, 1);
    assert_eq!(p.state, ReconnectState::Retrying { at: now, attempts: 50 });
}
