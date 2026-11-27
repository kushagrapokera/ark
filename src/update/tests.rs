use super::*;
use std::path::{Path, PathBuf};

fn valid_manifest() -> &'static str {
    r#"{
      "version": "0.1.1",
      "channel": "stable",
      "published_at": "2026-11-27T00:00:00Z",
      "minimum_version": "0.1.0",
      "critical": false,
      "notes_url": "https://example.com/releases/0.1.1",
      "assets": {
        "x86_64-unknown-linux-gnu": {
          "url": "https://example.com/ark.tar.gz",
          "size": 1234,
          "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
          "signature": "https://example.com/ark.tar.gz.sig"
        }
      }
    }"#
}

#[test]
fn versions_parse_and_follow_semver_precedence() {
    assert!(Version::parse("0.1.1").unwrap() > Version::parse("0.1.0-beta").unwrap());
    assert!(Version::parse("0.1.0-beta").unwrap() > Version::parse("0.1.0-alpha").unwrap());
    assert!(Version::parse("0.1.0-beta.2").unwrap() > Version::parse("0.1.0-beta.1").unwrap());
    assert!(Version::parse("1.0.0").unwrap() > Version::parse("0.99.99").unwrap());
    assert_eq!(
        Version::parse("0.1.0+build.1").unwrap(),
        Version::parse("0.1.0").unwrap()
    );
}

#[test]
fn versions_reject_malformed_input() {
    for input in [
        "",
        "1",
        "1.2",
        "01.2.3",
        "1.2.3-",
        "1.2.3-alpha..1",
        "x.y.z",
    ] {
        assert!(
            Version::parse(input).is_err(),
            "invalid version accepted: {input}"
        );
    }
}

#[test]
fn targets_select_the_expected_linux_asset() {
    assert_eq!(
        Target::from_arch("x86_64").unwrap().asset_key(),
        "x86_64-unknown-linux-gnu"
    );
    assert_eq!(Target::from_arch("arm64").unwrap(), Target::Aarch64);
    assert!(Target::from_arch("riscv64").is_err());
}

#[test]
fn install_layout_uses_xdg_directories() {
    let layout = InstallLayout::new(
        PathBuf::from("/home/u/.local/share"),
        PathBuf::from("/home/u/.local/bin"),
        PathBuf::from("/home/u/.local/state"),
        PathBuf::from("/home/u/.cache"),
        PathBuf::from("/home/u/.config"),
    );
    assert_eq!(layout.app_data_dir(), Path::new("/home/u/.local/share/ark"));
    assert_eq!(
        layout.versions_dir(),
        Path::new("/home/u/.local/share/ark/versions")
    );
    assert_eq!(
        layout.current_link(),
        Path::new("/home/u/.local/share/ark/current")
    );
    assert_eq!(layout.bin_link(), Path::new("/home/u/.local/bin/ark"));
    assert_eq!(layout.state_dir(), Path::new("/home/u/.local/state/ark"));
}

#[test]
fn manifest_parses_and_selects_the_target_asset() {
    let manifest = ReleaseManifest::parse(valid_manifest(), false).unwrap();
    assert_eq!(manifest.version, Version::parse("0.1.1").unwrap());
    assert!(manifest.is_newer_than(&Version::parse("0.1.0-beta").unwrap()));
    let asset = manifest.asset(Target::X86_64).unwrap();
    assert_eq!(asset.size, 1234);
    assert!(asset.signature.is_some());
}

#[test]
fn manifest_rejects_http_unless_explicitly_allowed() {
    let input = valid_manifest().replace("https://", "http://");
    assert!(ReleaseManifest::parse(&input, false).is_err());
    assert!(ReleaseManifest::parse(&input, true).is_ok());
}

#[test]
fn manifest_rejects_bad_checksums_and_missing_stable_signatures() {
    let bad_checksum = valid_manifest().replace(
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "not-a-checksum",
    );
    assert!(ReleaseManifest::parse(&bad_checksum, false).is_err());

    let no_signature = valid_manifest().replace(
        ",\n          \"signature\": \"https://example.com/ark.tar.gz.sig\"",
        "",
    );
    assert!(ReleaseManifest::parse(&no_signature, false).is_err());
}

#[test]
fn manifest_rejects_oversized_assets_and_unknown_fields_are_ignored() {
    let oversized = valid_manifest().replace("\"size\": 1234", "\"size\": 999999999");
    assert!(ReleaseManifest::parse(&oversized, false).is_err());
    let extra = valid_manifest().replace(
        "\"critical\": false,",
        "\"critical\": false,\n  \"future\": {\"ok\": true},",
    );
    assert!(ReleaseManifest::parse(&extra, false).is_ok());
}

#[test]
fn archive_listing_rejects_traversal_absolute_paths_and_links() {
    assert!(validate_archive_listing(
        "ark-0.1.1/ark\nark-0.1.1/README.md\n",
        "-rwxr-xr-x ark-0.1.1/ark\n-rw-r--r-- ark-0.1.1/README.md\n"
    )
    .is_ok());
    assert!(validate_archive_listing("../escape\n", "-rw-r--r-- ../escape\n").is_err());
    assert!(validate_archive_listing("/absolute\n", "-rw-r--r-- /absolute\n").is_err());
    assert!(
        validate_archive_listing("ark/link\n", "lrwxrwxrwx ark/link -> /etc/passwd\n").is_err()
    );
    assert!(
        validate_archive_listing("ark/hard\n", "hrw-r--r-- ark/hard link to ark/ark\n").is_err()
    );
}
