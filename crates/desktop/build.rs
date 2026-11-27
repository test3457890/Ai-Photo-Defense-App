//! Records versions of key dependencies from Cargo.lock for the Diagnostics Center.
fn main() {
    let lock = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let text = std::fs::read_to_string(&lock).unwrap_or_default();
    let wanted = [
        "iced",
        "image",
        "rxing",
        "rusqlite",
        "kamadak-exif",
        "sysinfo",
        "regex",
        "candle-core",
    ];
    let mut found = vec![];
    let mut name = String::new();
    for line in text.lines() {
        if let Some(n) = line.strip_prefix("name = ") {
            name = n.trim_matches('"').to_string();
        } else if let Some(v) = line.strip_prefix("version = ")
            && wanted.contains(&name.as_str())
        {
            found.push((name.clone(), v.trim_matches('"').to_string()));
        }
    }
    // Several versions can coexist; report the newest, which is the one this workspace uses directly.
    let key = |v: &str| {
        v.split(|c: char| !c.is_ascii_digit())
            .filter_map(|p| p.parse::<u64>().ok())
            .collect::<Vec<_>>()
    };
    let mut best: Vec<(String, String)> = vec![];
    for (n, v) in found {
        match best.iter_mut().find(|(bn, _)| *bn == n) {
            Some(b) if key(&v) > key(&b.1) => b.1 = v,
            Some(_) => {}
            None => best.push((n, v)),
        }
    }
    best.sort();
    let list: Vec<String> = best.into_iter().map(|(n, v)| format!("{n} {v}")).collect();
    println!("cargo:rustc-env=APD_DEPENDENCIES={}", list.join(", "));
}
