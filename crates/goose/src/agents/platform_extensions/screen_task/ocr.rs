//! macOS Vision OCR: lines of text with their on-screen frames.
//! Vision reports bounding boxes normalized to the image, origin bottom-left —
//! everything here is converted back into the capture's point space.

use super::capture::Capture;
use super::items::{Frame, OcrLine};

#[cfg(target_os = "macos")]
pub fn read_lines(capture: &Capture) -> Vec<OcrLine> {
    use objc2::AllocAnyThread;
    use objc2_foundation::{NSArray, NSString, NSURL};
    use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest};

    let path = NSString::from_str(&capture.path.to_string_lossy());
    let url = NSURL::fileURLWithPath(&path);
    let handler = unsafe {
        VNImageRequestHandler::initWithURL_options(
            VNImageRequestHandler::alloc(),
            &url,
            &objc2_foundation::NSDictionary::new(),
        )
    };
    let request = VNRecognizeTextRequest::new();
    let base: &objc2_vision::VNRequest = &request;
    let requests = NSArray::from_slice(&[base]);
    if handler.performRequests_error(&requests).is_err() {
        return Vec::new();
    }
    let Some(results) = request.results() else {
        return Vec::new();
    };

    let mut lines = Vec::with_capacity(results.count());
    for observation in results.iter() {
        let candidates = observation.topCandidates(1);
        let Some(candidate) = candidates.firstObject() else {
            continue;
        };
        let text = candidate.string().to_string();
        if text.trim().is_empty() {
            continue;
        }
        let bb = unsafe { observation.boundingBox() };
        lines.push(OcrLine {
            text: text.trim().to_string(),
            frame: Frame {
                x: capture.region.x + bb.origin.x * capture.region.w,
                y: capture.region.y + (1.0 - bb.origin.y - bb.size.height) * capture.region.h,
                w: bb.size.width * capture.region.w,
                h: bb.size.height * capture.region.h,
            },
        });
    }
    lines
}

#[cfg(not(target_os = "macos"))]
pub fn read_lines(_capture: &Capture) -> Vec<OcrLine> {
    Vec::new()
}
