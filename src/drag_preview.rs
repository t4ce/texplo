//! UI4 owns the floating preview; SSH retains Termdir's in-terminal preview.
use std::path::PathBuf;

pub struct Preview {
    #[cfg(not(test))]
    buffer: trueos::ui4_drag::DragBuffer,
}

pub fn encode_paths(paths: &[PathBuf]) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    for path in paths {
        let text = path.to_str()?;
        let len = u32::try_from(text.len()).ok()?;
        if bytes.len() + 4 + text.len() > trueos::ui4_drag::MAX_PAYLOAD_BYTES {
            return None;
        }
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.extend_from_slice(text.as_bytes());
    }
    Some(bytes)
}

pub fn decode_paths(mut bytes: &[u8]) -> Option<Vec<PathBuf>> {
    let mut paths = Vec::new();
    while !bytes.is_empty() {
        let len = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
        let text = std::str::from_utf8(bytes.get(4..4usize.checked_add(len)?)?).ok()?;
        if text.is_empty() || text.contains('\0') {
            return None;
        }
        paths.push(PathBuf::from(text));
        bytes = &bytes[4 + len..];
    }
    (!paths.is_empty()).then_some(paths)
}

pub fn take_drop() -> Option<(u16, u16, Vec<PathBuf>)> {
    #[cfg(not(test))]
    {
        let drop = trueos::ui4_drag::take_drop(0).ok()??;
        if drop.kind != trueos::ui4_drag::PATH_LIST_V1 {
            return None;
        }
        Some((
            u16::try_from(drop.x).ok()?,
            u16::try_from(drop.y).ok()?,
            decode_paths(&drop.bytes)?,
        ))
    }
    #[cfg(test)]
    {
        None
    }
}

impl Preview {
    pub fn begin(label: &str, paths: &[PathBuf]) -> Option<Self> {
        let payload = encode_paths(paths)?;
        #[cfg(not(test))]
        {
            let buffer = trueos::ui4_drag::DragBuffer::begin_attached(
                trueos::ui4_drag::PATH_LIST_V1,
                label,
                &payload,
            )
            .ok()?;
            Some(Self { buffer })
        }
        #[cfg(test)]
        {
            let _ = (label, payload);
            None
        }
    }
    pub fn finish(self) -> bool {
        #[cfg(not(test))]
        {
            self.buffer.finish().unwrap_or(false)
        }
        #[cfg(test)]
        {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn payload_preserves_unicode_and_newlines_without_separator_ambiguity() {
        let paths = [PathBuf::from("4:/hello\nworld"), PathBuf::from("4:/§file")];
        let bytes = encode_paths(&paths).unwrap();
        let mut cursor = bytes.as_slice();
        for path in &paths {
            let len = u32::from_le_bytes(cursor[..4].try_into().unwrap()) as usize;
            assert_eq!(&cursor[4..4 + len], path.to_str().unwrap().as_bytes());
            cursor = &cursor[4 + len..];
        }
        assert!(cursor.is_empty());
        assert_eq!(decode_paths(&bytes).unwrap(), paths);
    }
    #[test]
    fn oversized_path_list_is_rejected_without_truncating_paths() {
        assert!(
            encode_paths(&[PathBuf::from(
                "x".repeat(trueos::ui4_drag::MAX_PAYLOAD_BYTES)
            )])
            .is_none()
        );
    }
    #[test]
    fn incomplete_or_invalid_path_payload_is_rejected() {
        assert!(decode_paths(&[1, 0, 0]).is_none());
        assert!(decode_paths(&[2, 0, 0, 0, b'x']).is_none());
        assert!(decode_paths(&[1, 0, 0, 0, 255]).is_none());
        assert!(decode_paths(&[]).is_none());
    }
}
