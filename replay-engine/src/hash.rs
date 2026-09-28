// SPDX-License-Identifier: MIT
//! SHA-256 commitment over canonical transcript JSON.

use sha2::{Digest, Sha256};

use crate::transcript::{RoundTranscript, TranscriptError};

/// Canonical JSON uses serde field order and participants sorted by ascending `index`.
pub fn transcript_commitment_hex(transcript: &RoundTranscript) -> Result<String, TranscriptError> {
    transcript.validate_schema()?;
    let canonical =
        serde_json::to_string(transcript).map_err(|_| TranscriptError::EmptyParticipants)?;
    Ok(format!("{:x}", Sha256::digest(canonical.as_bytes())))
}
