/// Returns whether the first line identifies the Nacre Niri fork.
///
/// Fork versions may report either `(nacre-niri)` or append a hexadecimal
/// source revision of up to 40 characters.
pub fn is_nacre_niri_version(version: &str) -> bool {
    let Some(line) = version.lines().next().map(str::trim_end) else {
        return false;
    };
    if line.ends_with("(nacre-niri)") {
        return true;
    }
    let Some(identity) = line.strip_suffix(')') else {
        return false;
    };
    let Some((prefix, revision)) = identity.rsplit_once(" (nacre-niri ") else {
        return false;
    };
    !prefix.is_empty()
        && !revision.is_empty()
        && revision.len() <= 40
        && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::is_nacre_niri_version;

    #[test]
    fn recognizes_supported_fork_identity_forms() {
        assert!(is_nacre_niri_version("niri 26.04 (nacre-niri)"));
        assert!(is_nacre_niri_version(
            "niri 26.04 (nacre-niri c2b6349ed7d996d523c3a8fc74be64a2262c0f67)"
        ));
        assert!(is_nacre_niri_version("niri 26.04 (nacre-niri c2b6349)\n"));
    }

    #[test]
    fn rejects_stock_and_malformed_identities() {
        assert!(!is_nacre_niri_version("niri 26.04 (8ed0da4)"));
        assert!(!is_nacre_niri_version("niri 26.04 (nacre-niri abcxyz)"));
        assert!(!is_nacre_niri_version(&format!(
            "niri 26.04 (nacre-niri {})",
            "a".repeat(41)
        )));
        assert!(is_nacre_niri_version(
            "niri 26.04 (nacre-niri)\nadditional output"
        ));
    }
}
