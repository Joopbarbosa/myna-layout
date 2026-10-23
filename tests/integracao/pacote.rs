use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn required_program(program: &str) {
    let available = Command::new(program).arg("--help").output();
    assert!(
        available.is_ok(),
        "required program '{program}' is missing; install it to run the package integration test"
    );
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

#[test]
fn deb_package_contains_only_the_application_and_session_integration() {
    required_program("cargo-deb");
    required_program("dpkg-deb");

    let root = repository_root();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let output_dir = std::env::temp_dir().join(format!("myna-layout-deb-{nonce}"));
    fs::create_dir_all(&output_dir).expect("create temporary package directory");
    {
        let build = Command::new("cargo-deb")
            .args(["--manifest-path", "Cargo.toml", "--output"])
            .arg(&output_dir)
            .current_dir(&root)
            .output()
            .expect("start cargo-deb");
        assert!(
            build.status.success(),
            "cargo-deb failed:\n{}\n{}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
        let package = fs::read_dir(&output_dir)
            .expect("read temporary package directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|extension| extension == "deb"))
            .expect("cargo-deb produced no .deb package");

        let contents = Command::new("dpkg-deb")
            .arg("--contents")
            .arg(&package)
            .output()
            .expect("start dpkg-deb --contents");
        assert!(contents.status.success(), "dpkg-deb --contents failed");
        let contents = String::from_utf8_lossy(&contents.stdout);
        for required in [
            "./usr/bin/myna-layout",
            "./usr/lib/systemd/user/myna-layout.service",
            "./usr/share/applications/myna-layout.desktop",
            "./usr/share/icons/hicolor/scalable/apps/myna-layout.svg",
        ] {
            assert!(
                contents.contains(required),
                "package lacks {required}:\n{contents}"
            );
        }
        assert!(
            !contents.contains("fake-myna"),
            "the fake Myna binary must not be packaged"
        );

        let field = Command::new("dpkg-deb")
            .args(["--field"])
            .arg(&package)
            .output()
            .expect("start dpkg-deb --field");
        assert!(field.status.success(), "dpkg-deb --field failed");
        let field = String::from_utf8_lossy(&field.stdout);
        assert!(
            field.contains("Package: myna-layout"),
            "unexpected package fields:\n{field}"
        );

        let extracted = output_dir.join("extracted");
        let extraction = Command::new("dpkg-deb")
            .args(["--extract"])
            .arg(&package)
            .arg(&extracted)
            .output()
            .expect("start dpkg-deb --extract");
        assert!(extraction.status.success(), "dpkg-deb --extract failed");
        let service =
            fs::read_to_string(extracted.join("usr/lib/systemd/user/myna-layout.service"))
                .expect("read packaged service unit");
        for directive in [
            "After=graphical-session.target",
            "PartOf=graphical-session.target",
            "WantedBy=graphical-session.target",
            "ExecStart=/usr/bin/myna-layout",
        ] {
            assert!(
                service.contains(directive),
                "service unit lacks {directive}"
            );
        }
        let desktop = extracted.join("usr/share/applications/myna-layout.desktop");
        let desktop_text = fs::read_to_string(&desktop).expect("read desktop entry");
        assert!(
            desktop_text.contains("Exec=systemctl --user start myna-layout.service"),
            "menu entry must start the idempotent user service"
        );
        if Command::new("desktop-file-validate")
            .arg("--version")
            .output()
            .is_ok()
        {
            let validation = Command::new("desktop-file-validate")
                .arg(&desktop)
                .output()
                .expect("start desktop-file-validate");
            assert!(
                validation.status.success(),
                "desktop-file-validate failed:\n{}",
                String::from_utf8_lossy(&validation.stderr)
            );
        }
    }
    let _ = fs::remove_dir_all(output_dir);
}
