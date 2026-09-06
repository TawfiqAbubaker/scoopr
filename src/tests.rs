use super::{
    encode_base64, extract_candidates, pane_ids_in_scope, ranked_matches, tab_ids_in_workspace,
    Candidate, Filter, Scope, KIND_LINE, KIND_WORD,
};
use norm::fzf::{FzfParser, FzfV2};
use serde_json::json;

fn word_candidates(values: &[&str]) -> Vec<Candidate> {
    values
        .iter()
        .map(|value| Candidate {
            text: (*value).to_string(),
            kinds: KIND_WORD,
        })
        .collect()
}

fn values_for_filter(candidates: &[Candidate], filter: Filter) -> Vec<&str> {
    candidates
        .iter()
        .filter(|candidate| candidate.appears_in(filter))
        .map(|candidate| candidate.text.as_str())
        .collect()
}

#[test]
fn encodes_osc52_payloads() {
    assert_eq!(encode_base64(b""), "");
    assert_eq!(encode_base64(b"f"), "Zg==");
    assert_eq!(encode_base64(b"fo"), "Zm8=");
    assert_eq!(encode_base64(b"foo"), "Zm9v");
    assert_eq!(encode_base64("scoop 🥄".as_bytes()), "c2Nvb3Ag8J+lhA==");
}

#[test]
fn ranks_a_contiguous_word_as_the_selected_last_result() {
    let candidates = word_candidates(&[
        "w_o_r_d spread across a weak match",
        "the exact word is here",
        "another wandering odd result, deliberately",
    ]);
    let mut matcher = FzfV2::new();
    let mut parser = FzfParser::new();

    let ranked = ranked_matches(&candidates, Filter::All, "word", &mut matcher, &mut parser);

    assert_eq!(
        ranked.last().map(|(candidate, _)| candidate.as_str()),
        Some("the exact word is here")
    );
}

#[test]
fn ranks_a_literal_multi_term_prefix_above_reordered_terms() {
    let candidates = word_candidates(&["pane ...... 1", "1 package"]);
    let mut matcher = FzfV2::new();
    let mut parser = FzfParser::new();

    let ranked = ranked_matches(&candidates, Filter::All, "1 pa", &mut matcher, &mut parser);

    assert_eq!(
        ranked.last().map(|(candidate, _)| candidate.as_str()),
        Some("1 package")
    );
}

#[test]
fn prefers_the_newer_matching_terminal_line() {
    let candidates = vec![
        Candidate {
            text: "git add one".into(),
            kinds: KIND_LINE,
        },
        Candidate {
            text: "git add two".into(),
            kinds: KIND_LINE,
        },
    ];
    let mut matcher = FzfV2::new();
    let mut parser = FzfParser::new();

    let ranked = ranked_matches(
        &candidates,
        Filter::All,
        "git add",
        &mut matcher,
        &mut parser,
    );

    assert_eq!(
        ranked.last().map(|(candidate, _)| candidate.as_str()),
        Some("git add two")
    );
}

#[test]
fn prefers_a_strong_match_over_a_newer_weak_match() {
    let candidates = vec![
        Candidate {
            text: "git push".into(),
            kinds: KIND_LINE,
        },
        Candidate {
            text: "scoopr git:(main)".into(),
            kinds: KIND_LINE,
        },
    ];
    let mut matcher = FzfV2::new();
    let mut parser = FzfParser::new();

    let ranked = ranked_matches(&candidates, Filter::All, "git p", &mut matcher, &mut parser);

    assert_eq!(
        ranked.last().map(|(candidate, _)| candidate.as_str()),
        Some("git push")
    );
}

#[test]
fn handles_a_single_h_search_query() {
    let candidates = extract_candidates(
        "https://example.com\ncommit deadbeef\nherdr plugin action invoke scoopr.open",
    );
    let mut matcher = FzfV2::new();
    let mut parser = FzfParser::new();

    let ranked = ranked_matches(&candidates, Filter::All, "h", &mut matcher, &mut parser);

    assert!(!ranked.is_empty());
}

#[test]
fn handles_single_letter_search_queries() {
    let candidates = extract_candidates(
        "alpha bravo charlie delta echo foxtrot golf hotel\n\
             herdr plugin action invoke scoopr.open\n\
             https://example.com/path",
    );
    let mut matcher = FzfV2::new();
    let mut parser = FzfParser::new();

    for letter in 'a'..='z' {
        let query = letter.to_string();
        let _ = ranked_matches(&candidates, Filter::All, &query, &mut matcher, &mut parser);
    }
}

#[test]
fn handles_repeated_queries_and_clearing() {
    let candidates = extract_candidates(
        "alpha bravo charlie delta echo foxtrot golf hotel\n\
             herdr plugin action invoke scoopr.open\n\
             https://example.com/path",
    );
    let mut matcher = FzfV2::new();
    let mut parser = FzfParser::new();

    for query in ["a", "aa", "a", "", "a", "a", "", "a"] {
        let _ = ranked_matches(&candidates, Filter::All, query, &mut matcher, &mut parser);
    }
}

#[test]
fn treats_straight_and_typographic_apostrophes_as_equivalent() {
    let candidates = word_candidates(&["Earlier you wrote I’m here"]);
    let mut matcher = FzfV2::new();
    let mut parser = FzfParser::new();

    let ranked = ranked_matches(&candidates, Filter::All, "I'm", &mut matcher, &mut parser);

    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].0, "Earlier you wrote I’m here");
    assert_eq!(ranked[0].1, [18, 19, 20]);
}

#[test]
fn cycles_through_all_scopes() {
    assert_eq!(Scope::Space.next(false), Scope::Tab);
    assert_eq!(Scope::Tab.next(false), Scope::Server);
    assert_eq!(Scope::Server.next(false), Scope::Space);
    assert_eq!(Scope::Space.next(true), Scope::Server);
    assert_eq!(Scope::Server.next(true), Scope::Space);
    assert_eq!(Scope::Tab.index(), 0);
    assert_eq!(Scope::Space.index(), 1);
    assert_eq!(Scope::Server.index(), 2);
}

#[test]
fn selects_filters_by_shortcut() {
    assert_eq!(Filter::from_key('a'), Some(Filter::All));
    assert_eq!(Filter::from_key('W'), Some(Filter::Word));
    assert_eq!(Filter::from_key('l'), Some(Filter::Line));
    assert_eq!(Filter::from_key('p'), Some(Filter::Path));
    assert_eq!(Filter::from_key('u'), Some(Filter::Url));
    assert_eq!(Filter::from_key('h'), Some(Filter::Hash));
    assert_eq!(Filter::from_key('q'), Some(Filter::Quote));
    assert_eq!(Filter::from_key('x'), None);
}

#[test]
fn tags_structured_candidates_for_filtering() {
    let candidates = extract_candidates(
        "open /tmp/report.txt at https://example.com/a\n\
             commit deadbeef says \"hello world\"",
    );

    assert!(values_for_filter(&candidates, Filter::Path).contains(&"/tmp/report.txt"));
    assert!(values_for_filter(&candidates, Filter::Url).contains(&"https://example.com/a"));
    assert!(values_for_filter(&candidates, Filter::Hash).contains(&"deadbeef"));
    assert!(values_for_filter(&candidates, Filter::Quote).contains(&"hello world"));
}

#[test]
fn collects_panes_for_each_scope() {
    let response = json!({
        "id": "cli:pane",
        "result": {
            "panes": [
                { "pane_id": "w1:p1", "tab_id": "w1:t1", "workspace_id": "w1" },
                { "pane_id": "w1:p2", "tab_id": "w1:t1", "workspace_id": "w1" },
                { "pane_id": "w1:p3", "tab_id": "w1:t2", "workspace_id": "w1" },
                { "pane_id": "w2:p1", "tab_id": "w2:t1", "workspace_id": "w2" }
            ]
        }
    });

    assert_eq!(
        pane_ids_in_scope(&response, Scope::Tab, "w1:t1", "w1"),
        ["w1:p1".to_string(), "w1:p2".to_string()]
    );
    assert_eq!(
        pane_ids_in_scope(&response, Scope::Space, "w1:t1", "w1"),
        [
            "w1:p1".to_string(),
            "w1:p2".to_string(),
            "w1:p3".to_string()
        ]
    );
    assert_eq!(
        pane_ids_in_scope(&response, Scope::Server, "w1:t1", "w1"),
        [
            "w1:p1".to_string(),
            "w1:p2".to_string(),
            "w1:p3".to_string(),
            "w2:p1".to_string()
        ]
    );
}

#[test]
fn counts_distinct_tabs_in_a_workspace() {
    let response = json!({
        "panes": [
            { "pane_id": "w1:p1", "tab_id": "w1:t1", "workspace_id": "w1" },
            { "pane_id": "w1:p2", "tab_id": "w1:t1", "workspace_id": "w1" },
            { "pane_id": "w1:p3", "tab_id": "w1:t2", "workspace_id": "w1" },
            { "pane_id": "w2:p1", "tab_id": "w2:t1", "workspace_id": "w2" }
        ]
    });

    assert_eq!(tab_ids_in_workspace(&response, "w1").len(), 2);
    assert_eq!(tab_ids_in_workspace(&response, "w2").len(), 1);
}

#[test]
fn detects_only_active_matching_keybindings() {
    assert!(!super::active_keybinding(
        "# key = \"prefix+shift+c\"\n",
        super::DEFAULT_KEYBINDING
    ));
    assert!(super::active_keybinding(
        "key = \"prefix+shift+c\"\n",
        super::DEFAULT_KEYBINDING
    ));
}

#[test]
fn removes_only_scoopr_managed_block() {
    let config = "[keys]\n\n# >>> scoopr keybinding >>>\n[[keys.command]]\nkey = \"prefix+shift+c\"\ntype = \"plugin_action\"\ncommand = \"scoopr.open\"\ndescription = \"Scoop text from current tab\"\n# <<< scoopr keybinding <<<\n";

    assert_eq!(
        super::remove_setup_block(config),
        Some("[keys]\n\n".to_string())
    );
}
