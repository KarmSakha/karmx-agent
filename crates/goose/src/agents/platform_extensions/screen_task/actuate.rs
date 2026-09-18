//! Synthetic input via CGEvent: clicks, key presses, unicode typing, scroll.

#![cfg(target_os = "macos")]

use super::items::Frame;
use anyhow::bail;
use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventType, CGKeyCode, CGMouseButton, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

const KEY_RETURN: CGKeyCode = 36;
const KEY_ESCAPE: CGKeyCode = 53;

fn source() -> CGEventSource {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState).expect("HID event source")
}

fn post(event: &CGEvent) {
    event.post(CGEventTapLocation::HID);
    std::thread::sleep(std::time::Duration::from_millis(40));
}

/// Left-click the centre of `frame`.
pub fn click(frame: &Frame) -> anyhow::Result<()> {
    let (cx, cy) = frame.center();
    let point = CGPoint::new(cx, cy);
    for kind in [
        CGEventType::MouseMoved,
        CGEventType::LeftMouseDown,
        CGEventType::LeftMouseUp,
    ] {
        let event = CGEvent::new_mouse_event(source(), kind, point, CGMouseButton::Left)
            .map_err(|_| anyhow::anyhow!("could not create mouse event"))?;
        post(&event);
    }
    Ok(())
}

/// Insert `text` as unicode input — one key event carrying the whole string.
pub fn type_text(text: &str) -> anyhow::Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let event = CGEvent::new_keyboard_event(source(), 0, true)
        .map_err(|_| anyhow::anyhow!("could not create keyboard event"))?;
    event.set_string(text);
    post(&event);
    Ok(())
}

/// A named key press — Return or Escape (the two Jev can pick).
pub fn press_key(key: &str) -> anyhow::Result<()> {
    let code = match key {
        "return" | "enter" => KEY_RETURN,
        "escape" => KEY_ESCAPE,
        _ => bail!("unknown key {key}"),
    };
    for down in [true, false] {
        let event = CGEvent::new_keyboard_event(source(), code, down)
            .map_err(|_| anyhow::anyhow!("could not create keyboard event"))?;
        post(&event);
    }
    Ok(())
}

/// Scroll wheel — positive `dy` scrolls content down (wheel up on macOS is
/// positive wheel1 for natural scrolling content-up).
pub fn scroll(dy: i32) -> anyhow::Result<()> {
    let event = CGEvent::new_scroll_event(source(), ScrollEventUnit::LINE, 1, dy, 0, 0)
        .map_err(|_| anyhow::anyhow!("could not create scroll event"))?;
    post(&event);
    Ok(())
}

/// AXPress on an element — works even when the control is off-screen.
#[cfg(target_os = "macos")]
pub fn ax_press(element: &accessibility::AXUIElement) -> anyhow::Result<()> {
    use core_foundation::string::CFString;
    element
        .perform_action(&CFString::new("AXPress"))
        .map_err(|e| anyhow::anyhow!("AXPress failed: {e:?}"))
}
