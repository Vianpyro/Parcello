//! Merge behavior tests: base + override mods, last-loaded-wins, rules.

use std::fs;
use std::path::Path;

use parcello_engine::{CardEffect, MarketEffect, RentModel, TileKind};
use parcello_mods::{ModError, resolve};

fn write_mod(root: &Path, id: &str, files: &[(&str, &str)]) {
    let dir = root.join(id);
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        format!("id = \"{id}\"\nversion = \"1.0.0\"\n"),
    )
    .unwrap();
    for (name, body) in files {
        fs::write(dir.join("data").join(name), body).unwrap();
    }
}

const BASE_PROPERTIES: &str = r#"
[[tile]]
id = "go"
name = "Go"
type = "go"

[[tile]]
id = "ave_a"
name = "Avenue A"
type = "property"
group = "brown"
price = 60
house_cost = 50
rents = [2, 10, 30, 90, 160, 250]

[[tile]]
id = "chance_1"
name = "Chance"
type = "chance"

[[tile]]
id = "jail"
name = "Jail"
type = "jail"
"#;

const BASE_CARDS: &str = r#"
[[chance]]
id = "ch_bonus"
text = "Bank pays you 50"
[chance.effect]
type = "money"
amount = 50
"#;

#[test]
fn base_mod_resolves_to_valid_content() {
    let tmp = tempfile::tempdir().unwrap();
    write_mod(
        tmp.path(),
        "base",
        &[
            ("properties.toml", BASE_PROPERTIES),
            ("cards.toml", BASE_CARDS),
        ],
    );

    let resolved = resolve(tmp.path(), &["base".into()]).unwrap();
    assert_eq!(resolved.content.board.len(), 4);
    assert_eq!(resolved.content.chance.len(), 1);
    assert_eq!(resolved.mods.len(), 1);
    assert_eq!(resolved.mods[0].id, "base");
}

#[test]
fn override_mod_replaces_tile_in_place_and_layers_rules() {
    let tmp = tempfile::tempdir().unwrap();
    write_mod(
        tmp.path(),
        "base",
        &[
            ("properties.toml", BASE_PROPERTIES),
            ("cards.toml", BASE_CARDS),
            ("rules.toml", "[rules]\nstarting_balance = 1500\n"),
        ],
    );
    write_mod(
        tmp.path(),
        "richer",
        &[
            (
                "properties.toml",
                r#"
[[tile]]
id = "ave_a"
name = "Grand Avenue A"
type = "property"
group = "brown"
price = 100
house_cost = 60
rents = [5, 20, 60, 180, 320, 500]
"#,
            ),
            (
                "rules.toml",
                "[rules]\nstarting_balance = 3000\nfuture_unknown_key = 1\n",
            ),
        ],
    );

    let resolved = resolve(tmp.path(), &["base".into(), "richer".into()]).unwrap();
    let content = &resolved.content;

    // Replaced in place: same board position, new definition.
    assert_eq!(content.board[1].id, "ave_a");
    assert_eq!(content.board[1].name, "Grand Avenue A");
    match &content.board[1].kind {
        TileKind::Property(p) => assert_eq!(p.price, 100),
        other => panic!("expected property, got {other:?}"),
    }
    assert_eq!(
        content.board.len(),
        4,
        "override must not append a duplicate"
    );

    // Last-loaded-wins on scalar rules; unknown keys ignored.
    assert_eq!(content.rules.starting_balance, 3000);
    assert_eq!(resolved.mods.len(), 2);
}

#[test]
fn card_override_replaces_by_id() {
    let tmp = tempfile::tempdir().unwrap();
    write_mod(
        tmp.path(),
        "base",
        &[
            ("properties.toml", BASE_PROPERTIES),
            ("cards.toml", BASE_CARDS),
        ],
    );
    write_mod(
        tmp.path(),
        "meaner",
        &[(
            "cards.toml",
            r#"
[[chance]]
id = "ch_bonus"
text = "Pay the bank 50"
[chance.effect]
type = "money"
amount = -50
"#,
        )],
    );

    let resolved = resolve(tmp.path(), &["base".into(), "meaner".into()]).unwrap();
    assert_eq!(resolved.content.chance.len(), 1);
    assert_eq!(
        resolved.content.chance[0].effect,
        CardEffect::Money { amount: -50 }
    );
}

#[test]
fn scaled_rent_tiles_parse_without_house_cost() {
    let tmp = tempfile::tempdir().unwrap();
    write_mod(
        tmp.path(),
        "stations",
        &[(
            "properties.toml",
            r#"
[[tile]]
id = "go"
name = "Go"
type = "go"

[[tile]]
id = "jail"
name = "Jail"
type = "jail"

[[tile]]
id = "st_a"
name = "Station A"
type = "property"
group = "transit"
price = 200
rent_model = "group_scaled"
rents = [25, 50, 100, 200, 0, 0]
"#,
        )],
    );

    let resolved = resolve(tmp.path(), &["stations".into()]).unwrap();
    match &resolved.content.board[2].kind {
        TileKind::Property(p) => {
            assert_eq!(p.rent_model, RentModel::GroupScaled);
            assert_eq!(p.house_cost, 0);
        }
        other => panic!("expected property, got {other:?}"),
    }
}

#[test]
fn spotlight_tile_type_parses() {
    let tmp = tempfile::tempdir().unwrap();
    write_mod(
        tmp.path(),
        "expo",
        &[(
            "properties.toml",
            r#"
[[tile]]
id = "go"
name = "Go"
type = "go"

[[tile]]
id = "jail"
name = "Jail"
type = "jail"

[[tile]]
id = "exposition"
name = "The Exposition"
type = "spotlight"
"#,
        )],
    );

    let resolved = resolve(tmp.path(), &["expo".into()]).unwrap();
    assert!(matches!(
        &resolved.content.board[2].kind,
        TileKind::Spotlight
    ));
}

#[test]
fn missing_mod_directory_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let err = resolve(tmp.path(), &["ghost".into()]).unwrap_err();
    assert!(matches!(err, ModError::NotFound(_)));
}

#[test]
fn incompatible_min_game_version_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("future");
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        "id = \"future\"\nversion = \"1.0.0\"\nmin_game_version = \"99.0.0\"\n",
    )
    .unwrap();

    let err = resolve(tmp.path(), &["future".into()]).unwrap_err();
    assert!(matches!(err, ModError::IncompatibleVersion { .. }));
}

#[test]
fn invalid_merged_content_is_rejected() {
    // A board without a Go tile must fail content validation.
    let tmp = tempfile::tempdir().unwrap();
    write_mod(
        tmp.path(),
        "broken",
        &[(
            "properties.toml",
            r#"
[[tile]]
id = "jail"
name = "Jail"
type = "jail"
"#,
        )],
    );

    let err = resolve(tmp.path(), &["broken".into()]).unwrap_err();
    assert!(matches!(err, ModError::Content(_)));
}

#[test]
fn event_pool_override_replaces_by_id() {
    let tmp = tempfile::tempdir().unwrap();
    write_mod(
        tmp.path(),
        "base",
        &[
            ("properties.toml", BASE_PROPERTIES),
            ("cards.toml", BASE_CARDS),
            (
                "events.toml",
                r#"
[forecast]
gap_turns = 6

[[event]]
id = "crash"
name = "Market Crash"
effect = "rent_multiplier"
magnitude_pct = -50
duration_turns = 4
"#,
            ),
        ],
    );
    write_mod(
        tmp.path(),
        "harsher",
        &[(
            "events.toml",
            r#"
[[event]]
id = "crash"
name = "Bigger Market Crash"
effect = "rent_multiplier"
magnitude_pct = -80
duration_turns = 6
"#,
        )],
    );

    let resolved = resolve(tmp.path(), &["base".into(), "harsher".into()]).unwrap();
    let events = &resolved.content.market_events;
    assert_eq!(events.len(), 1, "override must not append a duplicate");
    assert_eq!(events[0].name, "Bigger Market Crash");
    assert_eq!(events[0].magnitude_pct, -80);
    assert_eq!(events[0].effect, MarketEffect::RentMultiplier);
}

#[test]
fn forecast_gap_turns_last_loaded_wins() {
    let tmp = tempfile::tempdir().unwrap();
    write_mod(
        tmp.path(),
        "base",
        &[
            ("properties.toml", BASE_PROPERTIES),
            ("cards.toml", BASE_CARDS),
            ("events.toml", "[forecast]\ngap_turns = 6\n"),
        ],
    );
    write_mod(
        tmp.path(),
        "faster",
        &[("events.toml", "[forecast]\ngap_turns = 10\n")],
    );

    let resolved = resolve(tmp.path(), &["base".into(), "faster".into()]).unwrap();
    assert_eq!(resolved.content.forecast_gap_turns, 10);
}

// --- Error paths: a community mod that fails must fail loudly and precisely -

#[test]
fn unknown_tile_type_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    write_mod(
        tmp.path(),
        "bad",
        &[(
            "properties.toml",
            "[[tile]]\nid = \"w\"\nname = \"Warp\"\ntype = \"warp\"\n",
        )],
    );
    let err = resolve(tmp.path(), &["bad".into()]).unwrap_err();
    assert!(matches!(err, ModError::InvalidTile { ref tile, .. } if tile == "w"));
}

#[test]
fn property_missing_required_fields_is_rejected_per_field() {
    // One resolve per missing field, so each error branch is exercised.
    let cases = [
        // (omitted field, body)
        (
            "house_cost",
            "group = \"brown\"\nprice = 60\nrents = [1, 2, 3, 4, 5, 6]\n",
        ),
        (
            "group",
            "price = 60\nhouse_cost = 50\nrents = [1, 2, 3, 4, 5, 6]\n",
        ),
        (
            "price",
            "group = \"brown\"\nhouse_cost = 50\nrents = [1, 2, 3, 4, 5, 6]\n",
        ),
        ("rents", "group = \"brown\"\nprice = 60\nhouse_cost = 50\n"),
    ];
    for (missing, body) in cases {
        let tmp = tempfile::tempdir().unwrap();
        write_mod(
            tmp.path(),
            "bad",
            &[(
                "properties.toml",
                &format!("[[tile]]\nid = \"p\"\nname = \"P\"\ntype = \"property\"\n{body}"),
            )],
        );
        let err = resolve(tmp.path(), &["bad".into()]).unwrap_err();
        assert!(
            matches!(err, ModError::InvalidTile { .. }),
            "missing `{missing}` must reject as InvalidTile, got: {err}"
        );
    }
}

#[test]
fn group_scaled_property_needs_no_house_cost() {
    let tmp = tempfile::tempdir().unwrap();
    let station = r#"
[[tile]]
id = "go"
name = "Go"
type = "go"

[[tile]]
id = "st"
name = "Station"
type = "property"
group = "transit"
price = 200
rents = [25, 50, 100, 200, 0, 0]
rent_model = "group_scaled"

[[tile]]
id = "jail"
name = "Jail"
type = "jail"
"#;
    write_mod(tmp.path(), "st", &[("properties.toml", station)]);
    let resolved = resolve(tmp.path(), &["st".into()]).unwrap();
    let TileKind::Property(p) = &resolved.content.board[1].kind else {
        panic!("expected a property");
    };
    assert_eq!(p.house_cost, 0);
    assert_eq!(p.rent_model, RentModel::GroupScaled);
}

#[test]
fn tax_tiles_require_their_amount_fields() {
    for (kind, body) in [
        ("tax", ""),
        ("net_worth_tax", ""),
        ("net_worth_tax", "min_pct = 5\n"), // max_pct still missing
    ] {
        let tmp = tempfile::tempdir().unwrap();
        write_mod(
            tmp.path(),
            "bad",
            &[(
                "properties.toml",
                &format!("[[tile]]\nid = \"t\"\nname = \"T\"\ntype = \"{kind}\"\n{body}"),
            )],
        );
        let err = resolve(tmp.path(), &["bad".into()]).unwrap_err();
        assert!(matches!(err, ModError::InvalidTile { .. }), "{kind}: {err}");
    }
}

#[test]
fn missing_mod_directory_is_an_io_error() {
    let tmp = tempfile::tempdir().unwrap();
    let err = resolve(tmp.path(), &["ghost".into()]).unwrap_err();
    assert!(matches!(err, ModError::NotFound(_)), "got: {err}");
}

#[test]
fn broken_manifest_is_a_parse_error_with_the_path() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("bad");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("manifest.toml"), "id = [not toml").unwrap();
    let err = resolve(tmp.path(), &["bad".into()]).unwrap_err();
    assert!(
        matches!(err, ModError::Parse { ref path, .. } if path.contains("manifest.toml")),
        "got: {err}"
    );
}

#[test]
fn mod_requiring_a_newer_game_is_rejected_but_garbage_versions_only_warn() {
    // Too new: hard error.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("future");
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        "id = \"future\"\nversion = \"1.0.0\"\nmin_game_version = \"999.0.0\"\n",
    )
    .unwrap();
    let err = resolve(tmp.path(), &["future".into()]).unwrap_err();
    assert!(matches!(err, ModError::IncompatibleVersion { .. }), "{err}");

    // Unparsable requirement: ignored with a WARN, mod still loads.
    let tmp = tempfile::tempdir().unwrap();
    write_mod(
        tmp.path(),
        "base",
        &[
            ("properties.toml", BASE_PROPERTIES),
            ("cards.toml", BASE_CARDS),
        ],
    );
    let dir = tmp.path().join("odd");
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        "id = \"odd\"\nversion = \"1.0.0\"\nmin_game_version = \"tomorrow\"\n",
    )
    .unwrap();
    resolve(tmp.path(), &["base".into(), "odd".into()])
        .expect("unparsable min_game_version must not block loading");
}

#[test]
fn a_mod_with_no_data_files_layers_as_a_no_op() {
    // Exercises every read_data missing-file branch in one resolve.
    let tmp = tempfile::tempdir().unwrap();
    write_mod(
        tmp.path(),
        "base",
        &[
            ("properties.toml", BASE_PROPERTIES),
            ("cards.toml", BASE_CARDS),
        ],
    );
    write_mod(tmp.path(), "empty", &[]);
    let resolved = resolve(tmp.path(), &["base".into(), "empty".into()]).unwrap();
    assert_eq!(resolved.content.board.len(), 4);
    assert_eq!(resolved.mods.len(), 2, "the empty mod still registers");
}
