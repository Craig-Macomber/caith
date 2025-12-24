#![no_main]
use libfuzzer_sys::fuzz_target;

use ::dicey::{Command, Rollable};

fuzz_target!(|data: &str| {
    if let Ok(roller) = Command::parse(data) {
        let _ = roller.roll();
    }
});
