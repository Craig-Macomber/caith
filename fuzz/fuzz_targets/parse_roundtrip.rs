#![no_main]
use libfuzzer_sys::fuzz_target;

use ::dicey::Command;

fuzz_target!(|data: &str| {
    if let Ok(roller) = Command::parse(data) {
        let f = format!("{roller}");
        let parsed2 = Command::parse(&f).unwrap();
        let f2 = format!("{parsed2}");
        assert_eq!(f, f2);
    }
});
