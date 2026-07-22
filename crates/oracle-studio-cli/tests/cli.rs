use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const DECK: &str = r#"{"schema_version":1,"id":"cli_fixture_deck","name":"CLI Fixture Deck","attribution":{"author":"Oracle Studio contributors","artist":null,"publisher":null},"tradition":"Original metadata-only test fixture","rights":{"license":"AGPL-3.0-or-later","source":null,"notes":"No artwork."},"reversal_rate_basis_points":0,"cards":[{"id":"fool","identity":{"kind":"conventional","id":"fool"},"printed_title":"The Fool","printed_number":null,"printed_suit":null,"printed_rank":null,"enabled":true,"asset_id":null,"correspondences":[],"notes":null}]}"#;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).unwrap();
        let suffix = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = std::env::temp_dir().join(format!("oracle-studio-cli-test-{suffix}"));
        fs::create_dir(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command(vault: &Path, password: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_oracle-studio"));
    command
        .arg("--vault")
        .arg(vault)
        .arg("--password-file")
        .arg(password);
    command
}

fn success(mut command: Command) -> String {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn guided_cli_completes_and_recovers_an_encrypted_manual_reading() {
    let directory = TestDirectory::new();
    let vault = directory.0.join("journal.vault");
    let password = directory.0.join("password");
    fs::write(&password, b"fictional CLI password\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&password, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let deck = directory.0.join("deck.json");
    fs::write(&deck, DECK).unwrap();

    let mut init = command(&vault, &password);
    init.arg("init");
    success(init);

    let mut person = command(&vault, &password);
    person.args([
        "person-add",
        "Fictional Client",
        "--id",
        "fictional_client",
        "--professional-client",
    ]);
    success(person);

    let mut session = command(&vault, &password);
    session.args([
        "session-add",
        "Fictional Session",
        "--id",
        "fictional_session",
        "--person",
        "fictional_client",
    ]);
    success(session);

    let mut import = command(&vault, &password);
    import
        .arg("deck-import")
        .arg(&deck)
        .args(["--id", "fictional_deck_record"]);
    success(import);

    let mut decks = command(&vault, &password);
    decks.arg("deck-list");
    assert!(success(decks).contains("fictional_deck_record\tCLI Fixture Deck\tpack=none"));

    let mut reading = command(&vault, &password);
    reading
        .args([
            "reading-new",
            "--deck",
            "fictional_deck_record",
            "--person",
            "fictional_client",
            "--session",
            "fictional_session",
            "--method",
            "manual",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = reading.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"\nFictional question\nFictional context\nReader note\n\nfool\nunspecified\n\nyes\n",
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("The Fool"));

    let mut search = command(&vault, &password);
    search.args(["search", "Fictional question"]);
    let output = success(search);
    assert!(output.contains("Artifact"));

    let mut cancelled = command(&vault, &password);
    cancelled
        .args([
            "reading-new",
            "--deck",
            "fictional_deck_record",
            "--method",
            "manual",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = cancelled.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"\nCancelled marker\n\n\n\nfool\nupright\n\nno\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("vault was not changed"));

    let mut search = command(&vault, &password);
    search.args(["search", "Cancelled marker"]);
    assert!(success(search).is_empty());

    fs::remove_file(deck).unwrap();
    for entry in fs::read_dir(&directory.0).unwrap() {
        let path = entry.unwrap().path();
        if path != password && path.is_file() {
            let bytes = fs::read(path).unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("Fictional question"));
        }
    }
}

#[cfg(unix)]
#[test]
fn broad_password_file_permissions_are_rejected_before_unlock() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TestDirectory::new();
    let password = directory.0.join("password");
    fs::write(&password, b"fictional password").unwrap();
    fs::set_permissions(&password, fs::Permissions::from_mode(0o644)).unwrap();
    let mut command = command(&directory.0.join("journal.vault"), &password);
    command.arg("init");
    let output = command.output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("password file permissions"));
}
