//! Normalization tests.
//!
//! The names are synthetic, but cover the punctuation and accents SWTOR allows.

use super::normalize::{edit_distance, normalize, similarity};

#[test]
fn strips_log_decoration() {
    assert_eq!(normalize("@Test Alpha#101"), "TESTALPHA");
    assert_eq!(normalize("@Test Bravo#102"), "TESTBRAVO");
    // Undecorated input is left alone beyond the usual folding.
    assert_eq!(normalize("Test Alpha"), "TESTALPHA");
}

#[test]
fn folds_accents_seen_in_player_names() {
    assert_eq!(normalize("Tést Alpha"), "TESTALPHA");
    assert_eq!(normalize("Brav'à"), "BRAVA");
    assert_eq!(normalize("Chárlie Tank"), "CHARLIETANK");
    assert_eq!(normalize("Éxample"), "EXAMPLE");
    assert_eq!(normalize("Földgåme"), "FOLDGAME");
    assert_eq!(normalize("Bõt"), "BOT");
}

#[test]
fn drops_separators_the_ui_renders_inconsistently() {
    // SWTOR hair-thins or drops apostrophes at raid-frame sizes, so both sides
    // of the comparison have to lose them.
    assert_eq!(normalize("Test'Alpha"), normalize("TESTALPHA"));
    assert_eq!(normalize("B'ravo"), "BRAVO");
    assert_eq!(normalize("Char'lie"), "CHARLIE");
    assert_eq!(normalize("Test Long-name"), "TESTLONGNAME");
}

#[test]
fn ocr_output_and_log_name_converge() {
    // What the frame shows, uppercased by the game, versus what the log records.
    assert_eq!(normalize("T'EST ALPHA"), normalize("@T'est Alpha#123456"));
    assert_eq!(normalize("TEST BRAVO"), normalize("@Test Bravo#1"));
    assert_eq!(normalize("TEST CHARLIE"), normalize("@Test Charlie#1"));
}

#[test]
fn folding_is_idempotent() {
    for name in ["Tést Alpha", "@Test Long-name#1", "Test'Bravo", "Hotel"] {
        let once = normalize(name);
        assert_eq!(normalize(&once), once, "not idempotent for {name}");
    }
}

#[test]
fn distinct_players_do_not_collide() {
    // Every player must remain distinguishable after normalization.
    let group = [
        "Test Alpha",
        "Brávo Tank",
        "Test Charlie-Long",
        "Test Delta",
        "Test Echo Player",
        "Test Foxtrot",
        "T'est Golf",
        "Test Hotel Player",
    ];
    let normalized: Vec<String> = group.iter().map(|n| normalize(n)).collect();

    for (i, a) in normalized.iter().enumerate() {
        for (j, b) in normalized.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "{} and {} collide", group[i], group[j]);
            }
        }
    }
}

#[test]
fn empty_and_junk_input() {
    assert_eq!(normalize(""), "");
    assert_eq!(normalize("@#123"), "");
    assert_eq!(normalize("---'''"), "");
}

#[test]
fn edit_distance_basics() {
    assert_eq!(edit_distance("", ""), 0);
    assert_eq!(edit_distance("ABC", ""), 3);
    assert_eq!(edit_distance("", "ABC"), 3);
    assert_eq!(edit_distance("ABC", "ABC"), 0);
    // One substitution.
    assert_eq!(edit_distance("TESTALPHA", "TESTALPHB"), 1);
    // One deletion.
    assert_eq!(edit_distance("HOTEL", "HOTL"), 1);
}

#[test]
fn similarity_is_bounded() {
    assert_eq!(similarity("", ""), 1.0);
    assert_eq!(similarity("HOTEL", "HOTEL"), 1.0);
    assert!(similarity("HOTEL", "BRAVO") < 0.5);
    assert!(similarity("443604", "443025") < 0.7);
}
