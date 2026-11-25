//! Pure predicates over actual DOM/window-load observations, not a URL resolver.
//! Target assignment/insertion is observed separately from terminal resource
//! state. In particular, legitimate pending CSS is not an absent href. Only the
//! fixture's sole observer helper mints "pending" (a bypassed target reports
//! UNOBSERVED), and same-helper image/stylesheet controls must load.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OracleFailure {
    MalformedSnapshot,
    InlineControls,
    MissingCssTarget,
    WrongCssTarget,
    MissingImageTarget,
    WrongImageTarget,
    DocumentBaseline,
    CssBaseline,
    ImageBaseline,
}

#[derive(Debug)]
pub(super) struct RendererSnapshot<'a> {
    fields: [&'a str; 25],
}

impl<'a> RendererSnapshot<'a> {
    pub(super) fn parse(request_line: &'a str) -> Result<Self, OracleFailure> {
        let words: Vec<_> = request_line.split_whitespace().collect();
        let [method, path, version] = words.as_slice() else {
            return Err(OracleFailure::MalformedSnapshot);
        };
        if *method != "GET" || *version != "HTTP/1.1" {
            return Err(OracleFailure::MalformedSnapshot);
        }
        let values = path
            .strip_prefix("/GH1037_ASSETS/")
            .ok_or(OracleFailure::MalformedSnapshot)?
            .split('/')
            .collect::<Vec<_>>();
        let fields = values
            .try_into()
            .map_err(|_| OracleFailure::MalformedSnapshot)?;
        Ok(Self { fields })
    }

    pub(super) fn inline_controls(&self) -> Result<(), OracleFailure> {
        // Field 24 is the data-URL stylesheet observed by the same helper as
        // the relative stylesheet: it separates "no event" from "no observer".
        if self.fields[4..7] == ["load", "3", "19-73-109"] && self.fields[24] == "load" {
            Ok(())
        } else {
            Err(OracleFailure::InlineControls)
        }
    }

    pub(super) fn relative_targets(&self) -> Result<(), OracleFailure> {
        // Presence is live hasAttribute; values are getAttribute and DOM node
        // state, not a JS "attempted" enum. No network claim follows. The
        // base-resolved href/src reflections (10, 17) belong to the baseline.
        if self.fields[8] != "1" {
            return Err(OracleFailure::MissingCssTarget);
        }
        if self.fields[9] != ".%2Frelative.css"
            || self.fields[11..15] != ["1", "stylesheet", "link", "1"]
            || self.fields[22] != "0"
        {
            return Err(OracleFailure::WrongCssTarget);
        }
        if self.fields[15] != "1" {
            return Err(OracleFailure::MissingImageTarget);
        }
        if self.fields[16] != ".%2Frelative.svg" || self.fields[18..21] != ["1", "img", "1"] {
            return Err(OracleFailure::WrongImageTarget);
        }
        Ok(())
    }

    pub(super) fn measured_baseline(&self) -> Result<(), OracleFailure> {
        // This deliberately enforces the one generic inline about:blank case
        // at its window-load callback, not all engines or Drawio loadURL.
        // Against that base an assigned relative reference stays unresolved:
        // its reflection equals its raw attribute. Explicit edge: only an
        // assigned attribute (8, 15) carries base evidence; an absent one
        // reflects "" under any base and is the target oracle's fault.
        let unresolved = |present: usize, raw: usize, reflected: usize| {
            self.fields[present] != "1" || self.fields[reflected] == self.fields[raw]
        };
        if self.fields[7] != "about%3Ablank"
            || self.fields[21] != "complete"
            || !unresolved(8, 9, 10)
            || !unresolved(15, 16, 17)
        {
            return Err(OracleFailure::DocumentBaseline);
        }
        if self.fields[0] != "pending" || self.fields[3] != "11-22-33" {
            return Err(OracleFailure::CssBaseline);
        }
        if self.fields[1] != "error" || self.fields[2] != "0" || self.fields[23] != "1" {
            return Err(OracleFailure::ImageBaseline);
        }
        Ok(())
    }

    #[cfg(test)]
    fn verdict(&self) -> Result<(), OracleFailure> {
        self.inline_controls()
            .and(self.relative_targets())
            .and(self.measured_baseline())
    }
}

#[cfg(test)]
mod tests {
    use super::{OracleFailure, RendererSnapshot};

    // Existing native effect values plus the declared DOM input contract.
    // DOM fields require their own actual native baseline/mutation proof.
    const BASELINE: [&str; 25] = [
        "pending",
        "error",
        "0",
        "11-22-33",
        "load",
        "3",
        "19-73-109",
        "about%3Ablank",
        "1",
        ".%2Frelative.css",
        ".%2Frelative.css",
        "1",
        "stylesheet",
        "link",
        "1",
        "1",
        ".%2Frelative.svg",
        ".%2Frelative.svg",
        "1",
        "img",
        "1",
        "complete",
        "0",
        "1",
        "load",
    ];

    fn line(fields: &[&str; 25]) -> String {
        format!("GET /GH1037_ASSETS/{} HTTP/1.1", fields.join("/"))
    }

    #[test]
    fn assigned_targets_with_genuinely_pending_css_pass() {
        assert_eq!(
            RendererSnapshot::parse(&line(&BASELINE))
                .expect("snapshot")
                .verdict(),
            Ok(())
        );
    }

    #[test]
    fn omitted_css_target_rejects_even_with_same_pending_state_and_inline_positives() {
        let mut fields = BASELINE;
        fields[8] = "0";
        fields[9] = "UNASSIGNED";
        fields[10] = "";
        let text = line(&fields);
        let snapshot = RendererSnapshot::parse(&text).expect("snapshot");
        assert_eq!(snapshot.inline_controls(), Ok(()));
        assert_eq!(snapshot.measured_baseline(), Ok(()));
        assert_eq!(
            snapshot.relative_targets(),
            Err(OracleFailure::MissingCssTarget)
        );
    }

    #[test]
    fn omitted_image_target_rejects_while_inline_positives_pass() {
        let mut fields = BASELINE;
        fields[1] = "pending";
        fields[15] = "0";
        fields[16] = "UNASSIGNED";
        fields[17] = "";
        let text = line(&fields);
        let snapshot = RendererSnapshot::parse(&text).expect("snapshot");
        assert_eq!(snapshot.inline_controls(), Ok(()));
        assert_eq!(
            snapshot.relative_targets(),
            Err(OracleFailure::MissingImageTarget)
        );
    }

    #[test]
    fn neither_relative_target_cannot_pass_from_inline_controls() {
        let mut fields = BASELINE;
        fields[1] = "pending";
        fields[8] = "0";
        fields[15] = "0";
        let text = line(&fields);
        let snapshot = RendererSnapshot::parse(&text).expect("snapshot");
        assert_eq!(snapshot.inline_controls(), Ok(()));
        assert_eq!(snapshot.verdict(), Err(OracleFailure::MissingCssTarget));
    }

    #[test]
    fn both_loaded_effects_cannot_pass_the_recorded_baseline() {
        let mut fields = BASELINE;
        fields[0] = "load";
        fields[1] = "load";
        fields[2] = "7";
        fields[3] = "7-31-91";
        let text = line(&fields);
        let snapshot = RendererSnapshot::parse(&text).expect("snapshot");
        assert_eq!(snapshot.relative_targets(), Ok(()));
        assert_eq!(snapshot.verdict(), Err(OracleFailure::CssBaseline));
    }

    #[test]
    fn foreign_origin_or_wrong_color_cannot_pass() {
        let mut fields = BASELINE;
        fields[7] = "https%3A%2F%2Fforeign.example%2F";
        let text = line(&fields);
        assert_eq!(
            RendererSnapshot::parse(&text).expect("snapshot").verdict(),
            Err(OracleFailure::DocumentBaseline)
        );
        fields[7] = BASELINE[7];
        fields[3] = "222-222-222";
        let text = line(&fields);
        assert_eq!(
            RendererSnapshot::parse(&text).expect("snapshot").verdict(),
            Err(OracleFailure::CssBaseline)
        );
    }

    #[test]
    fn wrong_dom_target_property_or_detached_node_cannot_pass() {
        let mut fields = BASELINE;
        // An absolute attribute reflects itself under about:blank too.
        fields[9] = "https%3A%2F%2Fforeign.example%2Frelative.css";
        fields[10] = fields[9];
        let text = line(&fields);
        let snapshot = RendererSnapshot::parse(&text).expect("snapshot");
        assert_eq!(snapshot.measured_baseline(), Ok(()));
        assert_eq!(snapshot.verdict(), Err(OracleFailure::WrongCssTarget));
        fields[9] = BASELINE[9];
        fields[10] = BASELINE[10];
        fields[18] = "0";
        let text = line(&fields);
        assert_eq!(
            RendererSnapshot::parse(&text).expect("snapshot").verdict(),
            Err(OracleFailure::WrongImageTarget)
        );
    }

    #[test]
    fn missing_effect_control_or_malformed_report_cannot_pass() {
        let mut fields = BASELINE;
        fields[5] = "0";
        let text = line(&fields);
        assert_eq!(
            RendererSnapshot::parse(&text).expect("snapshot").verdict(),
            Err(OracleFailure::InlineControls)
        );
        assert!(matches!(
            RendererSnapshot::parse("GET /GH1037_ASSETS/pending/error HTTP/1.1"),
            Err(OracleFailure::MalformedSnapshot)
        ));
    }

    #[test]
    fn unloaded_stylesheet_control_or_bypassed_observer_cannot_pass() {
        // A missing or dead observer must not masquerade as legitimate pending.
        for control in ["pending", "error", "UNOBSERVED"] {
            let mut fields = BASELINE;
            fields[24] = control;
            let text = line(&fields);
            let snapshot = RendererSnapshot::parse(&text).expect("snapshot");
            assert_eq!(snapshot.relative_targets(), Ok(()));
            assert_eq!(snapshot.measured_baseline(), Ok(()));
            assert_eq!(snapshot.verdict(), Err(OracleFailure::InlineControls));
        }
        let mut fields = BASELINE;
        fields[0] = "UNOBSERVED";
        let text = line(&fields);
        let snapshot = RendererSnapshot::parse(&text).expect("snapshot");
        assert_eq!(snapshot.inline_controls(), Ok(()));
        assert_eq!(snapshot.relative_targets(), Ok(()));
        assert_eq!(snapshot.verdict(), Err(OracleFailure::CssBaseline));
    }

    #[test]
    fn base_resolved_reflections_are_baseline_not_target_faults() {
        let mut fields = BASELINE;
        fields[7] = "http%3A%2F%2F127.0.0.1%3A9%2F";
        fields[10] = "http%3A%2F%2F127.0.0.1%3A9%2Frelative.css";
        fields[17] = "http%3A%2F%2F127.0.0.1%3A9%2Frelative.svg";
        let text = line(&fields);
        let snapshot = RendererSnapshot::parse(&text).expect("snapshot");
        assert_eq!(snapshot.relative_targets(), Ok(()));
        assert_eq!(snapshot.verdict(), Err(OracleFailure::DocumentBaseline));
        // Even with the recorded document base, any resolved reflection is a
        // baseline fault rather than a wrong target.
        for reflected in [10, 17] {
            let mut fields = BASELINE;
            fields[reflected] = "https%3A%2F%2Fforeign.example%2Frelative";
            let text = line(&fields);
            let snapshot = RendererSnapshot::parse(&text).expect("snapshot");
            assert_eq!(snapshot.relative_targets(), Ok(()));
            assert_eq!(snapshot.verdict(), Err(OracleFailure::DocumentBaseline));
        }
    }
}
