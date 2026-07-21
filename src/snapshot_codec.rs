//! Strict primitives shared by test-only semantic snapshot codecs.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotCodecError {
    offset: usize,
    message: String,
}

impl SnapshotCodecError {
    pub(crate) fn invalid(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }
}

impl fmt::Display for SnapshotCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "snapshot byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for SnapshotCodecError {}

#[derive(Default)]
pub(crate) struct SnapshotWriter {
    bytes: Vec<u8>,
}

impl SnapshotWriter {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn position(&self) -> usize {
        self.bytes.len()
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(crate) fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    pub(crate) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn usize(&mut self, value: usize) -> Result<(), SnapshotCodecError> {
        let value = u64::try_from(value)
            .map_err(|_| SnapshotCodecError::invalid(self.position(), "length exceeds u64"))?;
        self.u64(value);
        Ok(())
    }

    pub(crate) fn raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) -> Result<(), SnapshotCodecError> {
        self.usize(value.len())?;
        self.raw(value);
        Ok(())
    }

    pub(crate) fn string(&mut self, value: &str) -> Result<(), SnapshotCodecError> {
        self.bytes(value.as_bytes())
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

pub(crate) struct SnapshotReader<'bytes> {
    bytes: &'bytes [u8],
    offset: usize,
}

impl<'bytes> SnapshotReader<'bytes> {
    pub(crate) fn new(bytes: &'bytes [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(crate) fn position(&self) -> usize {
        self.offset
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn take(&mut self, count: usize) -> Result<&'bytes [u8], SnapshotCodecError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| SnapshotCodecError::invalid(self.offset, "offset overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| SnapshotCodecError::invalid(self.offset, "truncated input"))?;
        self.offset = end;
        Ok(value)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, SnapshotCodecError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn bool(&mut self) -> Result<bool, SnapshotCodecError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(SnapshotCodecError::invalid(
                self.offset - 1,
                "invalid boolean discriminant",
            )),
        }
    }

    pub(crate) fn u16(&mut self) -> Result<u16, SnapshotCodecError> {
        let value: [u8; 2] = self
            .take(2)?
            .try_into()
            .expect("the strict reader returned two bytes");
        Ok(u16::from_be_bytes(value))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, SnapshotCodecError> {
        let value: [u8; 4] = self
            .take(4)?
            .try_into()
            .expect("the strict reader returned four bytes");
        Ok(u32::from_be_bytes(value))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, SnapshotCodecError> {
        let value: [u8; 8] = self
            .take(8)?
            .try_into()
            .expect("the strict reader returned eight bytes");
        Ok(u64::from_be_bytes(value))
    }

    pub(crate) fn usize(&mut self) -> Result<usize, SnapshotCodecError> {
        let offset = self.offset;
        usize::try_from(self.u64()?)
            .map_err(|_| SnapshotCodecError::invalid(offset, "length exceeds usize"))
    }

    pub(crate) fn collection_len(
        &mut self,
        minimum_item_bytes: usize,
    ) -> Result<usize, SnapshotCodecError> {
        let offset = self.offset;
        let count = self.usize()?;
        let minimum = count
            .checked_mul(minimum_item_bytes)
            .ok_or_else(|| SnapshotCodecError::invalid(offset, "collection size overflow"))?;
        if minimum > self.remaining() {
            return Err(SnapshotCodecError::invalid(
                offset,
                "collection exceeds remaining input",
            ));
        }
        Ok(count)
    }

    pub(crate) fn raw(&mut self, count: usize) -> Result<&'bytes [u8], SnapshotCodecError> {
        self.take(count)
    }

    pub(crate) fn bytes(&mut self) -> Result<&'bytes [u8], SnapshotCodecError> {
        let length_offset = self.offset;
        let count = self.usize()?;
        if count > self.remaining() {
            return Err(SnapshotCodecError::invalid(
                length_offset,
                "length exceeds remaining input",
            ));
        }
        self.take(count)
    }

    pub(crate) fn string(&mut self) -> Result<&'bytes str, SnapshotCodecError> {
        let offset = self.offset;
        std::str::from_utf8(self.bytes()?)
            .map_err(|_| SnapshotCodecError::invalid(offset, "invalid UTF-8 string"))
    }

    pub(crate) fn finish(self) -> Result<(), SnapshotCodecError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(SnapshotCodecError::invalid(self.offset, "trailing bytes"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_codec_roundtrips_big_endian_values() {
        let mut writer = SnapshotWriter::new();
        writer.u8(7);
        writer.bool(true);
        writer.u16(0x1234);
        writer.u32(0x1234_5678);
        writer.u64(0x1234_5678_9abc_def0);
        writer.string("žluťoučký").expect("string length fits");
        let bytes = writer.into_bytes();

        let mut reader = SnapshotReader::new(&bytes);
        assert_eq!(reader.u8(), Ok(7));
        assert_eq!(reader.bool(), Ok(true));
        assert_eq!(reader.u16(), Ok(0x1234));
        assert_eq!(reader.u32(), Ok(0x1234_5678));
        assert_eq!(reader.u64(), Ok(0x1234_5678_9abc_def0));
        assert_eq!(reader.string(), Ok("žluťoučký"));
        assert_eq!(reader.finish(), Ok(()));
    }

    #[test]
    fn strict_codec_rejects_noncanonical_and_truncated_values() {
        let mut invalid_bool = SnapshotReader::new(&[2]);
        assert!(invalid_bool.bool().is_err());

        let mut invalid_utf8 = SnapshotReader::new(&[0, 0, 0, 0, 0, 0, 0, 1, 0xff]);
        assert!(invalid_utf8.string().is_err());

        let mut truncated = SnapshotReader::new(&[0, 1, 2]);
        assert!(truncated.u32().is_err());

        let trailing = SnapshotReader::new(&[0]);
        assert!(trailing.finish().is_err());
    }
}
