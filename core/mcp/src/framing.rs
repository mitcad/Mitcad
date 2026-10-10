// SPDX-License-Identifier: MIT
use std::io::{self, BufRead};

pub(crate) enum FrameStatus {
    Complete,
    TooLarge,
}

pub(crate) fn read_frame<R: BufRead>(
    reader: &mut R,
    frame: &mut Vec<u8>,
    limit: usize,
) -> io::Result<Option<FrameStatus>> {
    frame.clear();
    let mut too_large = false;
    let mut received = false;
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(received.then_some(if too_large {
                FrameStatus::TooLarge
            } else {
                FrameStatus::Complete
            }));
        }
        received = true;
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let bytes = newline.unwrap_or(buffer.len());
        if !too_large {
            if bytes > limit.saturating_sub(frame.len()) {
                too_large = true;
                frame.clear();
            } else {
                frame.extend_from_slice(&buffer[..bytes]);
            }
        }
        reader.consume(bytes + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(Some(if too_large {
                FrameStatus::TooLarge
            } else {
                FrameStatus::Complete
            }));
        }
    }
}
