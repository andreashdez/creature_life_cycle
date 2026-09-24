use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn clean_config_home(name: &str) -> PathBuf {
    let config_home = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-config-home")
        .join(name);

    let _ = fs::remove_dir_all(&config_home);
    fs::create_dir_all(&config_home).expect("create test config home");

    config_home
}

fn test_config_home(name: &str) -> PathBuf {
    let config_home = clean_config_home(name);
    let app_config_dir = config_home.join("creature_life_cycle");

    fs::create_dir_all(&app_config_dir).expect("create test config directory");
    fs::write(
        app_config_dir.join("simulation.toml"),
        include_str!("../simulation.example.toml"),
    )
    .expect("write test simulation config");

    config_home
}

fn assert_seeded_snapshot(name: &str, args: &[&str], expected_stdout: &str) {
    let config_home = test_config_home(name);
    let output = Command::new(env!("CARGO_BIN_EXE_creature_life_cycle"))
        .args(args)
        .env("XDG_CONFIG_HOME", &config_home)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run creature_life_cycle binary");
    let status = output.status;
    let stdout = String::from_utf8(output.stdout).expect("stdout is valid UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr is valid UTF-8");

    assert!(status.success(), "process exited with {status}");
    assert_eq!(stderr, "");
    assert_eq!(stdout, expected_stdout);
}

#[test]
fn missing_config_is_created_with_defaults() {
    let config_home = clean_config_home("missing_config_defaults");
    let config_path = config_home
        .join("creature_life_cycle")
        .join("simulation.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_creature_life_cycle"))
        .args(["--turns", "0", "--delay-ms", "0", "--seed", "1"])
        .env("XDG_CONFIG_HOME", &config_home)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run creature_life_cycle binary");
    let stderr = String::from_utf8(output.stderr).expect("stderr is valid UTF-8");

    assert!(
        output.status.success(),
        "process exited with {}",
        output.status
    );
    assert!(stderr.contains("Created default config"));
    assert!(config_path.exists());

    let contents = fs::read_to_string(config_path).expect("read created simulation config");
    assert!(contents.contains("[board]"));
    assert!(contents.contains("[food]"));
    assert!(contents.contains("regeneration_probability = 0.1"));
}

#[test]
fn invalid_config_is_an_error_and_is_left_untouched() {
    let config_home = clean_config_home("invalid_config_error");
    let config_path = config_home
        .join("creature_life_cycle")
        .join("simulation.toml");
    let broken = "[board]\nrows = 3\ncolums = 4\n";
    fs::create_dir_all(config_path.parent().unwrap()).expect("create test config directory");
    fs::write(&config_path, broken).expect("write invalid simulation config");

    let output = Command::new(env!("CARGO_BIN_EXE_creature_life_cycle"))
        .args(["--turns", "1", "--delay-ms", "0", "--seed", "1"])
        .env("XDG_CONFIG_HOME", &config_home)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run creature_life_cycle binary");
    let stderr = String::from_utf8(output.stderr).expect("stderr is valid UTF-8");

    assert!(
        !output.status.success(),
        "an invalid config must fail the run"
    );
    assert!(output.stdout.is_empty(), "no board is simulated");
    assert!(stderr.starts_with("Error: invalid "), "{stderr}");
    assert_eq!(
        fs::read_to_string(&config_path).expect("read simulation config"),
        broken
    );
}

#[test]
fn config_flag_loads_the_named_file_instead_of_the_xdg_one() {
    // The XDG config is left missing: with `--config` it is neither read nor
    // created.
    let config_home = clean_config_home("config_flag");
    let setup = config_home.join("one_aphid.toml");
    fs::write(
        &setup,
        "[board]\nrows = 1\ncolumns = 2\naphids = [{ x = 0, y = 1 }]\nladybugs = []\n",
    )
    .expect("write named simulation config");

    let run = |config: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_creature_life_cycle"))
            .args(["--turns", "0", "--delay-ms", "0", "--seed", "1", "--config"])
            .arg(config)
            .env("XDG_CONFIG_HOME", &config_home)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("run creature_life_cycle binary")
    };

    let output = run(&setup);
    assert!(
        output.status.success(),
        "process exited with {}",
        output.status
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "\n __ 1_\n");
    assert!(output.stderr.is_empty());
    assert!(!config_home.join("creature_life_cycle").exists());

    let output = run(&config_home.join("missing.toml"));
    let stderr = String::from_utf8(output.stderr).expect("stderr is valid UTF-8");
    assert!(!output.status.success());
    assert!(stderr.contains("missing.toml does not exist"), "{stderr}");
}

#[test]
fn seeded_e2e_run_matches_death_snapshot() {
    assert_seeded_snapshot(
        "death_snapshot",
        &["--turns", "1", "--delay-ms", "0", "--seed", "1"],
        r#"
 __ __ __ __ __ __ __ __ __ __
 __ _1 __ __ __ __ 1_ __ __ 1_
 __ __ __ __ __ __ __ __ __ 1_
 __ __ __ __ __ 1_ __ __ _1 __
 __ __ __ __ __ __ __ __ 1_ __
 __ __ __ __ __ __ __ __ __ _1
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ _1 __ __ __ __ __ __ __

 __ __ __ __ __ __ __ __ __ __
 __ _1 __ __ __ __ __ __ __ 1_
 __ __ __ __ __ 1_ __ __ __ 1_
 __ __ __ __ __ __ __ __ _1 __
 __ __ __ __ __ __ 1_ __ 1_ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ _1 __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
Turn: 1 | Aphids: 5 | Ladybugs: 3 | Births: 0 | Deaths: 1 | Food: 425
"#,
    );
}

#[test]
fn seeded_e2e_run_matches_birth_snapshot() {
    assert_seeded_snapshot(
        "birth_snapshot",
        &["--turns", "6", "--delay-ms", "0", "--seed", "2"],
        r#"
 __ __ __ __ __ __ __ __ __ __
 __ _1 __ __ __ __ 1_ __ __ 1_
 __ __ __ __ __ __ __ __ __ 1_
 __ __ __ __ __ 1_ __ __ _1 __
 __ __ __ __ __ __ __ __ 1_ __
 __ __ __ __ __ __ __ __ __ _1
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ _1 __ __ __ __ __ __ __

 __ __ __ __ __ __ __ __ __ __
 __ _1 __ __ __ __ 1_ __ 1_ __
 __ __ __ __ __ __ __ __ __ 1_
 __ __ __ __ __ 1_ __ __ __ _1
 __ __ __ __ __ __ __ __ 1_ __
 __ __ __ __ __ __ __ __ __ _1
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ _1 __ __ __ __ __ __ __
Turn: 1 | Aphids: 5 | Ladybugs: 4 | Births: 0 | Deaths: 0 | Food: 448

 _1 __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ 1_ __ 1_ __
 __ __ __ __ __ __ __ __ 1_ __
 __ __ __ __ __ 1_ __ __ _1 1_
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ _1 __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ _1 __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
Turn: 2 | Aphids: 5 | Ladybugs: 4 | Births: 0 | Deaths: 0 | Food: 454

 __ __ __ __ __ __ __ __ __ __
 __ _1 __ __ __ __ 1_ __ __ __
 __ __ __ __ 1_ __ __ 1_ 1_ _1
 __ __ __ __ __ __ __ __ __ 1_
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ _1 __
 __ __ __ __ __ __ __ __ __ __
 __ _1 __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
Turn: 3 | Aphids: 5 | Ladybugs: 4 | Births: 0 | Deaths: 0 | Food: 457

 __ __ _1 __ __ __ __ __ __ __
 __ __ __ __ __ 1_ __ __ __ __
 __ __ __ __ 1_ __ __ 1_ 1_ 1_
 __ __ __ __ __ __ __ __ _1 __
 __ __ __ __ __ __ __ __ __ _1
 __ __ __ __ __ __ __ __ __ __
 __ __ _1 __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
Turn: 4 | Aphids: 5 | Ladybugs: 4 | Births: 0 | Deaths: 0 | Food: 459

 __ __ __ __ __ __ __ __ __ __
 __ __ _1 1_ __ 1_ __ __ __ __
 __ __ __ __ __ __ __ __ 1_ 11
 __ __ __ __ __ __ __ __ 1_ __
 __ __ __ __ __ __ __ __ __ _1
 __ __ __ _1 __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
Turn: 5 | Aphids: 5 | Ladybugs: 4 | Births: 0 | Deaths: 0 | Food: 459

 __ __ __ __ __ __ __ __ __ __
 __ __ _1 1_ __ __ __ __ 1_ __
 __ __ __ __ 1_ __ __ __ 1_ __
 __ __ __ __ __ __ __ __ _1 __
 __ __ __ __ _1 __ __ __ __ 11
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
 __ __ __ __ __ __ __ __ __ __
Turn: 6 | Aphids: 5 | Ladybugs: 4 | Births: 0 | Deaths: 0 | Food: 470
"#,
    );
}
