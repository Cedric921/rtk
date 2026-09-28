//! Span edits against a segment's own bytes, and the emitter that applies
//! them. A caller that has lexed a segment describes a rewrite as a list of
//! [`Edit`]s against that same text, so the only text that is rebuilt is the
//! part an edit names, and every other byte (gaps, quotes, CRLF, anything
//! odd) survives untouched. What decides an edit is concrete, per-consumer
//! knowledge (see `src/core/README.md`); this module only names the shape
//! and applies it.

/// Replace `original[start..end]` with `text`, as a span against the
/// *original* text a walk read — never against a copy an earlier edit
/// already changed, so a caller can compute every edit from one immutable
/// lexing pass and apply them all at once. `start == end` inserts `text` at
/// that position without removing anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edit {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) text: String,
}

/// Applies `edits` to `original`, keeping every byte no edit names — gaps,
/// quotes, CRLF, whatever a walk did not cover with an edit.
///
/// Validated, not assumed: every `start`/`end` must land on a `char`
/// boundary of `original` and satisfy `start <= end <= original.len()`, and
/// edits must not overlap once sorted. `None` on any violation — this is a
/// hook's rewrite path, so a caller must never let a bad edit panic; the
/// fallback pattern applies here too, and the caller's own contract is to
/// emit the segment unchanged when this returns `None` rather than surface
/// the error.
///
/// Edits take effect in position order regardless of the order they are
/// given in, so a caller building them out of order (an inner one found
/// before an outer one, say) doesn't have to sort first. Sorting is by
/// `(start, end)`, so a zero-width edit (an insertion) at the same position
/// as a wider one is ordered before it — inserted text precedes what starts
/// replacing right after it, rather than the two landing in whatever order
/// the caller happened to build them.
pub(crate) fn apply_edits(original: &str, edits: &[Edit]) -> Option<String> {
    let mut ordered: Vec<&Edit> = edits.iter().collect();
    ordered.sort_by_key(|edit| (edit.start, edit.end));

    let mut out = String::with_capacity(original.len() + 16);
    let mut cursor = 0usize;
    for edit in ordered {
        if edit.start > edit.end
            || edit.end > original.len()
            || edit.start < cursor
            || !original.is_char_boundary(edit.start)
            || !original.is_char_boundary(edit.end)
        {
            return None;
        }
        out.push_str(&original[cursor..edit.start]);
        out.push_str(&edit.text);
        cursor = edit.end;
    }
    out.push_str(&original[cursor..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_edits_keeps_untouched_bytes() {
        // Two replacements inside a padded, operator-joined segment: the
        // padding and the operator must survive exactly as written.
        let original = "  a   &&   b  ";
        let edits = vec![
            Edit {
                start: 2,
                end: 3,
                text: "X".into(),
            },
            Edit {
                start: 11,
                end: 12,
                text: "Y".into(),
            },
        ];
        assert_eq!(apply_edits(original, &edits), Some("  X   &&   Y  ".into()));
    }

    #[test]
    fn test_apply_edits_with_no_edits_returns_original() {
        assert_eq!(apply_edits("git status", &[]), Some("git status".into()));
    }

    /// A zero-width span (`start == end`) inserts text without removing any.
    #[test]
    fn test_apply_edits_zero_width_span_inserts() {
        let original = "run cargo test";
        let edits = vec![
            Edit {
                start: 0,
                end: 0,
                text: "time ".into(),
            },
            Edit {
                start: 4,
                end: 9,
                text: "rtk cargo".into(),
            },
        ];
        assert_eq!(
            apply_edits(original, &edits),
            Some("time run rtk cargo test".into())
        );
    }

    /// Edits given out of position order still land at their own offsets —
    /// a caller doesn't have to sort before calling.
    #[test]
    fn test_apply_edits_order_independent() {
        let original = "a b c";
        let edits = vec![
            Edit {
                start: 4,
                end: 5,
                text: "C".into(),
            },
            Edit {
                start: 0,
                end: 1,
                text: "A".into(),
            },
        ];
        assert_eq!(apply_edits(original, &edits), Some("A b C".into()));
    }

    /// A zero-width edit (insertion) shares its start with a wider edit
    /// right after it — the insertion must still land first regardless of
    /// which order the caller built the two in.
    #[test]
    fn test_apply_edits_same_start_orders_zero_width_first() {
        let original = "cargo test";
        for edits in [
            vec![
                Edit {
                    start: 0,
                    end: 5,
                    text: "rtk cargo".into(),
                },
                Edit {
                    start: 0,
                    end: 0,
                    text: "time ".into(),
                },
            ],
            vec![
                Edit {
                    start: 0,
                    end: 0,
                    text: "time ".into(),
                },
                Edit {
                    start: 0,
                    end: 5,
                    text: "rtk cargo".into(),
                },
            ],
        ] {
            assert_eq!(
                apply_edits(original, &edits),
                Some("time rtk cargo test".into())
            );
        }
    }

    /// CRLF and other bytes between two edits are gaps no edit names, so they
    /// must survive exactly as written.
    #[test]
    fn test_apply_edits_preserves_crlf_gap() {
        let original = "a\r\nb";
        let edits = vec![
            Edit {
                start: 0,
                end: 1,
                text: "A".into(),
            },
            Edit {
                start: 3,
                end: 4,
                text: "B".into(),
            },
        ];
        assert_eq!(apply_edits(original, &edits), Some("A\r\nB".into()));
    }

    #[test]
    fn test_apply_edits_rejects_overlapping_edits() {
        let edits = vec![
            Edit {
                start: 0,
                end: 3,
                text: "a".into(),
            },
            Edit {
                start: 2,
                end: 5,
                text: "b".into(),
            },
        ];
        assert_eq!(apply_edits("hello world", &edits), None);
    }

    #[test]
    fn test_apply_edits_rejects_identical_duplicate_edits() {
        // Two edits naming the same span overlap: once the first is applied
        // the cursor is at 6, and the second one's start, 5, is before it.
        let edits = vec![
            Edit {
                start: 5,
                end: 6,
                text: String::new(),
            },
            Edit {
                start: 5,
                end: 6,
                text: String::new(),
            },
        ];
        assert_eq!(apply_edits("hello world", &edits), None);
    }

    #[test]
    fn test_apply_edits_rejects_a_span_past_the_end() {
        let edits = vec![Edit {
            start: 0,
            end: 100,
            text: "x".into(),
        }];
        assert_eq!(apply_edits("short", &edits), None);
    }

    /// A span landing mid-character (not a `char` boundary) is rejected
    /// rather than panicking the way slicing `original` directly would.
    #[test]
    fn test_apply_edits_rejects_a_non_char_boundary_span() {
        let original = "café";
        // 'é' is a two-byte UTF-8 sequence; byte 4 sits between the two.
        assert!(!original.is_char_boundary(4));
        let edits = vec![Edit {
            start: 4,
            end: 5,
            text: "x".into(),
        }];
        assert_eq!(apply_edits(original, &edits), None);
    }
}
