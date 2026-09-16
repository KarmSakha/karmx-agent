//! Computer-control action vocabulary.
//!
//! Field names and semantics are taken from the recovered
//! `CognitionComputerUseAction` shape (9 fields):
//!
//! ```text
//! action_type · coordinate · duration · key · region
//! scroll_amount · scroll_direction · start_coordinate · text
//! ```
//!
//! Every field is optional so that one struct can carry any action, which is
//! why validation matters here: an action is only meaningful when the right
//! subset of fields is present. [`ComputerAction::validate`] enforces that, and
//! [`ComputerAction::to_cdp`] lowers a validated action onto CDP input events.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

impl ScrollDirection {
    /// CDP `Input.dispatchMouseEvent` deltas for one scroll step.
    fn deltas(self, amount: i32) -> (i32, i32) {
        match self {
            ScrollDirection::Up => (0, amount),
            ScrollDirection::Down => (0, -amount),
            ScrollDirection::Left => (amount, 0),
            ScrollDirection::Right => (-amount, 0),
        }
    }
}

/// One computer-control action.
///
/// Mirrors the recovered 9-field shape exactly; `action_type` selects which
/// other fields are meaningful.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerAction {
    /// Which action this is: click, double_click, right_click, move, type,
    /// key, scroll, drag, screenshot, wait.
    pub action_type: String,

    /// Target position, for pointer actions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinate: Option<Point>,

    /// Drag origin, paired with `coordinate` as the destination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_coordinate: Option<Point>,

    /// Key name for `key`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,

    /// Text to insert for `type`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// Scroll magnitude.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_amount: Option<i32>,

    /// Scroll axis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_direction: Option<ScrollDirection>,

    /// Region to crop, for `screenshot`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,

    /// Milliseconds to wait, for `wait`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
}

impl ComputerAction {
    pub fn new(action_type: &str) -> Self {
        Self {
            action_type: action_type.to_string(),
            coordinate: None,
            start_coordinate: None,
            key: None,
            text: None,
            scroll_amount: None,
            scroll_direction: None,
            region: None,
            duration: None,
        }
    }

    /// Every action type this vocabulary accepts.
    pub const TYPES: &'static [&'static str] = &[
        "click",
        "double_click",
        "right_click",
        "move",
        "type",
        "key",
        "scroll",
        "drag",
        "screenshot",
        "wait",
    ];

    /// Reject an action that cannot be executed.
    ///
    /// This is the trust boundary for model-authored input: a malformed action
    /// is rejected here rather than becoming a no-op pointer event at (0,0)
    /// that silently looks like it worked.
    pub fn validate(&self) -> Result<(), String> {
        let t = self.action_type.as_str();
        if !Self::TYPES.contains(&t) {
            return Err(format!(
                "unknown action_type {t:?}; expected one of {}",
                Self::TYPES.join(", ")
            ));
        }
        match t {
            "click" | "double_click" | "right_click" | "move" => {
                if self.coordinate.is_none() {
                    return Err(format!("{t} requires `coordinate`"));
                }
            }
            "drag" => {
                if self.start_coordinate.is_none() || self.coordinate.is_none() {
                    return Err("drag requires both `start_coordinate` and `coordinate`".into());
                }
            }
            "type" => {
                if self.text.as_deref().map(str::is_empty).unwrap_or(true) {
                    return Err("type requires non-empty `text`".into());
                }
            }
            "key" => {
                if self.key.as_deref().map(str::is_empty).unwrap_or(true) {
                    return Err("key requires a non-empty `key` name".into());
                }
            }
            "scroll" => {
                if self.scroll_amount.unwrap_or(0) == 0 {
                    return Err("scroll requires a non-zero `scroll_amount`".into());
                }
                if self.scroll_direction.is_none() {
                    return Err("scroll requires `scroll_direction`".into());
                }
            }
            "screenshot" => {
                if let Some(r) = self.region {
                    if r.width <= 0 || r.height <= 0 {
                        return Err(
                            "screenshot `region` must have positive width and height".into()
                        );
                    }
                }
            }
            "wait" if self.duration.unwrap_or(0) == 0 => {
                return Err("wait requires a non-zero `duration` in milliseconds".into());
            }
            _ => {}
        }
        Ok(())
    }

    /// Lower to CDP commands, as `(method, params)` pairs.
    ///
    /// Must be called on a validated action. Coordinates are forwarded as-is;
    /// the caller converts from the model's viewport space if the screenshot it
    /// saw was scaled.
    pub fn to_cdp(&self) -> Result<Vec<(String, serde_json::Value)>, String> {
        self.validate()?;
        let mut out = Vec::new();
        let btn = |name: &str| {
            if name == "right_click" {
                "right"
            } else {
                "left"
            }
        };

        match self.action_type.as_str() {
            "move" => {
                let p = self.coordinate.unwrap();
                out.push((
                    "Input.dispatchMouseEvent".into(),
                    serde_json::json!({"type":"mouseMoved","x":p.x,"y":p.y}),
                ));
            }
            "click" | "right_click" => {
                let p = self.coordinate.unwrap();
                let b = btn(&self.action_type);
                for ty in ["mousePressed", "mouseReleased"] {
                    out.push((
                        "Input.dispatchMouseEvent".into(),
                        serde_json::json!({"type":ty,"x":p.x,"y":p.y,"button":b,"clickCount":1}),
                    ));
                }
            }
            "double_click" => {
                let p = self.coordinate.unwrap();
                for ty in [
                    "mousePressed",
                    "mouseReleased",
                    "mousePressed",
                    "mouseReleased",
                ] {
                    out.push((
                        "Input.dispatchMouseEvent".into(),
                        serde_json::json!({"type":ty,"x":p.x,"y":p.y,"button":"left","clickCount":2}),
                    ));
                }
            }
            "drag" => {
                let a = self.start_coordinate.unwrap();
                let b = self.coordinate.unwrap();
                out.push((
                    "Input.dispatchMouseEvent".into(),
                    serde_json::json!({"type":"mousePressed","x":a.x,"y":a.y,"button":"left","clickCount":1}),
                ));
                out.push((
                    "Input.dispatchMouseEvent".into(),
                    serde_json::json!({"type":"mouseMoved","x":b.x,"y":b.y,"button":"left"}),
                ));
                out.push((
                    "Input.dispatchMouseEvent".into(),
                    serde_json::json!({"type":"mouseReleased","x":b.x,"y":b.y,"button":"left","clickCount":1}),
                ));
            }
            "type" => {
                for ch in self.text.as_deref().unwrap_or("").chars() {
                    out.push((
                        "Input.dispatchKeyEvent".into(),
                        serde_json::json!({"type":"char","text":ch.to_string()}),
                    ));
                }
            }
            "key" => {
                let k = self.key.as_deref().unwrap_or("");
                for ty in ["keyDown", "keyUp"] {
                    out.push((
                        "Input.dispatchKeyEvent".into(),
                        serde_json::json!({"type":ty,"key":k}),
                    ));
                }
            }
            "scroll" => {
                let amount = self.scroll_amount.unwrap_or(0).abs();
                let (dx, dy) = self.scroll_direction.unwrap().deltas(amount);
                let p = self.coordinate.unwrap_or(Point { x: 0, y: 0 });
                out.push((
                    "Input.dispatchMouseEvent".into(),
                    serde_json::json!({"type":"mouseWheel","x":p.x,"y":p.y,"deltaX":dx,"deltaY":dy}),
                ));
            }
            "wait" => {
                // no CDP command; the caller sleeps
            }
            "screenshot" => {
                let mut params = serde_json::json!({"format":"png"});
                if let Some(r) = self.region {
                    params["clip"] = serde_json::json!({
                        "x": r.x, "y": r.y, "width": r.width, "height": r.height, "scale": 1
                    });
                }
                out.push(("Page.captureScreenshot".into(), params));
            }
            other => return Err(format!("no CDP mapping for action_type {other:?}")),
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_click(x: i32, y: i32) -> ComputerAction {
        let mut a = ComputerAction::new("click");
        a.coordinate = Some(Point { x, y });
        a
    }

    #[test]
    fn unknown_action_type_is_rejected() {
        let a = ComputerAction::new("teleport");
        assert!(a.validate().unwrap_err().contains("unknown action_type"));
    }

    #[test]
    fn click_requires_a_coordinate() {
        let a = ComputerAction::new("click");
        assert!(a.validate().unwrap_err().contains("coordinate"));
        assert!(with_click(10, 20).validate().is_ok());
    }

    #[test]
    fn drag_requires_both_ends() {
        let mut a = ComputerAction::new("drag");
        a.coordinate = Some(Point { x: 1, y: 1 });
        assert!(a.validate().unwrap_err().contains("start_coordinate"));

        a.start_coordinate = Some(Point { x: 0, y: 0 });
        assert!(a.validate().is_ok());
    }

    #[test]
    fn type_rejects_empty_text() {
        let mut a = ComputerAction::new("type");
        a.text = Some(String::new());
        assert!(a.validate().is_err());

        a.text = Some("hello".into());
        assert!(a.validate().is_ok());
    }

    #[test]
    fn scroll_needs_amount_and_direction() {
        let mut a = ComputerAction::new("scroll");
        assert!(a.validate().is_err());

        a.scroll_amount = Some(0);
        a.scroll_direction = Some(ScrollDirection::Down);
        assert!(a.validate().unwrap_err().contains("non-zero"));

        a.scroll_amount = Some(120);
        assert!(a.validate().is_ok());
    }

    #[test]
    fn wait_needs_a_duration() {
        let mut a = ComputerAction::new("wait");
        assert!(a.validate().is_err());
        a.duration = Some(250);
        assert!(a.validate().is_ok());
    }

    #[test]
    fn screenshot_region_must_be_positive() {
        let mut a = ComputerAction::new("screenshot");
        assert!(a.validate().is_ok(), "full-viewport screenshot is valid");
        a.region = Some(Region {
            x: 0,
            y: 0,
            width: 0,
            height: 10,
        });
        assert!(a.validate().unwrap_err().contains("positive"));
    }

    #[test]
    fn click_lowers_to_press_and_release() {
        let cmds = with_click(5, 7).to_cdp().unwrap();
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].0, "Input.dispatchMouseEvent");
        assert_eq!(cmds[0].1["type"], "mousePressed");
        assert_eq!(cmds[1].1["type"], "mouseReleased");
        assert_eq!(cmds[0].1["x"], 5);
        assert_eq!(cmds[0].1["y"], 7);
    }

    #[test]
    fn right_click_uses_right_button() {
        let mut a = ComputerAction::new("right_click");
        a.coordinate = Some(Point { x: 1, y: 1 });
        let cmds = a.to_cdp().unwrap();
        assert_eq!(cmds[0].1["button"], "right");
    }

    #[test]
    fn drag_emits_press_move_release() {
        let mut a = ComputerAction::new("drag");
        a.start_coordinate = Some(Point { x: 0, y: 0 });
        a.coordinate = Some(Point { x: 50, y: 60 });
        let cmds = a.to_cdp().unwrap();
        let kinds: Vec<&str> = cmds.iter().map(|c| c.1["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, vec!["mousePressed", "mouseMoved", "mouseReleased"]);
    }

    #[test]
    fn scroll_direction_maps_to_signed_deltas() {
        let mut a = ComputerAction::new("scroll");
        a.scroll_amount = Some(100);
        a.scroll_direction = Some(ScrollDirection::Down);
        let cmds = a.to_cdp().unwrap();
        assert_eq!(cmds[0].1["deltaY"], -100, "down should be negative deltaY");

        a.scroll_direction = Some(ScrollDirection::Up);
        let cmds = a.to_cdp().unwrap();
        assert_eq!(cmds[0].1["deltaY"], 100);
    }

    #[test]
    fn type_emits_one_char_event_each() {
        let mut a = ComputerAction::new("type");
        a.text = Some("abc".into());
        let cmds = a.to_cdp().unwrap();
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[1].1["text"], "b");
    }

    #[test]
    fn screenshot_without_region_has_no_clip() {
        let cmds = ComputerAction::new("screenshot").to_cdp().unwrap();
        assert_eq!(cmds[0].0, "Page.captureScreenshot");
        assert!(cmds[0].1.get("clip").is_none());
    }

    #[test]
    fn screenshot_with_region_sets_clip() {
        let mut a = ComputerAction::new("screenshot");
        a.region = Some(Region {
            x: 1,
            y: 2,
            width: 30,
            height: 40,
        });
        let cmds = a.to_cdp().unwrap();
        assert_eq!(cmds[0].1["clip"]["width"], 30);
        assert_eq!(cmds[0].1["clip"]["height"], 40);
    }

    #[test]
    fn to_cdp_validates_first() {
        // an invalid action must not silently produce an empty command list
        assert!(ComputerAction::new("click").to_cdp().is_err());
    }
}
