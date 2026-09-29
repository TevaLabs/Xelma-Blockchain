// SPDX-License-Identifier: MIT
//! CLI for offline round replay from JSON transcripts.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use xelma_replay::{
    assert_live_matches_replay, replay_round, transcript_commitment_hex, RoundTranscript,
};

fn usage() -> ! {
    eprintln!(
        "Usage: xelma-replay [--hash] [--verify] <transcript.json>\n\n\
         Options:\n\
           --hash     Print SHA-256 commitment hex and exit\n\
           --verify   Fail if replay != transcript.expected (default)\n\
           --replay   Print replay JSON only (skip parity check)"
    );
    process::exit(2);
}

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }

    let mut hash_only = false;
    let mut verify = true;
    let mut replay_only = false;

    while let Some(flag) = args.first().cloned() {
        if flag.starts_with('-') {
            args.remove(0);
            match flag.as_str() {
                "--hash" => hash_only = true,
                "--verify" => verify = true,
                "--replay" => {
                    verify = false;
                    replay_only = true;
                }
                "--help" | "-h" => usage(),
                _ => {
                    eprintln!("Unknown flag: {flag}");
                    usage();
                }
            }
        } else {
            break;
        }
    }

    if args.len() != 1 {
        usage();
    }

    let path = PathBuf::from(&args[0]);
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {e}", path.display());
        process::exit(1);
    });

    let transcript: RoundTranscript = serde_json::from_str(&raw).unwrap_or_else(|e| {
        eprintln!("Invalid transcript JSON: {e}");
        process::exit(1);
    });

    if hash_only {
        match transcript_commitment_hex(&transcript) {
            Ok(hex) => println!("{hex}"),
            Err(e) => {
                eprintln!("Hash error: {e}");
                process::exit(1);
            }
        }
        return;
    }

    let replay = replay_round(&transcript).unwrap_or_else(|e| {
        eprintln!("Replay error: {e}");
        process::exit(1);
    });

    if replay_only {
        let json = serde_json::to_string_pretty(&replay).expect("serialize replay");
        println!("{json}");
        return;
    }

    if verify {
        if let Err(mismatches) = assert_live_matches_replay(&transcript, &replay) {
            eprintln!("REPLAY_MISMATCH: live != replay for {}", path.display());
            for m in &mismatches {
                eprintln!(
                    "  {}: expected={}, replayed={}",
                    m.field, m.expected, m.replayed
                );
            }
            process::exit(1);
        }
        println!("OK: live == replay ({})", path.display());
    }
}
