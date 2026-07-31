//! Regenerates the EXD-derivable size constants in `resources/data/constants.yml`.
//!
//! This tool owns EXACTLY the 18 keys in [`DESCRIPTORS`] and nothing else. It reads the game's
//! Excel sheets, derives each size from the live data, and rewrites *only* those 18 lines of
//! `constants.yml` in place -- every other line (key order, comments, list blocks) is preserved
//! byte-for-byte. The three "stale-looking" client-fixed values
//! (`GATHERED_GATHERING_ITEMS`, `UNLOCKED_FISHING_SPOTS`, `CLASSJOB_ARRAY_SIZE`) are NOT owned here.
//!
//! # Modes
//!
//! * (no flag)  -- dry-run: compute, print a diff, exit 0 whether or not there is drift.
//! * `--check`  -- compute, print a diff, exit 1 if anything drifted, write nothing.
//! * `--write`  -- compute and apply the surgical rewrite to `constants.yml`.
//!
//! # Running the tests
//!
//! The arithmetic and rewriter tests are data-independent and always run. The golden test that
//! pins all 18 values against a real install is `#[ignore]`d (CI has no game data):
//!
//! ```text
//! cargo test -p kawari-constants-gen -- --include-ignored
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use physis::{
    Language,
    resource::{ResourceResolver, SqPackResource},
};

// -------------------------------------------------------------------------------------------------
// Rules and units
// -------------------------------------------------------------------------------------------------

/// How the raw count `N` derived by a [`Rule`] is turned into the stored constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    /// A bitmask: one bit per element, rounded up to whole bytes -- `ceil(N / 8)`.
    Div8,
    /// A plain element count stored as-is -- `N`.
    Raw,
}

impl Unit {
    /// The stored constant for a raw count `n`.
    fn stored(self, n: u32) -> usize {
        match self {
            Unit::Div8 => (n as usize).div_ceil(8),
            Unit::Raw => n as usize,
        }
    }
}

/// The derivation rule for one constant. The variant documents *what* is counted; the actual typed
/// sheet read lives in [`raw_count`], keyed by the descriptor's `sheet`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    /// `N = max_row_id + 1` (dense row-id sheets: the bit count equals the row count).
    Count,
    /// `N = (max_row_id + 1) - 1` -- a per-element array whose row 0 is an empty header slot.
    CountMinus1,
    /// `N = max(row.<field>) + 1` -- the elements are indexed by an in-row field.
    IndexField(&'static str),
    /// `N = max(row_id where row.<field> == true) + 1` -- indexed by row id, gated on a bool.
    IndexRowIdWhere(&'static str),
    /// `N = max(row_id in [k, k + 10_000)) - k` -- indexed by `row_id - k` within the first block.
    ///
    /// Note there is deliberately **no** `+ 1`: the block's top rows are empty sentinels that carry
    /// no bit, so the count is `max_row_id - offset`. Pinned against the real install (see the
    /// golden test): `20304 - 20000 = 304`, `ceil(304 / 8) = 38`.
    RowIdMinusOffset(u32),
    /// `N = max(row_id where the row is populated) + 1` -- `AetherCurrentCompFlgSet` has a trailing
    /// all-zero row that must be dropped, so a plain row count is one too many. "Populated" means a
    /// non-zero `Territory` link.
    PopulatedRowIdPlus1,
}

/// One owned constant: its yaml key, the icarus sheet it derives from, the [`Rule`], and the
/// [`Unit`]. `expected` is the value pinned against the real install for the golden test.
#[derive(Debug, Clone, Copy)]
struct Descriptor {
    key: &'static str,
    sheet: &'static str,
    rule: Rule,
    unit: Unit,
    /// The value pinned against the real install, asserted only by the golden test. Read in the
    /// `#[cfg(test)]` build, hence `allow(dead_code)` for the plain binary.
    #[allow(dead_code)]
    expected: usize,
}

/// The 18 keys T1 owns. This is the ONLY set of keys the rewriter is allowed to touch.
const DESCRIPTORS: &[Descriptor] = &[
    // -- Count + Div8 --------------------------------------------------------------------------
    d("TITLE_UNLOCK_BITMASK_SIZE", "Title", Rule::Count, Unit::Div8, 112),
    d("ORCHESTRION_ROLL_BITMASK_SIZE", "Orchestrion", Rule::Count, Unit::Div8, 112),
    d("TRIPLE_TRIAD_CARDS_BITMASK_SIZE", "TripleTriadCard", Rule::Count, Unit::Div8, 60),
    d("GLASSES_STYLES_BITMASK_SIZE", "GlassesStyle", Rule::Count, Unit::Div8, 8),
    d("COMPLETED_LEVEQUEST_BITMASK_SIZE", "Leve", Rule::Count, Unit::Div8, 226),
    d("MINION_BITMASK_SIZE", "Companion", Rule::Count, Unit::Div8, 75),
    d("ORNAMENT_BITMASK_SIZE", "Ornament", Rule::Count, Unit::Div8, 8),
    d("AETHER_CURRENT_BITMASK_SIZE", "AetherCurrent", Rule::Count, Unit::Div8, 56),
    d("AETHERYTE_UNLOCK_BITMASK_SIZE", "Aetheryte", Rule::Count, Unit::Div8, 30),
    d("ADVENTURE_BITMASK_SIZE", "Adventure", Rule::Count, Unit::Div8, 43),
    d("CHOCOBO_TAXI_STANDS_BITMASK_SIZE", "ChocoboTaxiStand", Rule::Count, Unit::Div8, 12),
    d("BUDDY_EQUIP_BITMASK_SIZE", "BuddyEquip", Rule::Count, Unit::Div8, 14),
    // -- CountMinus1 + Raw ---------------------------------------------------------------------
    d("BEAST_TRIBE_ARRAY_SIZE", "BeastTribe", Rule::CountMinus1, Unit::Raw, 20),
    // -- IndexField + Div8 ---------------------------------------------------------------------
    d("MOUNT_BITMASK_SIZE", "Mount", Rule::IndexField("Order"), Unit::Div8, 45),
    d("CUTSCENE_SEEN_BITMASK_SIZE", "CutsceneWorkIndex", Rule::IndexField("WorkIndex"), Unit::Div8, 183),
    // -- IndexRowIdWhere(bool) + Div8 ----------------------------------------------------------
    d("CAUGHT_FISH_BITMASK_SIZE", "FishParameter", Rule::IndexRowIdWhere("IsInLog"), Unit::Div8, 191),
    // -- RowIdMinusOffset + Div8 ---------------------------------------------------------------
    d("CAUGHT_SPEARFISH_BITMASK_SIZE", "SpearfishingItem", Rule::RowIdMinusOffset(20000), Unit::Div8, 38),
    // -- Special (trailing-empty-row drop) + Div8 ----------------------------------------------
    d(
        "AETHER_CURRENT_COMP_FLG_SET_BITMASK_SIZE",
        "AetherCurrentCompFlgSet",
        Rule::PopulatedRowIdPlus1,
        Unit::Div8,
        4,
    ),
];

/// `const fn` constructor so [`DESCRIPTORS`] can be a `const` table.
const fn d(
    key: &'static str,
    sheet: &'static str,
    rule: Rule,
    unit: Unit,
    expected: usize,
) -> Descriptor {
    Descriptor {
        key,
        sheet,
        rule,
        unit,
        expected,
    }
}

// -------------------------------------------------------------------------------------------------
// Raw-count computation over the icarus sheets
// -------------------------------------------------------------------------------------------------

/// The number of populated rows in the sheet, matching the `total_rows` EXDViewer reports.
///
/// `EXH::row_count` is `pub(crate)` in physis, so the count is derived by iteration instead. This is
/// the row *ordinal* count, independent of the row-id base: `AetherCurrent` is based at row 2818048
/// yet has 448 rows, so a `max_row_id + 1` would be wildly wrong -- only the iteration count is the
/// bit count.
fn row_count<'a, S, X>(sheet: &'a S) -> u32
where
    &'a S: IntoIterator<Item = (u32, X)>,
{
    sheet.into_iter().count() as u32
}

/// Sheets that carry no localized column, so physis only has a `Language::None` EXD for them.
/// Requesting the config language for one of these fails with `ResolverFailed`.
const LANGUAGE_NEUTRAL_SHEETS: &[&str] = &["AetherCurrent", "CutsceneWorkIndex", "AetherCurrentCompFlgSet"];

/// The language to read `sheet` in: `Language::None` for the neutral sheets above, else `lang`.
fn sheet_language(sheet: &str, lang: Language) -> Language {
    if LANGUAGE_NEUTRAL_SHEETS.contains(&sheet) {
        Language::None
    } else {
        lang
    }
}

/// The raw count `N` for one descriptor, read from the live sheet.
fn raw_count(
    desc: &Descriptor,
    resolver: &mut ResourceResolver,
    lang: Language,
) -> Result<u32, String> {
    use icarus::Adventure::AdventureSheet;
    use icarus::AetherCurrent::AetherCurrentSheet;
    use icarus::AetherCurrentCompFlgSet::AetherCurrentCompFlgSetSheet;
    use icarus::Aetheryte::AetheryteSheet;
    use icarus::BeastTribe::BeastTribeSheet;
    use icarus::BuddyEquip::BuddyEquipSheet;
    use icarus::ChocoboTaxiStand::ChocoboTaxiStandSheet;
    use icarus::Companion::CompanionSheet;
    use icarus::CutsceneWorkIndex::CutsceneWorkIndexSheet;
    use icarus::FishParameter::FishParameterSheet;
    use icarus::GlassesStyle::GlassesStyleSheet;
    use icarus::Leve::LeveSheet;
    use icarus::Mount::MountSheet;
    use icarus::Orchestrion::OrchestrionSheet;
    use icarus::Ornament::OrnamentSheet;
    use icarus::SpearfishingItem::SpearfishingItemSheet;
    use icarus::Title::TitleSheet;
    use icarus::TripleTriadCard::TripleTriadCardSheet;

    let sheet_lang = sheet_language(desc.sheet, lang);

    /// Reads a typed sheet, mapping the error to a readable string.
    macro_rules! read {
        ($ty:ty) => {
            <$ty>::read_from(resolver, sheet_lang)
                .map_err(|e| format!("failed to read the {} sheet: {e:?}", desc.sheet))?
        };
    }

    // A `Count`/`CountMinus1` sheet is bit-indexed by its row *ordinal*, so the raw count is the
    // number of populated rows (matching EXDViewer's `total_rows`), NOT `max_row_id + 1`. Most of
    // these sheets are dense from row 0 so the two coincide, but `AetherCurrent` is a contiguous
    // block based at row 2818048 -- there, only the row count is correct. `CountMinus1` drops the
    // empty row-0 header slot.
    macro_rules! count {
        ($ty:ty) => {{
            let rows = row_count(&read!($ty));
            match desc.rule {
                Rule::Count => rows,
                Rule::CountMinus1 => rows.saturating_sub(1),
                other => return Err(format!("{other:?} is not a plain-count rule")),
            }
        }};
    }

    let n = match desc.sheet {
        // -- Count + Div8 (dense row-id sheets) ------------------------------------------------
        "Title" => count!(TitleSheet),
        "Orchestrion" => count!(OrchestrionSheet),
        "TripleTriadCard" => count!(TripleTriadCardSheet),
        "GlassesStyle" => count!(GlassesStyleSheet),
        "Leve" => count!(LeveSheet),
        "Companion" => count!(CompanionSheet),
        "Ornament" => count!(OrnamentSheet),
        "AetherCurrent" => count!(AetherCurrentSheet),
        "Aetheryte" => count!(AetheryteSheet),
        "Adventure" => count!(AdventureSheet),
        "ChocoboTaxiStand" => count!(ChocoboTaxiStandSheet),
        "BuddyEquip" => count!(BuddyEquipSheet),
        // -- CountMinus1 + Raw -----------------------------------------------------------------
        "BeastTribe" => count!(BeastTribeSheet),
        // -- IndexField + Div8 -----------------------------------------------------------------
        "Mount" => {
            // `Order` is the mount's index into the bitmask; `i16`, never negative in practice.
            let sheet = read!(MountSheet);
            let max = (&sheet)
                .into_iter()
                .filter_map(|(_, subrows)| subrows.into_iter().next())
                .map(|(_, row)| row.Order as i64)
                .max()
                .unwrap_or(-1);
            u32::try_from(max + 1).map_err(|_| "Mount.Order max is negative".to_string())?
        }
        "CutsceneWorkIndex" => {
            let sheet = read!(CutsceneWorkIndexSheet);
            let max = (&sheet)
                .into_iter()
                .filter_map(|(_, subrows)| subrows.into_iter().next())
                .map(|(_, row)| row.WorkIndex as u32)
                .max()
                .unwrap_or(0);
            max + 1
        }
        // -- IndexRowIdWhere(bool) + Div8 ------------------------------------------------------
        "FishParameter" => {
            let sheet = read!(FishParameterSheet);
            let max = (&sheet)
                .into_iter()
                .filter_map(|(row_id, subrows)| {
                    subrows
                        .into_iter()
                        .next()
                        .filter(|(_, row)| row.IsInLog)
                        .map(|_| row_id)
                })
                .max()
                .unwrap_or(0);
            max + 1
        }
        // -- RowIdMinusOffset + Div8 -----------------------------------------------------------
        "SpearfishingItem" => {
            let Rule::RowIdMinusOffset(offset) = desc.rule else {
                return Err("SpearfishingItem expects a RowIdMinusOffset rule".to_string());
            };
            let sheet = read!(SpearfishingItemSheet);
            // Only the first block `[offset, offset + 10_000)` is bitmask-indexed; the 30000 block
            // is a separate id space. No `+ 1`: the block's top rows are empty sentinels (pinned).
            let max = (&sheet)
                .into_iter()
                .map(|(row_id, _)| row_id)
                .filter(|row_id| *row_id >= offset && *row_id < offset + 10_000)
                .max()
                .unwrap_or(offset);
            max - offset
        }
        // -- Special: trailing-empty-row drop + Div8 -------------------------------------------
        "AetherCurrentCompFlgSet" => {
            // The sheet has a trailing all-zero row, so a plain row count is one too many. A row is
            // "populated" iff its `Territory` link is non-zero.
            let sheet = read!(AetherCurrentCompFlgSetSheet);
            let max = (&sheet)
                .into_iter()
                .filter_map(|(row_id, subrows)| {
                    subrows
                        .into_iter()
                        .next()
                        .filter(|(_, row)| row.Territory != 0)
                        .map(|_| row_id)
                })
                .max()
                .unwrap_or(0);
            max + 1
        }
        other => return Err(format!("no reader wired for sheet `{other}`")),
    };

    Ok(n)
}

/// Computes the stored constant for every descriptor. Returns `key -> value`.
fn compute_all(
    resolver: &mut ResourceResolver,
    lang: Language,
) -> Result<BTreeMap<&'static str, usize>, String> {
    let mut out = BTreeMap::new();
    for desc in DESCRIPTORS {
        let n = raw_count(desc, resolver, lang)?;
        out.insert(desc.key, desc.unit.stored(n));
    }
    Ok(out)
}

// -------------------------------------------------------------------------------------------------
// Schema canary
// -------------------------------------------------------------------------------------------------

/// Hard-fails if the game data does not decode the way the rules assume. `--game-path` can point at
/// any install; if its schema disagrees with the icarus pin, columns shift and the tool would emit
/// a plausible-looking but wrong constant. A few known decodes are asserted before computing.
fn check_schema_canary(resolver: &mut ResourceResolver, lang: Language) -> Result<(), String> {
    use icarus::AetherCurrentCompFlgSet::AetherCurrentCompFlgSetSheet;
    use icarus::FishParameter::FishParameterSheet;
    use icarus::Mount::MountSheet;
    use icarus::SpearfishingItem::SpearfishingItemSheet;

    let fail = |what: &str| {
        format!(
            "schema canary failed: {what}. The game data does not decode the way this tool \
             expects -- the install's schema may disagree with the icarus pin \
             (ver/2026.04.21.0000.0000), or the physis subrow-offset patch may not be in effect. \
             Refusing to emit a plausible-looking but WRONG constant."
        )
    };

    // Mount.Order decodes as a small in-row index, not garbage: the max must be well under the row
    // count (a column slip turns it into a huge or negative value).
    let mounts = MountSheet::read_from(resolver, lang)
        .map_err(|e| format!("failed to read Mount for the canary: {e:?}"))?;
    let (rows, max_order) = (&mounts).into_iter().fold((0u32, i64::MIN), |(n, m), (_, s)| {
        let order = s.into_iter().next().map(|(_, r)| r.Order as i64).unwrap_or(i64::MIN);
        (n + 1, m.max(order))
    });
    if rows == 0 || !(0..=(rows as i64 * 2)).contains(&max_order) {
        return Err(fail(&format!(
            "Mount.Order max = {max_order} across {rows} rows is not a plausible in-row index"
        )));
    }

    // A FishParameter row flagged IsInLog exists (the bool column is where we think it is).
    let fish = FishParameterSheet::read_from(resolver, lang)
        .map_err(|e| format!("failed to read FishParameter for the canary: {e:?}"))?;
    if !(&fish)
        .into_iter()
        .filter_map(|(_, s)| s.into_iter().next())
        .any(|(_, r)| r.IsInLog)
    {
        return Err(fail("no FishParameter row has IsInLog = true"));
    }

    // SpearfishingItem's 20000 block exists.
    let spear = SpearfishingItemSheet::read_from(resolver, lang)
        .map_err(|e| format!("failed to read SpearfishingItem for the canary: {e:?}"))?;
    if !(&spear)
        .into_iter()
        .any(|(row_id, _)| (20000..30000).contains(&row_id))
    {
        return Err(fail("SpearfishingItem has no row in the 20000 block"));
    }

    // AetherCurrentCompFlgSet has at least one populated (non-zero Territory) row. It is a
    // language-neutral sheet, so it must be read with `Language::None`.
    let flg = AetherCurrentCompFlgSetSheet::read_from(
        resolver,
        sheet_language("AetherCurrentCompFlgSet", lang),
    )
    .map_err(|e| format!("failed to read AetherCurrentCompFlgSet for the canary: {e:?}"))?;
    if !(&flg)
        .into_iter()
        .filter_map(|(_, s)| s.into_iter().next())
        .any(|(_, r)| r.Territory != 0)
    {
        return Err(fail("AetherCurrentCompFlgSet has no populated row"));
    }

    Ok(())
}

// -------------------------------------------------------------------------------------------------
// Surgical, allowlist-gated YAML rewriter
// -------------------------------------------------------------------------------------------------

/// Rewrites *only* the owned scalar lines of `constants.yml`, leaving every other line -- key order,
/// comments, blank lines, list blocks -- byte-for-byte unchanged.
///
/// This is a line walk, not a serde round-trip: serde would reorder keys, drop comments and re-emit
/// list blocks differently. A line is rewritten iff its `KEY:` prefix is one of `values`' keys. If
/// any owned key is never seen, that is a hard error (a silently-missing key would leave a stale
/// constant in place).
fn rewrite_constants(input: &str, values: &BTreeMap<&str, usize>) -> Result<String, String> {
    // Preserve the input's newline style and whether it ended with one.
    let ends_with_newline = input.ends_with('\n');
    let mut seen: BTreeMap<&str, ()> = BTreeMap::new();
    let mut out: Vec<String> = Vec::new();

    for line in input.split('\n') {
        // The key is everything before the first colon, trimmed. Only top-level `KEY: value` lines
        // (no leading whitespace) are candidates; list items (`- x`) and nested lines never match.
        let key = line.split_once(':').map(|(k, _)| k).unwrap_or("");
        if !key.is_empty() && key == key.trim() {
            if let Some((&owned_key, value)) = values.get_key_value(key) {
                out.push(format!("{owned_key}: {value}"));
                seen.insert(owned_key, ());
                continue;
            }
        }
        out.push(line.to_string());
    }

    let missing: Vec<&str> = values
        .keys()
        .filter(|k| !seen.contains_key(*k))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "these owned keys were not found in constants.yml: {}",
            missing.join(", ")
        ));
    }

    let mut result = out.join("\n");
    // `split('\n')` on a trailing-newline input yields a final empty element, so `join` already
    // reproduces the trailing newline. Guard the (unusual) no-trailing-newline case.
    if !ends_with_newline && result.ends_with('\n') {
        result.pop();
    }
    Ok(result)
}

// -------------------------------------------------------------------------------------------------
// CLI
// -------------------------------------------------------------------------------------------------

/// The repository root, resolved from the crate's compile-time location and thus independent of the
/// current working directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .to_path_buf()
}

/// The default path to `constants.yml`, anchored at the repo root.
fn default_constants_path() -> PathBuf {
    repo_root().join("resources/data/constants.yml")
}

/// The physis language shortnames. Anything else silently degrades to `Language::None`, so `--lang`
/// is validated against this list.
const LANGUAGES: [&str; 8] = ["ja", "en", "de", "fr", "chs", "cht", "tc", "ko"];

fn parse_language(shortname: &str) -> Option<Language> {
    LANGUAGES
        .contains(&shortname)
        .then(|| Language::from_shortname(shortname))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Compute + diff, exit 0 regardless of drift, write nothing.
    DryRun,
    /// Compute + diff, exit 1 on drift, write nothing.
    Check,
    /// Compute + apply the rewrite.
    Write,
}

#[derive(Debug)]
struct Args {
    game_path: Option<String>,
    constants: Option<PathBuf>,
    lang: Option<String>,
    mode: Mode,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            game_path: None,
            constants: None,
            lang: None,
            mode: Mode::DryRun,
        }
    }
}

const HELP: &str = "\
kawari-constants-gen -- regenerate the EXD-derivable size constants in constants.yml

USAGE:
    cargo run -p kawari-constants-gen -- [OPTIONS]

OPTIONS:
    --game-path <PATH>   Game install (sqpack). Default: config.filesystem.game_path
    --constants <PATH>   Path to constants.yml. Default: <repo>/resources/data/constants.yml
    --lang <SHORT>       Sheet language. Default: config.world.language()
    --check              Exit 1 if any owned constant drifted. Writes nothing.
    --write              Apply the rewrite to constants.yml.
    (no flag)            Dry run: print the diff and exit 0.
    -h, --help           Print this help.
";

fn parse_args(argv: &[String]) -> Result<Option<Args>, String> {
    let mut args = Args::default();
    let mut i = 0;

    fn value(argv: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
        *i += 1;
        argv.get(*i)
            .cloned()
            .ok_or_else(|| format!("{flag} requires a value"))
    }

    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => return Ok(None),
            "--game-path" => args.game_path = Some(value(argv, &mut i, "--game-path")?),
            "--constants" => {
                args.constants = Some(PathBuf::from(value(argv, &mut i, "--constants")?))
            }
            "--lang" => args.lang = Some(value(argv, &mut i, "--lang")?),
            "--check" => args.mode = Mode::Check,
            "--write" => args.mode = Mode::Write,
            other => return Err(format!("unknown argument `{other}`")),
        }
        i += 1;
    }

    Ok(Some(args))
}

/// The subset of the file's current values for the owned keys, parsed leniently. Used only to build
/// the diff; the rewrite itself never depends on parsing the old value.
fn current_owned_values(input: &str) -> BTreeMap<&'static str, Option<usize>> {
    let mut out = BTreeMap::new();
    for desc in DESCRIPTORS {
        out.insert(desc.key, None);
    }
    for line in input.split('\n') {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        if key != key.trim() {
            continue;
        }
        if let Some(slot) = out.get_mut(key.trim()) {
            *slot = rest.trim().parse::<usize>().ok();
        }
    }
    out
}

/// A single owned key's before/after. `drift` is true when the stored value would change.
struct DiffRow {
    key: &'static str,
    old: Option<usize>,
    new: usize,
}

fn build_diff(input: &str, values: &BTreeMap<&'static str, usize>) -> Vec<DiffRow> {
    let current = current_owned_values(input);
    let mut rows: Vec<DiffRow> = values
        .iter()
        .map(|(&key, &new)| DiffRow {
            key,
            old: current.get(key).copied().flatten(),
            new,
        })
        .collect();
    rows.sort_by_key(|r| r.key);
    rows
}

fn main() {
    tracing_subscriber::fmt::init();
    std::process::exit(run());
}

fn run() -> i32 {
    use kawari::config::get_config;

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&argv) {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{HELP}");
            return 0;
        }
        Err(error) => {
            eprintln!("error: {error}\n\n{HELP}");
            return 1;
        }
    };

    let config = get_config();
    let game_path = args
        .game_path
        .clone()
        .unwrap_or(config.filesystem.game_path.clone());
    let lang = match &args.lang {
        Some(short) => match parse_language(short) {
            Some(lang) => lang,
            None => {
                eprintln!(
                    "error: unknown --lang `{short}`. Valid values: {}",
                    LANGUAGES.join(", ")
                );
                return 1;
            }
        },
        None => config.world.language(),
    };
    let constants_path = args.constants.clone().unwrap_or_else(default_constants_path);

    if game_path.is_empty() || !Path::new(&game_path).exists() {
        eprintln!(
            "error: no FFXIV install at `{game_path}` (set filesystem.game_path in config.yaml or \
             pass --game-path)."
        );
        return 2;
    }

    let mut resolver = ResourceResolver::new();
    resolver.add_source(SqPackResource::from_existing(&game_path));

    if let Err(error) = check_schema_canary(&mut resolver, lang) {
        eprintln!("error: {error}");
        return 2;
    }

    let values = match compute_all(&mut resolver, lang) {
        Ok(values) => values,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };

    let input = match std::fs::read_to_string(&constants_path) {
        Ok(input) => input,
        Err(error) => {
            eprintln!(
                "error: cannot read `{}`: {error}",
                constants_path.display()
            );
            return 1;
        }
    };

    let diff = build_diff(&input, &values);
    let drifted: Vec<&DiffRow> = diff.iter().filter(|r| r.old != Some(r.new)).collect();

    println!("constants-gen: {} owned keys, {} drifted", diff.len(), drifted.len());
    for row in &drifted {
        match row.old {
            Some(old) => println!("  {} {}->{}", row.key, old, row.new),
            None => println!("  {} (absent)->{}", row.key, row.new),
        }
    }

    match args.mode {
        Mode::DryRun => 0,
        Mode::Check => {
            if drifted.is_empty() {
                println!("constants.yml is up to date.");
                0
            } else {
                eprintln!("constants.yml is stale ({} keys). Run with --write.", drifted.len());
                1
            }
        }
        Mode::Write => {
            if drifted.is_empty() {
                println!("constants.yml is already up to date; nothing to write.");
                return 0;
            }
            let output = match rewrite_constants(&input, &values) {
                Ok(output) => output,
                Err(error) => {
                    eprintln!("error: {error}");
                    return 1;
                }
            };
            if let Err(error) = std::fs::write(&constants_path, output) {
                eprintln!("error: cannot write `{}`: {error}", constants_path.display());
                return 1;
            }
            println!("wrote {} updated keys to {}", drifted.len(), constants_path.display());
            0
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Unit arithmetic (no sqpack) -----------------------------------------------------------

    #[test]
    fn div8_rounds_up_to_whole_bytes() {
        assert_eq!(Unit::Div8.stored(0), 0);
        assert_eq!(Unit::Div8.stored(1), 1);
        assert_eq!(Unit::Div8.stored(8), 1);
        assert_eq!(Unit::Div8.stored(9), 2);
        // The three drifts the acceptance gate expects, at the raw-count level.
        assert_eq!(Unit::Div8.stored(191), 24); // CAUGHT_FISH: ceil(191/8)
        assert_eq!(Unit::Div8.stored(183), 23); // CUTSCENE_SEEN: ceil(183/8)
        assert_eq!(Unit::Div8.stored(112), 14); // BUDDY_EQUIP: ceil(112/8)
    }

    #[test]
    fn raw_is_identity() {
        assert_eq!(Unit::Raw.stored(0), 0);
        assert_eq!(Unit::Raw.stored(20), 20);
    }

    // -- Descriptor table sanity ---------------------------------------------------------------

    #[test]
    fn descriptors_own_exactly_eighteen_unique_keys() {
        assert_eq!(DESCRIPTORS.len(), 18);
        let mut keys: Vec<&str> = DESCRIPTORS.iter().map(|d| d.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), 18, "owned keys must be unique");
    }

    /// T1's keys must be present in the checked-in constants.yml and disjoint from the hardcoded
    /// T2/D (client-fixed) set. This asserts the ownership partition without needing game data.
    #[test]
    fn owned_keys_present_and_disjoint_from_client_fixed() {
        // The client-fixed keys T1 must NEVER touch (from the ownership manifest).
        const CLIENT_FIXED: &[&str] = &[
            "GATHERED_GATHERING_ITEMS_BITMASK_SIZE",
            "UNLOCKED_FISHING_SPOTS_BITMASK_SIZE",
            "CLASSJOB_ARRAY_SIZE",
            "ACTIVE_HELP_BITMASK_SIZE",
            "UNLOCK_BITMASK_SIZE",
            "COMPLETED_LEGACY_QUEST_BITMASK_SIZE",
            "UNLOCKED_MAP_MARKERS_BITMASK_SIZE",
            "COMPLETED_QUEST_BITMASK_SIZE",
            "COMPLETED_RECIPES_BITMASK_SIZE",
            "FRAMERS_KIT_BITMASK_SIZE",
            "AVAILABLE_CLASSJOBS",
            "BEGINNER_TRAINING_ARRAY_SIZE",
            "DUNGEON_ARRAY_SIZE",
            "RAID_ARRAY_SIZE",
            "TRIAL_ARRAY_SIZE",
            "GUILDHEST_ARRAY_SIZE",
            "FRONTLINE_ARRAY_SIZE",
            "CRYSTALLINE_CONFLICT_ARRAY_SIZE",
            "MASKED_CARNIVALE_ARRAY_SIZE",
            "MISC_CONTENT_ARRAY_SIZE",
            "SPECIAL_CONTENT_ARRAY_SIZE",
        ];

        let input = std::fs::read_to_string(default_constants_path())
            .expect("constants.yml must exist at the repo default path");
        let file_keys: std::collections::BTreeSet<&str> = input
            .lines()
            .filter_map(|l| l.split_once(':'))
            .map(|(k, _)| k.trim())
            .filter(|k| !k.is_empty())
            .collect();

        for desc in DESCRIPTORS {
            assert!(
                file_keys.contains(desc.key),
                "owned key `{}` missing from constants.yml",
                desc.key
            );
            assert!(
                !CLIENT_FIXED.contains(&desc.key),
                "owned key `{}` overlaps the client-fixed set",
                desc.key
            );
        }
    }

    // -- Surgical rewriter ---------------------------------------------------------------------

    fn sample_yaml() -> &'static str {
        "# a leading comment\n\
         ALPHA_KEY: 1\n\
         TITLE_UNLOCK_BITMASK_SIZE: 112\n\
         SOME_LIST:\n\
         - 88\n\
         - 120\n\
         CAUGHT_FISH_BITMASK_SIZE: 190\n\
         # trailing comment\n\
         ZULU_KEY: 999\n"
    }

    #[test]
    fn rewriter_changes_only_owned_keys() {
        let mut values = BTreeMap::new();
        values.insert("TITLE_UNLOCK_BITMASK_SIZE", 14usize);
        values.insert("CAUGHT_FISH_BITMASK_SIZE", 24usize);

        let out = rewrite_constants(sample_yaml(), &values).expect("rewrite");
        let expected = "# a leading comment\n\
             ALPHA_KEY: 1\n\
             TITLE_UNLOCK_BITMASK_SIZE: 14\n\
             SOME_LIST:\n\
             - 88\n\
             - 120\n\
             CAUGHT_FISH_BITMASK_SIZE: 24\n\
             # trailing comment\n\
             ZULU_KEY: 999\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn rewriter_preserves_non_owned_lines_byte_for_byte() {
        let mut values = BTreeMap::new();
        values.insert("TITLE_UNLOCK_BITMASK_SIZE", 112usize); // unchanged value
        values.insert("CAUGHT_FISH_BITMASK_SIZE", 190usize);
        let out = rewrite_constants(sample_yaml(), &values).expect("rewrite");
        // No owned key drifted, so the output must be identical to the input.
        assert_eq!(out, sample_yaml());
    }

    #[test]
    fn rewriter_errors_on_missing_owned_key() {
        let mut values = BTreeMap::new();
        values.insert("TITLE_UNLOCK_BITMASK_SIZE", 14usize);
        values.insert("NOT_IN_FILE_KEY", 7usize);
        let err = rewrite_constants(sample_yaml(), &values).expect_err("must fail");
        assert!(err.contains("NOT_IN_FILE_KEY"), "error names the missing key: {err}");
    }

    #[test]
    fn rewriter_never_touches_list_items_or_nested_lines() {
        // A key that only appears as an indented list-block header must not be matched by a
        // same-named owned key (there are none here, but the guard is what matters).
        let mut values = BTreeMap::new();
        values.insert("SOME_LIST", 5usize); // matches the header line, but it has no scalar value
        values.insert("ALPHA_KEY", 2usize);
        let out = rewrite_constants(sample_yaml(), &values).expect("rewrite");
        // `SOME_LIST:` becomes `SOME_LIST: 5` (it IS a top-level key), but the `- 88`/`- 120` items
        // are untouched -- proving list items are never rewritten.
        assert!(out.contains("- 88\n- 120\n"), "list items preserved: {out}");
        assert!(out.contains("ALPHA_KEY: 2\n"));
    }

    // -- Golden test (real install; #[ignore]d) ------------------------------------------------

    /// Pins all 18 computed values against the expected column. Requires a real FFXIV install; run
    /// with `cargo test -p kawari-constants-gen -- --include-ignored`.
    /// `cargo test` runs with the crate directory as cwd, but `get_config` resolves `config.yaml`
    /// relative to the repository root. Match the world server (and actionaudit) by entering it.
    fn goto_repo_root() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let root = repo_root()
                .canonicalize()
                .expect("cannot resolve the repository root");
            std::env::set_current_dir(root).expect("cannot enter the repository root");
        });
    }

    #[test]
    #[ignore = "requires a real FFXIV install (set filesystem.game_path); run with --include-ignored"]
    fn golden_all_eighteen_values_match_expected() {
        use kawari::config::get_config;

        goto_repo_root();
        let config = get_config();
        let game_path = config.filesystem.game_path.clone();
        assert!(
            !game_path.is_empty() && Path::new(&game_path).exists(),
            "no FFXIV install at `{game_path}` (set filesystem.game_path in config.yaml)"
        );

        let mut resolver = ResourceResolver::new();
        resolver.add_source(SqPackResource::from_existing(&game_path));
        let lang = config.world.language();

        check_schema_canary(&mut resolver, lang).expect("schema canary");

        for desc in DESCRIPTORS {
            let n = raw_count(desc, &mut resolver, lang)
                .unwrap_or_else(|e| panic!("compute {}: {e}", desc.key));
            let got = desc.unit.stored(n);
            assert_eq!(
                got, desc.expected,
                "{} computed {got} (raw {n}), expected {}",
                desc.key, desc.expected
            );
        }
    }
}




