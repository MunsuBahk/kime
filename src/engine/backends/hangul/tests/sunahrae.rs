use kime_engine_backend::{InputEngineBackend, Key};
use kime_engine_backend_hangul::{builtin_layouts, HangulConfig, HangulData, HangulEngine};

fn type_keys(layout: &str, keys: &[&str]) -> String {
    let mut config = HangulConfig::default();
    config.layout = layout.into();
    let data = HangulData::new(&config, builtin_layouts());
    let mut engine = HangulEngine::new(false, config.preedit_johab);
    let mut commit = String::new();
    for key in keys {
        assert!(engine.press_key(&data, key.parse::<Key>().unwrap(), &mut commit));
    }
    engine.clear_preedit(&mut commit);
    commit
}

#[test]
fn tense_initials_in_both_orders() {
    for (a, b, expected) in [
        ("K", "J", "까"),
        ("L", "K", "짜"),
        ("SemiColon", "L", "빠"),
        ("U", "I", "따"),
        ("N", "M", "싸"),
    ] {
        assert_eq!(type_keys("sebeolsik-sunahrae", &[a, b, "F"]), expected);
        assert_eq!(type_keys("sebeolsik-sunahrae", &[b, a, "F"]), expected);
    }
}

#[test]
fn extended_vowels() {
    for (a, b, expected) in [
        ("Slash", "T", "궈"),
        ("Slash", "C", "궤"),
        ("Slash", "G", "걔"),
        ("Slash", "E", "계"),
        ("Slash", "V", "긔"),
        ("9", "F", "과"),
        ("9", "R", "괘"),
    ] {
        assert_eq!(type_keys("sebeolsik-sunahrae", &["K", a, b]), expected);
    }
}

#[test]
fn bracket_final_is_separate_from_shift_symbols() {
    assert_eq!(
        type_keys("sebeolsik-sunahrae", &["K", "F", "OpenBracket", "X"]),
        "값"
    );
    assert_eq!(
        type_keys(
            "sebeolsik-sunahrae",
            &[
                "S-1", "S-2", "S-3", "S-4", "S-5", "S-6", "S-7", "S-8", "S-9", "S-0", "S-Minus",
                "S-Equal"
            ]
        ),
        "!@#$%^&*()_+"
    );
}

#[test]
fn literal_context_keys() {
    assert_eq!(
        type_keys("sebeolsik-sunahrae", &["Slash", "OpenBracket"]),
        "/["
    );
}

#[test]
fn default_layout_retains_composition() {
    assert_eq!(type_keys("dubeolsik", &["R", "K"]), "가");
}
