use crate::error::{ProbeError, ProbeResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Pmv2,
    Unaware,
}

impl Mode {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Pmv2 => "pmv2",
            Self::Unaware => "unaware",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Control {
    InvalidMonitor,
    FailedDpi,
    HighContrastSize,
}

impl Control {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::InvalidMonitor => "invalid-monitor",
            Self::FailedDpi => "failed-dpi",
            Self::HighContrastSize => "high-contrast-size",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Args {
    pub(crate) mode: Mode,
    pub(crate) control: Option<Control>,
}

impl Args {
    pub(crate) fn parse(values: impl IntoIterator<Item = String>) -> ProbeResult<Self> {
        let mut values = values.into_iter();
        let mut mode = None;
        let mut control = None;
        while let Some(key) = values.next() {
            let value = values.next().ok_or_else(|| {
                ProbeError::invalid("arguments", format!("Missing value for {key}"))
            })?;
            match (key.as_str(), value.as_str()) {
                ("--mode", "pmv2") if mode.is_none() => mode = Some(Mode::Pmv2),
                ("--mode", "unaware") if mode.is_none() => mode = Some(Mode::Unaware),
                ("--control", "invalid-monitor") if control.is_none() => {
                    control = Some(Control::InvalidMonitor)
                }
                ("--control", "failed-dpi") if control.is_none() => {
                    control = Some(Control::FailedDpi)
                }
                ("--control", "high-contrast-size") if control.is_none() => {
                    control = Some(Control::HighContrastSize)
                }
                _ => {
                    return Err(ProbeError::invalid(
                        "arguments",
                        format!("Unknown or repeated argument: {key} {value}"),
                    ));
                }
            }
        }
        Ok(Self {
            mode: mode.ok_or_else(|| {
                ProbeError::invalid("arguments", "An explicit --mode pmv2|unaware is required")
            })?,
            control,
        })
    }
}
