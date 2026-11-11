//! Allocation-free counting and streaming facts for the one bootstrap emitter.
use std::{fmt, io};

#[derive(Default)]
pub(super) struct Measured {
    pub(super) utf8_bytes: usize,
    pub(super) utf16_units: usize,
}
impl fmt::Write for Measured {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        // A native PCWSTR must never truncate the admitted original body.
        if text.contains('\0') {
            return Err(fmt::Error);
        }
        self.utf8_bytes = self.utf8_bytes.checked_add(text.len()).ok_or(fmt::Error)?;
        self.utf16_units = self
            .utf16_units
            .checked_add(text.encode_utf16().count())
            .ok_or(fmt::Error)?;
        Ok(())
    }
}

pub(super) struct NativeWide {
    units: Vec<u16>,
    payload_units: usize,
}
impl NativeWide {
    pub(super) fn reserve(payload_units: usize) -> Result<Self, ()> {
        let capacity = payload_units.checked_add(1).ok_or(())?;
        capacity.checked_mul(std::mem::size_of::<u16>()).ok_or(())?;
        let mut units = Vec::new();
        units.try_reserve_exact(capacity).map_err(|_| ())?;
        if units.capacity() > capacity {
            return Err(());
        }
        Ok(Self {
            units,
            payload_units,
        })
    }
    pub(super) fn finish(mut self) -> Result<Vec<u16>, ()> {
        if self.units.len() != self.payload_units {
            return Err(());
        }
        self.units.push(0);
        Ok(self.units)
    }
}
impl fmt::Write for NativeWide {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self
            .units
            .len()
            .checked_add(text.encode_utf16().count())
            .ok_or(fmt::Error)?;
        if text.contains('\0') || end > self.payload_units {
            return Err(fmt::Error);
        }
        self.units.extend(text.encode_utf16());
        Ok(())
    }
}

/// serde_json remains the sole facts encoder. This adapter accepts even split
/// UTF8 code points without allocating a second JSON/facts buffer.
struct JsonText<'a, W: fmt::Write> {
    output: &'a mut W,
    partial: [u8; 4],
    length: usize,
}
impl<W: fmt::Write> io::Write for JsonText<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.length == 0
            && let Ok(text) = std::str::from_utf8(bytes)
        {
            self.output
                .write_str(text)
                .map_err(|_| io::ErrorKind::InvalidData)?;
            return Ok(bytes.len());
        }
        for &byte in bytes {
            if self.length == self.partial.len() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            self.partial[self.length] = byte;
            self.length += 1;
            match std::str::from_utf8(&self.partial[..self.length]) {
                Ok(text) => {
                    self.output
                        .write_str(text)
                        .map_err(|_| io::ErrorKind::InvalidData)?;
                    self.length = 0;
                }
                Err(error) if error.error_len().is_none() && self.length < 4 => {}
                Err(_) => return Err(io::ErrorKind::InvalidData.into()),
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.length == 0 {
            Ok(())
        } else {
            Err(io::ErrorKind::InvalidData.into())
        }
    }
}
pub(super) fn write_json(
    output: &mut impl fmt::Write,
    value: &impl serde::Serialize,
) -> Result<(), ()> {
    let mut writer = JsonText {
        output,
        partial: [0; 4],
        length: 0,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| ())?;
    io::Write::flush(&mut writer).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn json_text_preserves_split_unicode_and_refuses_incomplete_or_invalid_input() {
        let mut output = String::new();
        let mut writer = JsonText {
            output: &mut output,
            partial: [0; 4],
            length: 0,
        };
        for byte in "\u{1f680}\u{3bb}".as_bytes() {
            writer.write_all(&[*byte]).expect("split valid UTF8");
        }
        writer.flush().expect("all original code points completed");
        assert_eq!(output, "\u{1f680}\u{3bb}");
        let mut writer = JsonText {
            output: &mut output,
            partial: [0; 4],
            length: 0,
        };
        writer.write_all(&[0xf0]).expect("one pending prefix");
        assert!(writer.flush().is_err());
        assert!(writer.write_all(&[0xff]).is_err());
    }
}
