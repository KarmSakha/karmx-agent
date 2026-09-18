//! Merge OCR lines and AX pressables into one numbered item list, and build
//! the state document Jev decides over.

use serde_json::{json, Value};

/// A rect in screen points (top-left origin, as AX reports it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Frame {
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn intersects(&self, other: &Frame) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Ocr,
    Ax,
    AxOcr,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub index: usize,
    pub text: String,
    pub role: Option<String>,
    pub frame: Frame,
    pub source: Source,
}

/// A labelled pressable found in the AX tree.
#[derive(Debug, Clone)]
pub struct Pressable {
    pub role: String,
    pub label: String,
    pub frame: Frame,
    /// True when the frame overlaps the captured region; false keeps it as an
    /// off-screen control — pressable via AXPress but not click-worthy.
    pub on_screen: bool,
}

/// The focused text field, when the app exposes one.
#[derive(Debug, Clone)]
pub struct Field {
    pub label: Option<String>,
    pub placeholder: Option<String>,
    pub value: Option<String>,
    pub role: String,
}

/// OCR line: text plus where it sits on screen.
#[derive(Debug, Clone)]
pub struct OcrLine {
    pub text: String,
    pub frame: Frame,
}

/// Everything a step knows about the screen, in capture-cropped coordinates.
pub struct Screen {
    pub app: String,
    pub url: Option<String>,
    pub region: Frame,
    pub image_w: f64,
    pub image_h: f64,
    pub field: Option<Field>,
    pub offscreen: Vec<Pressable>,
}

/// Merge OCR lines with AX pressables into numbered items. An AX control whose
/// frame overlaps an OCR line becomes `ax+ocr` — the label wins since OCR of an
/// icon yields no text. Roughly clicker's merge: both sources feed one list.
pub fn merge_items(ocr: &[OcrLine], pressables: &[Pressable]) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    let mut ocr_used = vec![false; ocr.len()];

    for p in pressables.iter().filter(|p| p.on_screen) {
        let hit = ocr
            .iter()
            .enumerate()
            .find(|(i, line)| !ocr_used[*i] && line.frame.intersects(&shrink(&p.frame, 0.5)));
        let (text, source) = match hit {
            Some((i, line)) if line.text.len() > p.label.len() => {
                ocr_used[i] = true;
                (line.text.clone(), Source::AxOcr)
            }
            Some((i, _)) => {
                ocr_used[i] = true;
                (p.label.clone(), Source::AxOcr)
            }
            None => (p.label.clone(), Source::Ax),
        };
        items.push(Item {
            index: 0,
            text,
            role: Some(p.role.clone()),
            frame: p.frame,
            source,
        });
    }
    for (i, line) in ocr.iter().enumerate() {
        if !ocr_used[i] {
            items.push(Item {
                index: 0,
                text: line.text.clone(),
                role: None,
                frame: line.frame,
                source: Source::Ocr,
            });
        }
    }
    items.sort_by(|a, b| {
        a.frame
            .y
            .partial_cmp(&b.frame.y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (i, item) in items.iter_mut().enumerate() {
        item.index = i;
    }
    items
}

fn shrink(frame: &Frame, factor: f64) -> Frame {
    let dw = frame.w * factor / 2.0;
    let dh = frame.h * factor / 2.0;
    Frame {
        x: frame.x + dw,
        y: frame.y + dh,
        w: frame.w - 2.0 * dw,
        h: frame.h - 2.0 * dh,
    }
}

/// "top-left", "middle-center"… — clamped so an off-screen item never panics.
pub fn region_of(frame: &Frame, screen_w: f64, screen_h: f64) -> String {
    let (cx, cy) = frame.center();
    let col = ["left", "center", "right"][((3.0 * cx / screen_w.max(1.0)) as usize).min(2)];
    let row = ["top", "middle", "bottom"][((3.0 * cy / screen_h.max(1.0)) as usize).min(2)];
    format!("{row}-{col}")
}

/// The state document Jev sees. Items carry their index so the `item` question
/// can answer with one.
pub fn state_payload(goal: &str, screen: &Screen, items: &[Item], history: &[String]) -> Value {
    json!({
        "goal": goal,
        "now": chrono::Local::now().format("%Y-%m-%d %H:%M %A").to_string(),
        "frontmost_app": screen.app,
        "url": screen.url,
        "previous_actions": &history[history.len().saturating_sub(8)..],
        "focused_field": screen.field.as_ref().map(|f| json!({
            "role": f.role, "label": f.label, "placeholder": f.placeholder, "value": f.value,
        })),
        "items": items.iter().map(|it| {
            let mut o = json!({
                "i": it.index,
                "text": it.text,
                "where": region_of(&it.frame, screen.image_w, screen.image_h),
            });
            if let Some(role) = &it.role {
                o["role"] = json!(role);
            }
            o
        }).collect::<Vec<_>>(),
        "offscreen_controls": screen.offscreen.iter().enumerate().map(|(i, p)| json!({
            "key": format!("off{i}"), "role": p.role, "label": p.label,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ax_overlap_merges_into_ax_ocr() {
        let ocr = vec![OcrLine {
            text: "Settings".into(),
            frame: Frame {
                x: 10.0,
                y: 10.0,
                w: 40.0,
                h: 12.0,
            },
        }];
        let ax = vec![Pressable {
            role: "AXButton".into(),
            label: "Settings".into(),
            frame: Frame {
                x: 8.0,
                y: 8.0,
                w: 60.0,
                h: 20.0,
            },
            on_screen: true,
        }];
        let items = merge_items(&ocr, &ax);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].source, Source::AxOcr);
        assert_eq!(items[0].role.as_deref(), Some("AXButton"));
    }

    #[test]
    fn disjoint_sources_both_listed_and_indexed() {
        let ocr = vec![OcrLine {
            text: "plain text".into(),
            frame: Frame {
                x: 10.0,
                y: 90.0,
                w: 80.0,
                h: 10.0,
            },
        }];
        let ax = vec![Pressable {
            role: "AXButton".into(),
            label: "OK".into(),
            frame: Frame {
                x: 10.0,
                y: 10.0,
                w: 40.0,
                h: 20.0,
            },
            on_screen: true,
        }];
        let items = merge_items(&ocr, &ax);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].index, 0);
        assert_eq!(items[1].index, 1);
        assert_eq!(items[0].source, Source::Ax);
        assert_eq!(items[1].source, Source::Ocr);
    }

    #[test]
    fn region_clamps_far_offscreen_centers() {
        let frame = Frame {
            x: -5000.0,
            y: -9000.0,
            w: 10.0,
            h: 10.0,
        };
        let region = region_of(&frame, 1000.0, 800.0);
        assert!(region.ends_with("-left"));
    }
}
