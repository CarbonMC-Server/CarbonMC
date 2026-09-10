use super::*;
use serde_json::{json, Value};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("carbon-save-safety-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn primary(&self) -> PathBuf {
        self.0.join("world-save.json")
    }
    fn backup(&self) -> PathBuf {
        self.primary().with_extension("json.bak")
    }
    fn write(&self, path: &Path, value: &Value) {
        fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    }
    fn state(&self) -> anyhow::Result<ServerState> {
        ServerState::with_operator_file(
            "world".into(),
            99,
            watch::channel(false).0,
            self.0.join("operators.json"),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn current() -> Value {
    json!({"version":2,"generator_version":1,"blocks":[],"inventories":[],"furnaces":[],"chests":[]})
}

#[test]
fn legacy_migrates_only_on_save_and_preserves_edits_and_inventory() {
    let f = Fixture::new();
    let id = Uuid::new_v4();
    let legacy = json!({"version":1,"blocks":[{"x":4,"y":70,"z":4,"kind":"crafting_table"}],
        "inventories":[{"player_id":id.to_string(),"slots":[{"kind":"wooden_pickaxe","count":1,"damage":7}],
        "health":13.0,"food":9,"saturation":2.0}]});
    f.write(&f.primary(), &legacy);
    let original = fs::read(f.primary()).unwrap();
    let state = f.state().unwrap();
    assert_eq!(fs::read(f.primary()).unwrap(), original);
    assert!(!f.backup().exists());
    let position = BlockPosition { x: 4, y: 70, z: 4 };
    assert_eq!(state.block_at(position), BlockKind::CraftingTable);
    assert_eq!(state.inventory(id).unwrap().slots[0].unwrap().damage, 7);
    state.save().unwrap();
    assert_eq!(fs::read(f.backup()).unwrap(), original);
    let saved = decode_save(&fs::read(f.primary()).unwrap()).unwrap();
    assert_eq!(saved.version, CURRENT_SAVE_SCHEMA_VERSION);
    assert_eq!(saved.generator_version, CURRENT_GENERATOR_VERSION);
    assert_eq!(saved.inventories[0].health, 13.0);
    assert_eq!(saved.inventories[0].food, 9);
    assert_eq!(saved.inventories[0].saturation, 2.0);
    let reloaded = f.state().unwrap();
    assert_eq!(reloaded.block_at(position), BlockKind::CraftingTable);
    assert_eq!(reloaded.inventory(id).unwrap().slots[0].unwrap().damage, 7);
}

#[test]
fn fresh_save_and_rotation_use_current_metadata() {
    let f = Fixture::new();
    assert!(load_save_file(&f.primary()).unwrap().is_none());
    let state = f.state().unwrap();
    state.save().unwrap();
    let first = fs::read(f.primary()).unwrap();
    assert_eq!(decode_save(&first).unwrap().version, 2);
    state.set_block(
        BlockPosition { x: 4, y: 70, z: 4 },
        BlockKind::CraftingTable,
    );
    state.save().unwrap();
    assert_eq!(fs::read(f.backup()).unwrap(), first);
    assert_ne!(fs::read(f.primary()).unwrap(), first);
}

#[test]
fn incompatible_primary_never_falls_back_or_changes_files() {
    for (version, generator) in [(0, 1), (3, 1), (2, 2), (1, 2), (4294967296_u64, 1)] {
        let f = Fixture::new();
        // Deliberately incompatible payload shape, as a future schema could introduce.
        let primary =
            json!({"version":version,"generator_version":generator,"blocks":{},"inventories":[]});
        f.write(&f.primary(), &primary);
        f.write(&f.backup(), &current());
        let before = fs::read(f.primary()).unwrap();
        let backup = fs::read(f.backup()).unwrap();
        assert!(f.state().is_err());
        assert!(write_world_save(&f.primary(), &serde_json::to_vec(&current()).unwrap()).is_err());
        assert_eq!(fs::read(f.primary()).unwrap(), before);
        assert_eq!(fs::read(f.backup()).unwrap(), backup);
    }
}

#[test]
fn metadata_is_required_and_checked() {
    for value in [
        json!({"blocks":[],"inventories":[]}),
        json!({"version":2,"blocks":[],"inventories":[]}),
        json!({"version":2,"generator_version":-1,"blocks":[],"inventories":[]}),
        json!({"version":2,"generator_version":"1","blocks":[],"inventories":[]}),
    ] {
        assert!(decode_save(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let mut value = current();
    value["generator_version"] = json!(0);
    assert_eq!(
        decode_save(&serde_json::to_vec(&value).unwrap())
            .unwrap()
            .generator_version,
        0
    );
}

#[test]
fn corrupt_primary_recovery_then_save_retains_good_backup_and_evidence() {
    for corrupt in [
        b"{torn".as_slice(),
        br#"{"version":2,"generator_version":1,"blocks":{}}"#,
    ] {
        let f = Fixture::new();
        fs::write(f.primary(), corrupt).unwrap();
        f.write(&f.backup(), &current());
        let backup = fs::read(f.backup()).unwrap();
        let state = f.state().unwrap();
        assert_eq!(fs::read(f.primary()).unwrap(), corrupt);
        state.save().unwrap();
        assert_eq!(fs::read(f.backup()).unwrap(), backup);
        assert!(decode_save(&fs::read(f.primary()).unwrap()).is_ok());
        let quarantined: Vec<_> = fs::read_dir(&f.0)
            .unwrap()
            .map(Result::unwrap)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
            .collect();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(fs::read(quarantined[0].path()).unwrap(), corrupt);
        assert!(f.state().is_ok());
    }
}

#[test]
fn interrupted_rotation_uses_backup_and_ignores_temporary() {
    let f = Fixture::new();
    f.write(&f.backup(), &current());
    fs::write(f.primary().with_extension("json.tmp"), b"{unfinished").unwrap();
    let backup = fs::read(f.backup()).unwrap();
    f.state().unwrap().save().unwrap();
    assert_eq!(fs::read(f.backup()).unwrap(), backup);
    assert!(!f.primary().with_extension("json.tmp").exists());
    assert!(f.state().is_ok());
}

#[test]
fn temporary_file_never_supersedes_primary_or_creates_a_world() {
    let f = Fixture::new();
    f.write(&f.primary().with_extension("json.tmp"), &current());
    assert!(load_save_file(&f.primary()).unwrap().is_none());
    f.write(&f.primary(), &current());
    fs::write(f.primary().with_extension("json.tmp"), b"{bad").unwrap();
    assert_eq!(load_save_file(&f.primary()).unwrap().unwrap().version, 2);
}

#[test]
fn unusable_backups_fail_closed_without_mutation() {
    for primary in [None, Some(b"{bad".as_slice())] {
        for backup in [
            b"{bad".as_slice(),
            br#"{"version":3,"blocks":{},"inventories":[]}"#,
            br#"{"version":2,"generator_version":2,"blocks":[],"inventories":[]}"#,
        ] {
            let f = Fixture::new();
            if let Some(bytes) = primary {
                fs::write(f.primary(), bytes).unwrap();
            }
            fs::write(f.backup(), backup).unwrap();
            assert!(f.state().is_err());
            assert_eq!(fs::read(f.primary()).ok().as_deref(), primary);
            assert_eq!(fs::read(f.backup()).unwrap(), backup);
        }
    }
    let f = Fixture::new();
    fs::write(f.primary(), b"{bad").unwrap();
    assert!(f.state().is_err());
}

#[test]
fn failed_temporary_write_keeps_primary_and_backup() {
    let f = Fixture::new();
    f.write(&f.primary(), &current());
    f.write(&f.backup(), &current());
    let primary = fs::read(f.primary()).unwrap();
    let backup = fs::read(f.backup()).unwrap();
    fs::create_dir(f.primary().with_extension("json.tmp")).unwrap();
    assert!(f.state().unwrap().save().is_err());
    assert_eq!(fs::read(f.primary()).unwrap(), primary);
    assert_eq!(fs::read(f.backup()).unwrap(), backup);
}

// Compiled only into the test executable; shipped binaries have no pause hooks.
pub(super) fn crash_checkpoint(path: &Path, stage: &str) {
    if std::env::var_os("CARBON_CRASH_PATH").as_deref() != Some(path.as_os_str())
        || std::env::var("CARBON_CRASH_STAGE").as_deref() != Ok(stage)
    {
        return;
    }
    fs::write(path.with_extension("ready"), stage).unwrap();
    loop {
        std::thread::park_timeout(std::time::Duration::from_secs(1));
    }
}

#[test]
#[ignore = "child process entry point; invoked by forced_process_crashes_preserve_committed_state"]
fn crash_writer_child() {
    let path = PathBuf::from(std::env::var_os("CARBON_CRASH_PATH").expect("parent supplies path"));
    let bytes = fs::read(path.with_extension("next")).unwrap();
    write_world_save(&path, &bytes).unwrap();
    panic!("requested crash checkpoint was not reached");
}

#[test]
fn forced_process_crashes_preserve_committed_state() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for stage in [
        "temporary_open",
        "temporary_synced",
        "backup_removed",
        "primary_rotated",
        "committed",
        "quarantined",
    ] {
        let f = Fixture::new();
        let mut old = current();
        old["blocks"] = json!([{"x":4,"y":70,"z":4,"kind":"stone"}]);
        let mut next = old.clone();
        next["blocks"][0]["kind"] = json!("diamond_ore");
        f.write(&f.primary(), &old);
        f.write(&f.backup(), &old);
        f.write(&f.primary().with_extension("next"), &next);
        if stage == "quarantined" {
            fs::write(f.primary(), b"{torn").unwrap();
        }
        let mut child = ChildGuard(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "state::save_tests::crash_writer_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("CARBON_CRASH_PATH", f.primary())
                .env("CARBON_CRASH_STAGE", stage)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        while !f.primary().with_extension("ready").exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited before {stage}"
            );
            assert!(Instant::now() < deadline, "checkpoint timed out: {stage}");
            std::thread::sleep(Duration::from_millis(10));
        }
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());
        let expected = if stage == "committed" {
            BlockKind::DiamondOre
        } else {
            BlockKind::Stone
        };
        let state = f.state().unwrap();
        let position = BlockPosition { x: 4, y: 70, z: 4 };
        assert_eq!(state.block_at(position), expected, "recovery after {stage}");
        state.save().unwrap();
        assert_eq!(
            f.state().unwrap().block_at(position),
            expected,
            "second restart after {stage}"
        );
        if stage == "quarantined" {
            let evidence: Vec<_> = fs::read_dir(&f.0)
                .unwrap()
                .map(Result::unwrap)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
                .collect();
            assert_eq!(evidence.len(), 1);
            assert_eq!(fs::read(evidence[0].path()).unwrap(), b"{torn");
        }
        println!("forced process crash recovery passed: {stage}");
    }
}
