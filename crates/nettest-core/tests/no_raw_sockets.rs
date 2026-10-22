//! The probe layer must never open a raw socket: that needs admin rights on
//! Windows and root on macOS. Datagram ICMP sockets and `IcmpSendEcho2Ex` only.

use std::fs;
use std::path::Path;

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn no_raw_sockets_in_core() {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(!files.is_empty());
    for f in files {
        let text = fs::read_to_string(&f).unwrap();
        for needle in ["Type::RAW", "SOCK_RAW", "Type::from(libc::SOCK_RAW"] {
            assert!(!text.contains(needle), "{} uses {needle}", f.display());
        }
    }
}
