//! The search grammar and its evaluator over the fixture registry.

mod common;

use slotted_browser::index::Bitset;
use slotted_browser::{
    Field, PrefixMode, SearchConfig, SortStage, Token, evaluate, sort, tokenize,
};

fn config() -> SearchConfig {
    SearchConfig::default()
}

fn texts(tokens: &[Token]) -> Vec<&str> {
    tokens.iter().map(|t| t.text.as_str()).collect()
}

#[test]
fn an_empty_query_has_no_tokens() {
    assert!(tokenize("", &config()).is_empty());
    assert!(tokenize("   \t ", &config()).is_empty());
}

#[test]
fn whitespace_splits_terms_into_separate_and_groups() {
    let tokens = tokenize("iron ingot", &config());
    assert_eq!(texts(&tokens), ["iron", "ingot"]);
    assert_eq!(tokens[0].or_group, 0);
    assert_eq!(tokens[1].or_group, 1);
}

#[test]
fn pipe_binds_tighter_than_whitespace() {
    let tokens = tokenize("a b|c", &config());
    assert_eq!(texts(&tokens), ["a", "b", "c"]);
    let groups: Vec<u32> = tokens.iter().map(|t| t.or_group).collect();
    assert_eq!(groups, [0, 1, 1], "`a b|c` is `a AND (b OR c)`");
}

#[test]
fn a_pipe_chain_stays_in_one_group() {
    let tokens = tokenize("a|b|c d", &config());
    let groups: Vec<u32> = tokens.iter().map(|t| t.or_group).collect();
    assert_eq!(groups, [0, 0, 0, 1]);
}

#[test]
fn quotes_keep_spaces_and_a_backslash_escapes() {
    let tokens = tokenize(r#""iron ingot" plain"#, &config());
    assert_eq!(texts(&tokens), ["iron ingot", "plain"]);

    let tokens = tokenize(r#""say \"hi\"""#, &config());
    assert_eq!(texts(&tokens), [r#"say "hi""#]);

    let tokens = tokenize(r"a\ b", &config());
    assert_eq!(texts(&tokens), ["a b"], "an escaped space does not split");
}

#[test]
fn a_quoted_or_escaped_leading_character_is_text() {
    let tokens = tokenize(r#""-iron" \@demo "@mod""#, &config());
    assert_eq!(texts(&tokens), ["-iron", "@demo", "@mod"]);
    assert!(tokens.iter().all(|t| !t.negate));
    assert!(tokens.iter().all(|t| t.field == Field::Name));
}

#[test]
fn a_leading_dash_negates_and_a_trailing_dash_is_dropped() {
    let tokens = tokenize("iron -", &config());
    assert_eq!(texts(&tokens), ["iron"], "a lone `-` produces no token");
    let tokens = tokenize("-iron", &config());
    assert!(tokens[0].negate);
    let tokens = tokenize("a-b", &config());
    assert!(!tokens[0].negate, "a `-` inside a term is text");
    assert_eq!(tokens[0].text, "a-b");
}

#[test]
fn negation_composes_with_a_field_prefix() {
    let tokens = tokenize("-@demo", &config());
    assert!(tokens[0].negate);
    assert_eq!(tokens[0].field, Field::Mod);
    assert_eq!(tokens[0].text, "demo");
}

#[test]
fn prefix_modes_decide_whether_a_prefix_is_read() {
    let mut cfg = config();
    // `&` is Disabled by default: the prefix stays literal text.
    let tokens = tokenize("&minecraft:coal", &cfg);
    assert_eq!(tokens[0].field, Field::Name);
    assert_eq!(tokens[0].text, "&minecraft:coal");

    cfg.modes.insert(Field::Id, PrefixMode::RequirePrefix);
    let tokens = tokenize("&minecraft:coal", &cfg);
    assert_eq!(tokens[0].field, Field::Id);
    assert_eq!(tokens[0].text, "minecraft:coal");

    // RequirePrefix and Enabled both read the prefix; they differ only in
    // whether an unprefixed term also searches the field.
    let cfg = config();
    assert_eq!(cfg.mode(Field::Tooltip), PrefixMode::Enabled);
    assert_eq!(tokenize("$stone", &cfg)[0].field, Field::Tooltip);
    assert_eq!(cfg.mode(Field::Tag), PrefixMode::RequirePrefix);
    assert_eq!(tokenize("#c:tools", &cfg)[0].field, Field::Tag);
}

#[test]
fn a_prefix_applies_to_a_quoted_term() {
    let tokens = tokenize(r#"@"my mod""#, &config());
    assert_eq!(tokens[0].field, Field::Mod);
    assert_eq!(tokens[0].text, "my mod");
}

// ---------------------------------------------------------------------------
// Evaluation over the fixture registry.
// ---------------------------------------------------------------------------

fn matching(query: &str) -> Vec<String> {
    let index = common::index();
    let cfg = config();
    let tokens = tokenize(query, &cfg);
    let set = evaluate(&index, &tokens, &cfg, &Bitset::new(index.len()));
    sort(&index, &set, &cfg.sort)
        .into_iter()
        .filter_map(|id| index.get(id).map(|e| e.display.clone()))
        .collect()
}

#[test]
fn an_empty_query_matches_every_entry() {
    let index = common::index();
    let cfg = config();
    let set = evaluate(&index, &[], &cfg, &Bitset::new(index.len()));
    assert_eq!(set.count(), index.len());
    assert!(index.len() >= 11, "eleven items plus the tag entries");
}

#[test]
fn a_plain_term_matches_display_names() {
    // `$` is Enabled by default, so a plain word also searches tooltip text.
    // An item's tooltip is its name, rarity and component keys; a tag's is the
    // tag plus its member ids, which is why the two tags match "iron" as well.
    let names = matching("iron");
    assert_eq!(
        names,
        ["Iron Ingot", "Iron Pickaxe", "#c:ingots", "#c:tools"]
    );
}

#[test]
fn terms_intersect_and_or_groups_unite() {
    assert_eq!(matching("iron pickaxe"), ["Iron Pickaxe", "#c:tools"]);
    let either = matching("pickaxe|sword");
    assert_eq!(either, ["Diamond Sword", "Iron Pickaxe", "#c:tools"]);
    assert_eq!(matching("iron|diamond ingot"), ["Iron Ingot", "#c:ingots"]);
}

#[test]
fn a_negated_term_subtracts() {
    let all_iron = matching("iron");
    let without = matching("iron -pickaxe -#c:tools");
    assert_eq!(without, ["Iron Ingot", "#c:ingots"]);
    assert!(all_iron.len() > without.len());
}

#[test]
fn the_mod_field_matches_the_namespace() {
    let minecraft = matching("@minecraft");
    assert!(minecraft.contains(&"Iron Ingot".to_owned()));
    assert!(!minecraft.contains(&"Debug Stick".to_owned()));
    assert_eq!(matching("@slotted"), ["Debug Stick", "#slotted:dev"]);
}

#[test]
fn the_tag_field_matches_members_and_the_tag_entry() {
    let tools = matching("#c:tools");
    assert!(tools.contains(&"Iron Pickaxe".to_owned()));
    assert!(tools.contains(&"Diamond Sword".to_owned()));
    assert!(tools.contains(&"Debug Stick".to_owned()));
    assert!(tools.contains(&"#c:tools".to_owned()), "the tag entry too");
    assert!(!tools.contains(&"Dirt".to_owned()));
}

#[test]
fn a_tag_matches_by_path_as_well_as_by_full_id() {
    assert_eq!(matching("#c:tools"), matching("#tools"));
}

#[test]
fn the_category_field_matches_entries_a_recipe_produces() {
    let crafted = matching("%demo:crafting");
    assert_eq!(crafted, ["Diamond Sword", "Iron Pickaxe"]);
    assert_eq!(matching("%demo:smelting"), ["Coal"]);
}

#[test]
fn an_enabled_prefix_field_also_takes_unprefixed_terms() {
    // Tooltip text is Enabled by default, and it carries the rarity.
    let legendary = matching("legendary");
    assert_eq!(legendary, ["Debug Stick"]);
    assert_eq!(matching("$legendary"), legendary);
}

#[test]
fn hidden_entries_are_subtracted_and_restored() {
    let index = common::index();
    let cfg = config();
    let tokens = tokenize("iron", &cfg);
    let mut hidden = Bitset::new(index.len());
    let visible = evaluate(&index, &tokens, &cfg, &hidden);
    let count = visible.count();
    assert!(count > 1);

    let first = visible.iter().next().expect("at least one match");
    hidden.insert(first);
    let narrowed = evaluate(&index, &tokens, &cfg, &hidden);
    assert_eq!(narrowed.count(), count - 1);
    assert!(!narrowed.contains(first));

    hidden.remove(first);
    assert_eq!(evaluate(&index, &tokens, &cfg, &hidden).count(), count);
}

#[test]
fn sort_stages_are_stable_and_total() {
    let index = common::index();
    let all = Bitset::full(index.len());
    let by_name = sort(&index, &all, &[SortStage::Alphabetical]);
    assert_eq!(by_name, sort(&index, &all, &[SortStage::Alphabetical]));

    let displays: Vec<String> = by_name
        .iter()
        .filter_map(|id| index.get(*id).map(|e| e.display.to_lowercase()))
        .collect();
    let mut expected = displays.clone();
    expected.sort();
    assert_eq!(displays, expected);

    // Items are registered before tags, so the type stage puts every item first.
    let by_type = sort(&index, &all, &[SortStage::IngredientType]);
    let first = index.get(by_type[0]).expect("non-empty");
    let last = index
        .get(*by_type.last().expect("non-empty"))
        .expect("entry");
    assert_eq!(first.ingredient.ty, slotted_browser::types::item());
    assert_eq!(last.ingredient.ty, slotted_browser::types::tag());

    // Every stage returns the same set, only ordered differently.
    let mut a = sort(&index, &all, &[SortStage::ModName]);
    let mut b = sort(&index, &all, &[SortStage::Rarity]);
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b);
    assert_eq!(a.len(), index.len());
}
