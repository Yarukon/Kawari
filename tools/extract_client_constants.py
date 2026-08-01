"""
extract_client_constants.py  (T2)

Run INSIDE IDA (Alt+F7 -> select this file), or via the IDA MCP `py_exec_file`,
with an ffxiv_dx11 client loaded. Regenerates the eight *client-fixed* size
constants in resources/data/constants.yml straight from the client's own
`IsXxxUnlocked`-family range guards.

Unlike the EXD-derived keys (owned by `tools/constants-gen`), these eight sizes
are fixed by FFXIVClientStructs `PlayerState`/`QuestManager` arrays. Each getter
guards its bit-array access with a compare against the fixed array bound; this
tool reads that bound immediate directly, so the value tracks the client build
rather than a stale hand-maintained number.

Two phases (selected by the T2_MODE env var, or argv[1]):
  gen-patterns : run on the RENAMED int-client 7.55 IDB. Self-checks every
                 embedded baseline target (pattern is unique AND resolves to its
                 baseline RVA AND the extracted size equals `expect`), then
                 (re)writes tools/client_constants_patterns.json. Committed.
  apply        : (default) locate each guard by its byte pattern in whatever exe
                 is loaded (int OR CN -- patterns are instruction bytes, so they
                 are symbol-independent), read the bound immediate, and surgically
                 rewrite the eight scalar keys in constants.yml.
                 Set T2_DRY_RUN=1 to print the diff without touching the file.

fail-fast: any pattern miss, any non-unique match, or any parse failure raises
and STOPS immediately. The tool never falls back to a stale value.

This is a companion to extract_obfuscation.py and follows the same house style.
"""

import json
import os

import idaapi
import idautils
import idc
import ida_bytes

# --------------------------------------------------------------------------
# Config
# --------------------------------------------------------------------------
KAWARI_ROOT = r"F:\FFXIVPluginSRCs\Kawari"

PATTERNS_JSON = os.path.join(KAWARI_ROOT, "tools", "client_constants_patterns.json")
CONSTANTS_YML = os.path.join(KAWARI_ROOT, "resources", "data", "constants.yml")

IMAGE_BASE = idaapi.get_imagebase()

# The baseline targets embedded below are the source of truth for `gen-patterns`.
# `apply` reads tools/client_constants_patterns.json instead (so a rebuilt pattern
# file drives extraction without editing this script). Keep the two in sync via
# gen-patterns. RVAs are int-7.55 (relative to a 0x140000000 image base).
BASELINE_MODULE = "ffxiv_dx11_worldwide_7_55.exe"
BASELINE_IMAGEBASE = 0x140000000

# unit -> byte-size conversion of the bound immediate K.
#   bit          guard tests the raw bit index (a2 < K)         -> ceil(K/8)
#   byte         guard tests a pre-shifted byte index (a2>>3<K)  -> K
#   byte_masked  guard tests the masked index ((a2 & ~7) < K)    -> K/8
_UNITS = ("bit", "byte", "byte_masked")

BASELINE_TARGETS = [
    # key, fn, baseline_rva, pattern, imm_offset, imm_size, unit, expect
    ("FRAMERS_KIT_BITMASK_SIZE", "PlayerState.IsFramersKitUnlocked", 0xBE2060,
     "B8 ?? ?? ?? ?? 4C 8B C9 66 3B D0 73 ?? 0F B7 CA", 1, 4, "bit", 44),
    ("GATHERED_GATHERING_ITEMS_BITMASK_SIZE", "QuestManager.IsGatheringItemGathered", 0xE9464A,
     "8B D9 8B F9 C1 EB 03 83 FB ?? 72", 9, 1, "byte", 104),
    # COMPLETED_LEGACY_QUEST_BITMASK_SIZE is intentionally NOT extracted: FFXIV 1.x quest
    # completion flags are discontinued content (frozen since 2012), so the client's
    # IsLegacyQuestComplete bound never moves. It is pinned manually in constants.yml (D tier)
    # rather than kept as a pattern that could fail-fast the whole tool for a dead constant.
    ("COMPLETED_QUEST_BITMASK_SIZE", "QuestManager.IsQuestComplete1", 0xDF6280,
     "0F B7 C2 4C 8B C9 44 8B C0 49 C1 E8 03 49 81 F8 ?? ?? ?? ?? 72", 16, 4, "byte", 751),
    ("COMPLETED_RECIPES_BITMASK_SIZE", "QuestManager.IsRecipeComplete", 0xE9F4B7,
     "45 32 C0 C1 E9 03 81 F9 ?? ?? ?? ?? 73", 8, 4, "byte", 801),
    ("UNLOCKED_MAP_MARKERS_BITMASK_SIZE", "QuestManager.IsMapMarkerUnlocked", 0xDF62D3,
     "44 8B C3 49 C1 E8 03 49 83 F8 ?? 73", 10, 1, "byte", 64),
    ("UNLOCK_BITMASK_SIZE", "UIState.SetUnlockLinkValue", 0xC48D04,
     "8B C2 83 E0 F8 3D ?? ?? ?? ?? 73 ?? 44 8B CA 49 C1 E9 03", 6, 4, "byte_masked", 92),
    ("ACTIVE_HELP_BITMASK_SIZE", "UIState.AnnounceHowTo", 0xC4994A,
     "8B D3 83 E2 F8 81 FA ?? ?? ?? ?? 73", 7, 4, "byte_masked", 38),
    ("TRIPLE_TRIAD_NPC_BITMASK_SIZE", "UIState.IsTripleTriadNpcBeaten", 0xC49710,
     "8B C8 C1 E8 03 83 F8 ?? 73 ?? 83 E1 07 BA 01 00 00 00 D3 E2", 7, 1, "byte", 17),
    # CONTENT_ROULETTE_ARRAY_SIZE is a plain 12-entry byte array (one byte per roulette entry),
    # NOT a bitmask. We anchor on ContentRoulette.CanGetAwards, whose guard is a strict `< 0xC`
    # (K=12) over the same byte_142AAEE58 array; here `unit=byte` means "direct array length"
    # (returns K unchanged), which is numerically correct even though there is no >>3 shift.
    ("CONTENT_ROULETTE_ARRAY_SIZE", "ContentRoulette.CanGetAwards", 0xC24490,
     "80 78 47 0C 7D ?? 0F B6 4B 08", 3, 1, "byte", 12),
]

# --------------------------------------------------------------------------
# Low-level IDA helpers
# --------------------------------------------------------------------------
def _text_bounds():
    for seg in idautils.Segments():
        if idc.get_segm_name(seg) == ".text":
            return idc.get_segm_start(seg), idc.get_segm_end(seg)
    raise RuntimeError(".text segment not found")


def _find_all(pattern, lo, hi):
    """Every address in [lo, hi) whose bytes match `pattern` (?? = wildcard)."""
    out, ea = [], lo
    while True:
        ea = ida_bytes.find_bytes(pattern, range_start=ea, range_end=hi)
        if ea == idc.BADADDR:
            break
        out.append(ea)
        ea += 1
    return out


def _find_unique(pattern, lo, hi, key):
    """The single match for `pattern`, or raise. Non-unique is fail-fast."""
    hits = _find_all(pattern, lo, hi)
    if not hits:
        raise RuntimeError("%s: pattern not found: %s" % (key, pattern))
    if len(hits) > 1:
        raise RuntimeError(
            "%s: pattern is not unique (%d matches: %s): %s"
            % (key, len(hits), ", ".join(hex(h) for h in hits), pattern)
        )
    return hits[0]


def _read_imm(ea, imm_offset, imm_size):
    """Little-endian immediate of `imm_size` bytes at ea+imm_offset."""
    raw = ida_bytes.get_bytes(ea + imm_offset, imm_size)
    if raw is None or len(raw) != imm_size:
        raise RuntimeError("failed reading %d imm bytes at 0x%X" % (imm_size, ea + imm_offset))
    return int.from_bytes(raw, "little")


def _to_size(bound, unit):
    """Convert a guard bound immediate to a constants.yml byte size."""
    if unit == "bit":
        return (bound + 7) // 8
    if unit == "byte":
        return bound
    if unit == "byte_masked":
        return bound // 8
    raise RuntimeError("unknown unit %r (expected one of %s)" % (unit, _UNITS))


# --------------------------------------------------------------------------
# Extraction
# --------------------------------------------------------------------------
def _extract_one(t, lo, hi):
    """Locate one target's guard and return (key, size, detail-dict). Fail-fast."""
    ea = _find_unique(t["pattern"], lo, hi, t["key"])
    bound = _read_imm(ea, t["imm_offset"], t["imm_size"])
    size = _to_size(bound, t["unit"])
    return t["key"], size, {
        "addr": ea, "rva": ea - IMAGE_BASE, "bound": bound,
        "unit": t["unit"], "fn": t.get("fn", "?"),
    }


def _extract_all(targets):
    """Extract every target; returns ({key: size}, {key: detail})."""
    lo, hi = _text_bounds()
    sizes, details = {}, {}
    for t in targets:
        key, size, detail = _extract_one(t, lo, hi)
        sizes[key] = size
        details[key] = detail
    return sizes, details


# --------------------------------------------------------------------------
# gen-patterns: self-check the embedded baseline, then write the pattern file
# --------------------------------------------------------------------------
def _baseline_target_dicts():
    out = []
    for key, fn, rva, pat, ioff, isz, unit, expect in BASELINE_TARGETS:
        out.append({
            "key": key, "fn": fn, "baseline_rva": rva, "pattern": pat,
            "imm_offset": ioff, "imm_size": isz, "unit": unit, "expect": expect,
        })
    return out


def gen_patterns():
    targets = _baseline_target_dicts()
    lo, hi = _text_bounds()
    print("=== T2 gen-patterns (baseline self-check) ===")
    print("module 0x%X..0x%X  imagebase 0x%X" % (lo, hi, IMAGE_BASE))

    for t in targets:
        key, size, detail = _extract_one(t, lo, hi)
        want_rva = t["baseline_rva"]
        if detail["rva"] != want_rva:
            raise RuntimeError(
                "%s: match at rva 0x%X, expected baseline rva 0x%X"
                % (key, detail["rva"], want_rva)
            )
        if size != t["expect"]:
            raise RuntimeError(
                "%s: extracted %d, baseline expected %d (bound 0x%X, unit %s)"
                % (key, size, t["expect"], detail["bound"], t["unit"])
            )
        print("  OK %-38s 0x%-8X bound 0x%-4X %-11s -> %d"
              % (key, detail["addr"], detail["bound"], detail["unit"], size))

    doc = {
        "_comment": [
            "Byte-pattern data for tools/extract_client_constants.py (T2).",
            "Regenerate with:  T2_MODE=gen-patterns  on the RENAMED int-client 7.55 IDB.",
            "unit -> size: bit=ceil(K/8), byte=K, byte_masked=K//8.",
            "Patterns match instruction bytes only, so they are symbol-independent.",
        ],
        "meta": {
            "baseline_module": BASELINE_MODULE,
            "baseline_imagebase": hex(BASELINE_IMAGEBASE),
        },
        "targets": [
            {
                "key": t["key"], "fn": t["fn"],
                "baseline_rva": hex(t["baseline_rva"]),
                "pattern": t["pattern"],
                "imm_offset": t["imm_offset"], "imm_size": t["imm_size"],
                "unit": t["unit"], "expect": t["expect"],
            }
            for t in targets
        ],
    }
    with open(PATTERNS_JSON, "w", encoding="utf-8") as f:
        json.dump(doc, f, indent=2)
        f.write("\n")
    print("wrote %s (%d targets)" % (PATTERNS_JSON, len(targets)))


# --------------------------------------------------------------------------
# apply: extract from the loaded exe using the pattern file, rewrite the yml
# --------------------------------------------------------------------------
def _load_patterns():
    with open(PATTERNS_JSON, "r", encoding="utf-8") as f:
        doc = json.load(f)
    targets = doc.get("targets")
    if not targets:
        raise RuntimeError("%s has no targets" % PATTERNS_JSON)
    for t in targets:
        for field in ("key", "pattern", "imm_offset", "imm_size", "unit"):
            if field not in t:
                raise RuntimeError("pattern entry missing %r: %r" % (field, t))
        if t["unit"] not in _UNITS:
            raise RuntimeError("%s: bad unit %r" % (t["key"], t["unit"]))
    return targets


def _rewrite_yaml(path, values):
    """Surgically replace `KEY: <int>` scalars in-place. Fail if a key is absent."""
    with open(path, "r", encoding="utf-8") as f:
        lines = f.read().split("\n")

    out, seen = [], set()
    for line in lines:
        key = line.split(":", 1)[0].strip()
        if key in values:
            out.append("%s: %s" % (key, values[key]))
            seen.add(key)
        else:
            out.append(line)

    missing = set(values) - seen
    if missing:
        raise RuntimeError("constants.yml keys not found: %s" % ", ".join(sorted(missing)))

    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(out))


def _current_yaml_values(path, keys):
    """Read the current int value of each key from the yml (for a diff preview)."""
    cur = {}
    with open(path, "r", encoding="utf-8") as f:
        for line in f.read().split("\n"):
            k, sep, v = line.partition(":")
            k = k.strip()
            if sep and k in keys:
                try:
                    cur[k] = int(v.strip())
                except ValueError:
                    cur[k] = v.strip()
    return cur


def apply(dry_run):
    targets = _load_patterns()
    print("=== T2 apply%s ===" % (" (dry-run)" if dry_run else ""))
    print("module imagebase 0x%X  patterns %s" % (IMAGE_BASE, PATTERNS_JSON))

    sizes, details = _extract_all(targets)
    current = _current_yaml_values(CONSTANTS_YML, set(sizes))

    changed = 0
    for key in sorted(sizes):
        d, new = details[key], sizes[key]
        old = current.get(key)
        mark = "=" if old == new else ">"
        if old != new:
            changed += 1
        print("  %s %-38s %6s -> %-6d (0x%X bound 0x%X %s)"
              % (mark, key, old, new, d["addr"], d["bound"], d["unit"]))
    print("%d of %d keys differ from constants.yml" % (changed, len(sizes)))

    if dry_run:
        print("dry-run: constants.yml NOT modified")
        return
    _rewrite_yaml(CONSTANTS_YML, sizes)
    print("updated %s" % CONSTANTS_YML)


# --------------------------------------------------------------------------
# Entry
# --------------------------------------------------------------------------
def _mode():
    argv = list(idc.ARGV[1:]) if len(idc.ARGV) > 1 else []
    mode = os.environ.get("T2_MODE", argv[0] if argv else "apply").strip()
    dry = os.environ.get("T2_DRY_RUN", "").strip() not in ("", "0", "false", "False")
    if "--dry-run" in argv:
        dry = True
    return mode, dry


def main():
    mode, dry = _mode()
    if mode == "gen-patterns":
        gen_patterns()
    elif mode == "apply":
        apply(dry)
    else:
        raise RuntimeError("unknown T2_MODE %r (expected gen-patterns|apply)" % mode)
    print("Done.")


main()

