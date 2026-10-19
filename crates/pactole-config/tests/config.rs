use std::path::{Path, PathBuf};

use pactole_config::{Config, ConfigError, Direction, Overrides, Source, ValidationLevel};

fn source<'a>(contents: &'a str) -> Source<'a> {
    Source {
        path: Path::new("<test>"),
        contents,
    }
}

fn resolve(contents: &str) -> Result<Config, ConfigError> {
    Config::resolve(&[source(contents)], &Overrides::default())
}

#[test]
fn empty_layers_yield_defaults() {
    let config = resolve("").unwrap();

    assert_eq!(config.validation.payee, ValidationLevel::Error);
    assert_eq!(config.format.posting_width, 50);
    assert_eq!(config.import, None);
}

#[test]
fn parses_every_section() {
    let config = resolve(
        "[validation]\npayee = \"warn\"\n\n\
         [format]\nposting_width = 72\n\n\
         [import]\nbank_account = \"Assets:Checking\"\ncurrency = \"EUR\"\n\
         default_counter_account = \"Expenses:To-Be-Classified\"\n",
    )
    .unwrap();

    assert_eq!(config.validation.payee, ValidationLevel::Warning);
    assert_eq!(config.format.posting_width, 72);

    let import = config.import.unwrap();
    assert_eq!(import.bank_account, "Assets:Checking");
    assert_eq!(import.currency.as_deref(), Some("EUR"));
    assert_eq!(import.default_counter_account, "Expenses:To-Be-Classified");
    assert!(import.accounts.is_empty());
    assert!(import.rules.is_empty());
}

#[test]
fn parses_import_accounts_and_rules() {
    let config = resolve(
        "[import]\n\
         bank_account = \"Assets:Checking\"\n\
         default_counter_account = \"Expenses:To-Be-Classified\"\n\
         [import.accounts]\n\
         \"000123\" = \"Assets:Checking\"\n\
         \"000456\" = \"Assets:Savings\"\n\
         [[import.rules]]\n\
         payee_contains = \"supermarket\"\n\
         counter_account = \"Expenses:Groceries\"\n\
         payee = \"Supermarket\"\n\
         [[import.rules]]\n\
         direction = \"debit\"\n\
         counter_account = \"Expenses:Other\"\n",
    )
    .unwrap();

    let import = config.import.unwrap();
    assert_eq!(import.accounts["000123"], "Assets:Checking");
    assert_eq!(import.accounts["000456"], "Assets:Savings");
    assert_eq!(import.rules.len(), 2);
    assert_eq!(
        import.rules[0].payee_contains.as_deref(),
        Some("supermarket")
    );
    assert_eq!(import.rules[0].counter_account, "Expenses:Groceries");
    assert_eq!(import.rules[0].payee.as_deref(), Some("Supermarket"));
    assert_eq!(import.rules[1].direction, Some(Direction::Debit));
}

#[test]
fn overrides_beat_project_which_beats_global() {
    let global = source("[validation]\npayee = \"error\"\n[format]\nposting_width = 60\n");
    let project = source("[validation]\npayee = \"warn\"\n");

    let config = Config::resolve(&[global, project], &Overrides::default()).unwrap();
    assert_eq!(config.validation.payee, ValidationLevel::Warning);
    assert_eq!(config.format.posting_width, 60);

    let config = Config::resolve(
        &[global, project],
        &Overrides {
            payee: Some(ValidationLevel::Error),
        },
    )
    .unwrap();
    assert_eq!(config.validation.payee, ValidationLevel::Error);
}

#[test]
fn import_tables_merge_per_key_and_rules_in_block() {
    let global = source(
        "[import]\n\
         bank_account = \"Assets:Checking\"\n\
         currency = \"EUR\"\n\
         default_counter_account = \"Expenses:To-Be-Classified\"\n\
         [import.accounts]\n\
         \"000123\" = \"Assets:Checking\"\n\
         \"000456\" = \"Assets:Savings\"\n\
         [[import.rules]]\n\
         payee_contains = \"global\"\n\
         counter_account = \"Expenses:Global\"\n",
    );
    let project = source(
        "[import]\n\
         default_counter_account = \"Expenses:Uncategorized\"\n\
         [import.accounts]\n\
         \"000456\" = \"Assets:Joint\"\n\
         \"000789\" = \"Assets:Card\"\n\
         [[import.rules]]\n\
         payee_contains = \"project\"\n\
         counter_account = \"Expenses:Project\"\n",
    );

    let config = Config::resolve(&[global, project], &Overrides::default()).unwrap();
    let import = config.import.unwrap();

    // Per-key merge: untouched keys survive, overridden keys win.
    assert_eq!(import.bank_account, "Assets:Checking");
    assert_eq!(import.currency.as_deref(), Some("EUR"));
    assert_eq!(import.default_counter_account, "Expenses:Uncategorized");
    assert_eq!(import.accounts.len(), 3);
    assert_eq!(import.accounts["000456"], "Assets:Joint");

    // Rules are replaced in block, never concatenated.
    assert_eq!(import.rules.len(), 1);
    assert_eq!(import.rules[0].payee_contains.as_deref(), Some("project"));
}

#[test]
fn rejects_the_future_off_level_with_a_clear_message() {
    let error = resolve("[validation]\npayee = \"off\"\n").unwrap_err();

    assert!(
        matches!(&error, ConfigError::Invalid { message, .. }
            if message.contains("`off` is not supported yet")),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_an_unknown_validation_level() {
    let error = resolve("[validation]\npayee = \"strict\"\n").unwrap_err();

    assert!(
        matches!(&error, ConfigError::Invalid { message, .. }
            if message.contains("`strict`") && message.contains("`warn` or `error`")),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_a_zero_posting_width() {
    let error = resolve("[format]\nposting_width = 0\n").unwrap_err();

    assert!(
        matches!(&error, ConfigError::Invalid { message, .. }
            if message.contains("posting_width")),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_an_incomplete_import_section() {
    let error = resolve("[import]\nbank_account = \"Assets:Checking\"\n").unwrap_err();

    assert!(
        matches!(&error, ConfigError::Invalid { message, .. }
            if message.contains("default_counter_account")),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_unknown_keys_in_every_section() {
    for contents in [
        "[validation]\npaye = \"warn\"\n",
        "[format]\nposting_widthh = 40\n",
        "[import]\nbankaccount = \"Assets:Checking\"\n",
        "[unknown]\nkey = 1\n",
    ] {
        assert!(
            matches!(resolve(contents), Err(ConfigError::Parse { .. })),
            "expected a parse error for {contents:?}"
        );
    }
}

#[test]
fn an_invalid_value_names_the_layer_that_defines_it() {
    let global = Source {
        path: Path::new("global.toml"),
        contents: "[validation]\npayee = \"strict\"\n",
    };
    let project = Source {
        path: Path::new("project.toml"),
        contents: "[validation]\npayee = \"warn\"\n",
    };

    let error = Config::resolve(&[global, project], &Overrides::default()).unwrap_err();

    assert!(
        matches!(&error, ConfigError::Invalid { path, .. } if path == Path::new("global.toml")),
        "unexpected error: {error}"
    );
}

#[test]
fn an_incomplete_import_section_names_the_layer_that_defines_it() {
    let global = Source {
        path: Path::new("global.toml"),
        contents: "[validation]\npayee = \"warn\"\n",
    };
    let project = Source {
        path: Path::new("project.toml"),
        contents: "[import]\nbank_account = \"Assets:Checking\"\n",
    };

    let error = Config::resolve(&[global, project], &Overrides::default()).unwrap_err();

    assert!(
        matches!(&error, ConfigError::Invalid { path, .. } if path == Path::new("project.toml")),
        "unexpected error: {error}"
    );
}

#[test]
fn load_uses_an_explicit_file_as_the_only_layer() {
    let dir = std::env::temp_dir().join(format!(
        "pactole-config-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let explicit = dir.join("custom.toml");
    std::fs::write(&explicit, "[validation]\npayee = \"warn\"\n").unwrap();

    // The project layer is ignored when an explicit file is given.
    let project = dir.join("pactole.toml");
    std::fs::write(&project, "[validation]\npayee = \"error\"\n").unwrap();

    let config = Config::load(Some(&explicit), Some(&project), &Overrides::default()).unwrap();
    assert_eq!(config.validation.payee, ValidationLevel::Warning);

    // An explicit file that does not exist is an error.
    let missing = dir.join("missing.toml");
    let error = Config::load(Some(&missing), None, &Overrides::default()).unwrap_err();
    assert!(
        matches!(error, ConfigError::Read { .. }),
        "unexpected: {error}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_skips_an_absent_project_layer() {
    let missing = PathBuf::from("/nonexistent/pactole/pactole.toml");

    // A nonexistent project file is skipped silently (no error). The
    // assertion is deliberately independent of whatever global layer the
    // environment may provide.
    let _config = Config::load(None, Some(&missing), &Overrides::default()).unwrap();
}
