//! End-to-end tests for Firefox-style smooth scrolling, driven by the headless
//! [`iced_test::Simulator`].
//!
//! These tests check the defining behavior of smooth scrolling: a wheel notch
//! must move the viewport through intermediate positions over time instead of
//! jumping to the destination.

use std::time::Duration;

use iced_component::smooth_scrollable::{ScrollMode, smooth_scrollable};
use iced_test::core::{
    Element, Event, Point, Settings, Size, mouse, time::Instant, window,
};
use iced_test::simulator::Simulator;
use iced_widget::core::Length;
use iced_widget::scrollable::Scrollable;
use iced_widget::{Renderer, Theme, column, container, text};

const VIEWPORT: Size = Size::new(1024.0, 768.0);
const CENTER: Point = Point::new(VIEWPORT.width / 2.0, VIEWPORT.height / 2.0);

/// One wheel notch, in pixels (`ScrollDelta::Lines` is scaled by 60).
const NOTCH: f32 = 60.0;

/// The scroll range of the nested scrollable in [`nested_view`], which is
/// smaller than a single notch.
const NESTED_RANGE: f32 = 40.0;

#[derive(Debug, Clone, PartialEq)]
enum Message {
    Scrolled(f32),
    Nested(f32),
}

fn view(mode: ScrollMode) -> Element<'static, Message, Theme, Renderer> {
    smooth_scrollable(
        column((0..200).map(|index| text!("row {index}").into())).spacing(4),
    )
    .scroll_mode(mode)
    .height(Length::Fill)
    .width(Length::Fill)
    .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
    .into()
}

/// A [`SmoothScrollable`] with a plain [`Scrollable`] nested in its content.
///
/// The nested list sits below a few screens' worth of rows, so that it is still
/// visible once the wrapper has been scrolled away from the start.
///
/// Its content is 240px tall inside a 200px viewport, so a single notch already
/// exhausts it and the next one has to reach the wrapper.
fn nested_view() -> Element<'static, Message, Theme, Renderer> {
    let nested = Scrollable::new(
        container(
            column((0..10).map(|index| text!("nested {index}").into()))
                .spacing(4),
        )
        .height(240),
    )
    .id("nested")
    .height(200)
    .width(Length::Fill)
    .on_scroll(|viewport| Message::Nested(viewport.absolute_offset().y));

    smooth_scrollable(
        column![
            column((0..24).map(|index| text!("row {index}").into())).spacing(4),
            nested,
            column((0..200).map(|index| text!("row {index}").into()))
                .spacing(4),
        ]
        .spacing(8),
    )
    .scroll_mode(ScrollMode::Smooth)
    .height(Length::Fill)
    .width(Length::Fill)
    .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
    .into()
}

/// The visible center of the nested scrollable, given that the wrapper is
/// scrolled `offset` pixels away from the start.
///
/// The nested widget is laid out in the content space of the wrapper, which the
/// wrapper draws shifted up by its offset.
fn nested_center(
    ui: &mut Simulator<'_, Message, Theme, Renderer>,
    offset: f32,
) -> Point {
    let row = ui
        .find("nested 0")
        .expect("the nested list should be rendered");

    row.bounds().center() - iced_test::core::Vector::new(0.0, offset)
}

fn anchored_view() -> Element<'static, Message, Theme, Renderer> {
    smooth_scrollable(
        column((0..200).map(|index| text!("row {index}").into())).spacing(4),
    )
    .anchor_bottom()
    .height(Length::Fill)
    .width(Length::Fill)
    .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
    .into()
}

/// The offsets reported by the wrapper itself.
fn scroll_offsets(messages: impl Iterator<Item = Message>) -> Vec<f32> {
    messages
        .filter_map(|message| match message {
            Message::Scrolled(offset) => Some(offset),
            Message::Nested(_) => None,
        })
        .collect()
}

/// The offsets reported by the nested scrollable.
fn nested_offsets(messages: impl Iterator<Item = Message>) -> Vec<f32> {
    messages
        .filter_map(|message| match message {
            Message::Nested(offset) => Some(offset),
            Message::Scrolled(_) => None,
        })
        .collect()
}

fn redraw(ui: &mut Simulator<'_, Message, Theme, Renderer>) {
    let _ = ui.simulate([Event::Window(window::Event::RedrawRequested(
        Instant::now(),
    ))]);
}

/// Rolls the wheel by `y` lines at the current cursor position.
fn wheel(ui: &mut Simulator<'_, Message, Theme, Renderer>, y: f32) {
    let _ = ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y },
    })]);
}

#[test]
fn smooth_mode_animates_through_intermediate_offsets() {
    let mut ui = Simulator::with_size(
        Settings::default(),
        VIEWPORT,
        view(ScrollMode::Smooth),
    );

    ui.point_at(CENTER);
    wheel(&mut ui, -1.0);

    // Drive the animation with periodic redraws. The physics samples the real
    // elapsed time, so a short sleep lets the animation make progress.
    for _ in 0..160 {
        std::thread::sleep(Duration::from_millis(3));
        redraw(&mut ui);
    }

    let offsets = scroll_offsets(ui.into_messages());

    assert!(!offsets.is_empty(), "expected scroll notifications");

    let final_offset = *offsets.last().unwrap();

    assert!(
        (final_offset - NOTCH).abs() < 1.0,
        "expected the viewport to settle at {NOTCH}px, got {final_offset}"
    );

    assert!(
        offsets
            .iter()
            .any(|offset| *offset > 1.0 && *offset < NOTCH - 1.0),
        "expected intermediate offsets, got {offsets:?}"
    );
}

#[test]
fn end_anchored_scrollable_detects_its_anchor() {
    let mut ui =
        Simulator::with_size(Settings::default(), VIEWPORT, anchored_view());

    ui.point_at(CENTER);

    // With a bottom anchor and a fresh absolute offset of `0`, the viewport
    // starts at the bottom of the content.
    redraw(&mut ui);

    // Scrolling "up" (a positive line delta) must move away from the bottom,
    // which increases the absolute offset even though the anchor is reversed.
    wheel(&mut ui, 1.0);

    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(3));
        redraw(&mut ui);
    }

    let offsets = scroll_offsets(ui.into_messages());

    assert_eq!(offsets.first(), Some(&0.0));
    assert!(
        (offsets.last().copied().unwrap_or_default() - NOTCH).abs() < 1.0,
        "expected the bottom anchor to be detected, got {offsets:?}"
    );
}

#[test]
fn instant_mode_jumps_to_the_destination() {
    let mut ui = Simulator::with_size(
        Settings::default(),
        VIEWPORT,
        view(ScrollMode::Instant),
    );

    ui.point_at(CENTER);
    wheel(&mut ui, -1.0);

    // No redraws are needed: instant scrolling notifies the new viewport
    // synchronously.
    let offsets = scroll_offsets(ui.into_messages());

    assert_eq!(offsets.first(), Some(&NOTCH));
}

#[test]
fn smooth_mode_accumulates_consecutive_notches() {
    let mut ui = Simulator::with_size(
        Settings::default(),
        VIEWPORT,
        view(ScrollMode::Smooth),
    );

    ui.point_at(CENTER);

    for _ in 0..3 {
        wheel(&mut ui, -1.0);
        std::thread::sleep(Duration::from_millis(2));
        redraw(&mut ui);
    }

    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(3));
        redraw(&mut ui);
    }

    let offsets = scroll_offsets(ui.into_messages());
    let final_offset = *offsets.last().expect("expected scroll notifications");

    assert!(
        (final_offset - 3.0 * NOTCH).abs() < 1.0,
        "expected three notches ({}) to accumulate, got {final_offset}",
        3.0 * NOTCH
    );
}

#[test]
fn nested_scrollable_consumes_the_wheel() {
    let mut ui =
        Simulator::with_size(Settings::default(), VIEWPORT, nested_view());

    let target = nested_center(&mut ui, 0.0);
    ui.point_at(target);
    wheel(&mut ui, -1.0);

    let messages: Vec<Message> = ui.into_messages().collect();
    let nested = nested_offsets(messages.iter().cloned());

    assert_eq!(
        nested.first(),
        Some(&NESTED_RANGE),
        "expected the nested scrollable to consume the notch, got {messages:?}"
    );

    let outer = scroll_offsets(messages.iter().cloned());

    assert!(
        outer.iter().all(|offset| *offset == 0.0),
        "expected the wrapper to stay put, got {outer:?}"
    );
}

#[test]
fn nested_scrollable_is_found_while_the_wrapper_is_scrolled() {
    let mut ui =
        Simulator::with_size(Settings::default(), VIEWPORT, nested_view());

    // Scroll the wrapper away from the start first, and let it settle.
    ui.point_at(CENTER);
    wheel(&mut ui, -1.0);

    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(3));
        redraw(&mut ui);
    }

    // The nested list is laid out in the content space of the wrapper, so it is
    // drawn a notch higher than its layout bounds claim.
    let target = nested_center(&mut ui, NOTCH);
    ui.point_at(target);
    wheel(&mut ui, -1.0);

    let messages: Vec<Message> = ui.into_messages().collect();
    let nested = nested_offsets(messages.iter().cloned());

    assert_eq!(
        nested.last(),
        Some(&NESTED_RANGE),
        "expected the nested scrollable to consume the notch, got {messages:?}"
    );

    let outer = scroll_offsets(messages.iter().cloned());
    let final_offset = *outer.last().expect("expected scroll notifications");

    assert!(
        (final_offset - NOTCH).abs() < 1.0,
        "expected the wrapper to stay one notch down, got {outer:?}"
    );
}

#[test]
fn wheel_chains_smoothly_past_an_exhausted_nested_scrollable() {
    let mut ui =
        Simulator::with_size(Settings::default(), VIEWPORT, nested_view());

    let target = nested_center(&mut ui, 0.0);
    ui.point_at(target);

    // The first notch is consumed by the nested scrollable...
    wheel(&mut ui, -1.0);

    // ...and once its scroll transaction has expired, the next notch has
    // nowhere left to go, so it must reach the wrapper—smoothly, even though
    // the nested scrollable would have jumped.
    std::thread::sleep(Duration::from_millis(120));
    let _ = ui.simulate([Event::Mouse(mouse::Event::CursorMoved {
        position: target,
    })]);

    wheel(&mut ui, -1.0);

    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(3));
        redraw(&mut ui);
    }

    let messages: Vec<Message> = ui.into_messages().collect();
    let outer = scroll_offsets(messages.iter().cloned());
    let final_offset = *outer.last().expect("expected scroll notifications");

    assert!(
        (final_offset - NOTCH).abs() < 1.0,
        "expected the wrapper to settle one notch down, got {outer:?}"
    );

    assert!(
        outer
            .iter()
            .any(|offset| *offset > 1.0 && *offset < NOTCH - 1.0),
        "expected the chained scroll to be animated, got {outer:?}"
    );
}
