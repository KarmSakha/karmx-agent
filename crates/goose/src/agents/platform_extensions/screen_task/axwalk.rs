//! Accessibility-tree walk of the frontmost app: labelled pressable controls,
//! the front window's frame, the focused text field, and controls the tree
//! offers that sit off screen (pressable via AXPress, not the mouse).

#![cfg(target_os = "macos")]

use super::items::{Field, Frame, Pressable};
use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes};
use accessibility_sys::{
    kAXValueTypeCGPoint, kAXValueTypeCGSize, AXValueGetType, AXValueGetValue, AXValueRef,
};
use core_foundation::base::{CFType, TCFType};
use core_foundation::string::CFString;
use core_graphics_types::geometry::{CGPoint, CGSize};
use std::time::Instant;

/// AXUIElement is a raw-pointer wrapper and not Send; a pressable's element
/// handle must cross the Jev await. AX attribute reads and AXPress are safe to
/// issue from any thread — the retain/release is what needs to move.
pub struct SendElement(pub AXUIElement);
/// AX calls are safe to issue from any thread; the loop is sequential anyway,
/// so Sync only exists to satisfy the async borrow checker.
unsafe impl Send for SendElement {}
unsafe impl Sync for SendElement {}

const MAX_NODES: usize = 4000;
const MAX_DEPTH: usize = 25;
const WALK_BUDGET_MS: u128 = 600;
const MIN_SIZE_PT: f64 = 4.0;

/// What a step learned from the app's accessibility tree.
pub struct AxInfo {
    pub app: String,
    pub pid: i32,
    pub window: Option<Frame>,
    pub pressables: Vec<Pressable>,
    pub offscreen: Vec<Pressable>,
    pub field: Option<Field>,
    /// (role, element) pairs for offscreen pressables — AXPress needs the
    /// element handle, kept alongside its item index prefix.
    pub offscreen_elements: Vec<SendElement>,
}

/// Snapshot the frontmost application's accessibility surface.
pub fn snapshot(display: &Frame) -> Option<AxInfo> {
    let (app, pid) = frontmost()?;
    let element = AXUIElement::application(pid);
    let _ = element.set_messaging_timeout(0.6);

    let window = element
        .windows()
        .ok()
        .and_then(|ws| ws.iter().next().map(|w| w.clone()))
        .and_then(|w| frame_of(&w));

    let mut walk = Walk {
        nodes: 0,
        started: Instant::now(),
        pressables: Vec::new(),
        offscreen: Vec::new(),
        offscreen_elements: Vec::new(),
    };
    if let Some(win_frame) = window {
        if let Ok(wins) = element.windows() {
            for win in wins.iter() {
                walk.visit(&win, 0, &win_frame);
            }
        }
    } else {
        walk.visit(&element, 0, display);
    }

    let field = focused_field(&element);
    Some(AxInfo {
        app,
        pid,
        window,
        pressables: walk.pressables,
        offscreen: walk.offscreen,
        field,
        offscreen_elements: walk.offscreen_elements,
    })
}

pub(super) fn frontmost() -> Option<(String, i32)> {
    use objc2_app_kit::NSWorkspace;
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let name = app
        .localizedName()
        .map(|n| n.to_string())
        .unwrap_or_else(|| "app".to_string());
    Some((name, app.processIdentifier()))
}

struct Walk {
    nodes: usize,
    started: Instant,
    pressables: Vec<Pressable>,
    offscreen: Vec<Pressable>,
    offscreen_elements: Vec<SendElement>,
}

const PRESSABLE_ROLES: &[&str] = &[
    "AXButton",
    "AXLink",
    "AXCheckBox",
    "AXRadioButton",
    "AXMenuItem",
    "AXPopUpButton",
    "AXComboBox",
    "AXTab",
    "AXTextField",
    "AXTextArea",
    "AXSlider",
    "AXIncrementor",
    "AXDisclosureTriangle",
    "AXMenuButton",
];

impl Walk {
    fn visit(&mut self, element: &AXUIElement, depth: usize, clip: &Frame) {
        if self.nodes >= MAX_NODES
            || depth > MAX_DEPTH
            || self.started.elapsed().as_millis() > WALK_BUDGET_MS
        {
            return;
        }
        self.nodes += 1;

        let role = element.role().ok().map(|r| r.to_string());
        if role.as_deref() == Some("AXMenu") {
            return;
        }
        let frame = frame_of(element);
        let labelled = label_of(element, role.as_deref());
        let pressable = role
            .as_deref()
            .map(|r| PRESSABLE_ROLES.contains(&r))
            .unwrap_or(false);

        if pressable && !labelled.is_empty() {
            if let Some(frame) = frame {
                if frame.w >= MIN_SIZE_PT && frame.h >= MIN_SIZE_PT {
                    let on_screen = frame.intersects(clip);
                    let p = Pressable {
                        role: role.clone().unwrap_or_default(),
                        label: labelled,
                        frame,
                        on_screen,
                    };
                    if on_screen {
                        self.pressables.push(p);
                    } else {
                        self.offscreen.push(p);
                        self.offscreen_elements.push(SendElement(element.clone()));
                    }
                }
            }
        }

        let Ok(children) = element.children() else {
            return;
        };
        for child in children.iter() {
            self.visit(&child, depth + 1, clip);
        }
    }
}

/// A node's best human label: AXTitle for AppKit, AXDescription for web and
/// Electron, else a short AXValue.
fn label_of(element: &AXUIElement, role: Option<&str>) -> String {
    for attr in [
        element.title().ok().map(|s| s.to_string()),
        element.description().ok().map(|s| s.to_string()),
        element.help().ok().map(|s| s.to_string()),
    ] {
        if let Some(text) = attr {
            let text = text.trim().to_string();
            if !text.is_empty() {
                return text;
            }
        }
    }
    if matches!(
        role,
        Some("AXTextField") | Some("AXTextArea") | Some("AXComboBox")
    ) {
        if let Ok(value) = element.value() {
            let text = cf_to_string(&value);
            if !text.trim().is_empty() {
                return text.trim().chars().take(60).collect();
            }
        }
    }
    String::new()
}

fn cf_to_string(value: &CFType) -> String {
    if value.type_of() == CFString::type_id() {
        unsafe { CFString::wrap_under_get_rule(value.as_CFTypeRef() as _) }.to_string()
    } else {
        String::new()
    }
}

/// AXPosition + AXSize arrive as AXValue-wrapped CGPoint/CGSize.
fn frame_of(element: &AXUIElement) -> Option<Frame> {
    let position = ax_point(element, "AXPosition")?;
    let size = ax_size(element, "AXSize")?;
    Some(Frame {
        x: position.x,
        y: position.y,
        w: size.width,
        h: size.height,
    })
}

fn ax_point(element: &AXUIElement, name: &str) -> Option<CGPoint> {
    let attr = AXAttribute::<CFType>::new(&CFString::new(name));
    let value = element.attribute(&attr).ok()?;
    let mut point = CGPoint::new(0.0, 0.0);
    let ok = unsafe {
        AXValueGetType(value.as_CFTypeRef() as AXValueRef) == kAXValueTypeCGPoint
            && AXValueGetValue(
                value.as_CFTypeRef() as AXValueRef,
                kAXValueTypeCGPoint,
                &mut point as *mut _ as *mut _,
            )
    };
    ok.then_some(point)
}

fn ax_size(element: &AXUIElement, name: &str) -> Option<CGSize> {
    let attr = AXAttribute::<CFType>::new(&CFString::new(name));
    let value = element.attribute(&attr).ok()?;
    let mut size = CGSize::new(0.0, 0.0);
    let ok = unsafe {
        AXValueGetType(value.as_CFTypeRef() as AXValueRef) == kAXValueTypeCGSize
            && AXValueGetValue(
                value.as_CFTypeRef() as AXValueRef,
                kAXValueTypeCGSize,
                &mut size as *mut _ as *mut _,
            )
    };
    ok.then_some(size)
}

/// The focused text field, if any: AXFocusedUIElement on the app.
fn focused_field(app: &AXUIElement) -> Option<Field> {
    let attr = AXAttribute::<CFType>::new(&CFString::new("AXFocusedUIElement"));
    let element =
        unsafe { AXUIElement::wrap_under_get_rule(app.attribute(&attr).ok()?.as_CFTypeRef() as _) };
    let element = &element;
    let role = element.role().ok()?.to_string();
    if !matches!(
        role.as_str(),
        "AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField"
    ) {
        return None;
    }
    let value = element
        .value()
        .ok()
        .map(|v| cf_to_string(&v))
        .filter(|s| !s.is_empty());
    Some(Field {
        label: element.title().ok().map(|s| s.to_string()),
        placeholder: element.placeholder_value().ok().map(|s| s.to_string()),
        value,
        role,
    })
}
