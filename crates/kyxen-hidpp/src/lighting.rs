//! PER_KEY_LIGHTING_V2 (0x8081) frames.
//!
//! A frame is a series of `setRangeRgbZones` reports (three `[first, last,
//! r, g, b]` ranges each; we use single-key ranges) followed by `frameEnd`,
//! which latches the colours onto the LEDs. The keyboard does not need a
//! response to be read before the next report, so frames are sent as-is.

use crate::report;

const FN_SET_RANGE: u8 = 5;
const FN_FRAME_END: u8 = 7;
const RANGES_PER_REPORT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// Reports that set `zones` (LED id, colour) and commit them.
/// Returns no reports for an empty change set.
pub fn frame_reports(feature: u8, zones: &[(u8, Rgb)]) -> Vec<Vec<u8>> {
    if zones.is_empty() {
        return Vec::new();
    }
    let mut reports: Vec<Vec<u8>> = zones
        .chunks(RANGES_PER_REPORT)
        .map(|chunk| {
            let params: Vec<u8> = chunk
                .iter()
                .flat_map(|&(id, Rgb(r, g, b))| [id, id, r, g, b])
                .collect();
            report::long(feature, FN_SET_RANGE, &params)
        })
        .collect();
    reports.push(report::long(feature, FN_FRAME_END, &[]));
    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_three_keys_per_report_then_commits() {
        let zones: Vec<_> = (1..=4).map(|id| (id, Rgb(id, 2, 3))).collect();
        let reports = frame_reports(0x0a, &zones);
        assert_eq!(reports.len(), 3);
        assert_eq!(&reports[0][..9], &[0x11, 0xff, 0x0a, 0x5e, 1, 1, 1, 2, 3]);
        assert_eq!(&reports[1][..9], &[0x11, 0xff, 0x0a, 0x5e, 4, 4, 4, 2, 3]);
        assert_eq!(&reports[2][..4], &[0x11, 0xff, 0x0a, 0x7e]);
    }

    #[test]
    fn empty_frame_sends_nothing() {
        assert!(frame_reports(0x0a, &[]).is_empty());
    }
}
