#!/usr/bin/env python3
"""Build the script-function catalogue (`fnv.script.functions/1`) from three sources.

1. A private MediaWiki export of the GECK wiki category "Functions (All)"
   (geckwiki.com, Special:Export, current revisions only). Only the *structured* fields of
   each page's {{Function}} template are taken (name, alias, origin, calling reference,
   return value, argument names/types/optionality, categories). Page prose is never copied;
   human-written paraphrases live in `schemas/script/notes.json` and are merged in.
2. The owner's masters (read-only). Compiled script data (SCDA) is lined up against the
   script source (SCTX) shipped next to it, which yields the opcode of every function that
   the shipped scripts call, plus usage counts, Begin-block event codes and condition
   (CTDA) usage. See docs/spec-notes/script-functions.md section 3 for the observations.
3. Optionally xEdit's `Core/wbDefinitionsFNV.pas` (MPL-2.0, TES5Edit/TES5Edit, branch
   dev-4.1.6): its condition-function table gives an index per function name.

Outputs are our own JSON and Markdown; no wiki text and no game content (only names that
the wiki also documents, opcodes, and counts) is written.

usage:
  script_functions.py --wiki C:/fnv-private/geckwiki/functions.xml
                      --data "<FNV>/Data" [--xedit wbDefinitionsFNV.pas]
                      [--notes schemas/script/notes.json]
                      [--out schemas/script/functions.json]
                      [--out-md docs/spec-notes/script-functions-catalogue.md]

Standard library only. Exit codes: 0 ok, 1 input unreadable, 2 usage error.
"""
import argparse
import collections
import html
import json
import os
import re
import struct
import sys
import xml.etree.ElementTree as ET
import zlib

SCHEMA = "fnv.script.functions/1"
WIKI_URL = "https://geckwiki.com/index.php?title="
MASTERS = ["FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm",
           "LonesomeRoad.esm", "GunRunnersArsenal.esm", "CaravanPack.esm",
           "ClassicPack.esm", "MercenaryPack.esm", "TribalPack.esm"]

# Origin codes used by the wiki's {{Function}} template. Only the first three are part of
# the unmodified game; everything else needs a script extender or one of its plugins.
VANILLA_ORIGINS = {"GECK1": "base game (present since the Fallout 3 GECK)",
                   "VEGAS1": "base game (added in New Vegas)",
                   "CONSOLENV": "base game console command"}

# The core extender families (docs/handoff/extender-core-tasks.md, task N1): an NVSE or
# FOSE1 function in one of them gets `core_family`, the name of the spec note
# (docs/spec-notes/ext-<family>.md) that covers it.
CORE_ORIGINS = {"NVSE", "FOSE1"}
CORE_NAMED = {"values": {"typeof", "isformvalid", "isreference"},
              "strings": {"tostring", "tonumber", "numtohex", "getformidstring"},
              "udf": {"call", "setfunctionvalue", "getcallingscript"},
              "events": {"dispatchevent"}}


def core_family(rec):
    """The core family of an extender function, or None."""
    if rec["origin"] not in CORE_ORIGINS:
        return None
    n = rec["name"].lower()
    if n.startswith("sv_"):
        return "strings"
    if n.startswith("ar_"):
        return "arrays"
    if "eventhandler" in n:
        return "events"
    return next((fam for fam, names in CORE_NAMED.items() if n in names), None)


# JIP LN NVSE (docs/handoff/jip-tasks.md, task J1): a JIP or LU (Lutana, now part of
# JIP) function gets `jip_family`, its wiki category without the "(JIP)" suffix, and
# `jip_probe`, how a probe can call it: "pure" (numbers, strings or axes only, no calling
# reference: a runtime-compiled capture query can), "form" (it needs forms or a calling
# reference, built with BuildRef), "none" (variables, arrays or other arguments, or a
# setter returning nothing: a GECK-compiled probe).
JIP_ORIGINS = {"JIP", "LU"}
PURE_KINDS = {"int", "float", "string", "axis"}
FORM_KINDS = PURE_KINDS | {"reference", "form", "actor_value"}


def jip_family(rec):
    """The JIP family of a function (its first wiki category, "(JIP)" ones first), or None."""
    if rec["origin"] not in JIP_ORIGINS:
        return None
    cats = [c for c in rec.get("categories", []) if not c.startswith("Functions")]
    own = [c for c in cats if c.endswith(" (JIP)")]
    pick = (own or cats or [None])[0]
    return pick[:-len(" (JIP)")] if pick and pick.endswith(" (JIP)") else pick


def jip_probe(rec):
    """How a probe can call a JIP function: "pure", "form" or "none"."""
    kinds = {p.get("kind") for p in rec.get("params", [])}
    ret = ((rec.get("returns") or {}).get("type") or "").lower()
    if ret in ("void", "none", "nothing"):
        return "none"
    if kinds <= PURE_KINDS and not rec.get("calling_reference"):
        return "pure"
    if kinds <= FORM_KINDS:
        return "form"
    return "none"


# Statement opcodes below 0x1000 seen in shipped bytecode, named by lining up the source
# line [observed]. Function opcodes are 0x1000 and above.
STATEMENT_OPCODES = {0x10: "Begin", 0x11: "End", 0x15: "Set", 0x16: "If", 0x17: "Else",
                     0x18: "ElseIf", 0x19: "EndIf", 0x1C: "(reference prefix)",
                     0x1D: "ScriptName", 0x1E: "Return"}
EXPRESSION_STATEMENTS = (0x15, 0x16, 0x18, 0x1E)
DECLARATION = re.compile(r"^(short|int|long|float|ref|reference)\b", re.I)
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


# --------------------------------------------------------------------------- wiki dump

def _template_body(text, name):
    m = re.search(r"\{\{\s*" + name + r"\b", text)
    if not m:
        return None
    i = j = m.end()
    depth = 1
    while j < len(text) and depth:
        if text.startswith("{{", j):
            depth += 1
            j += 2
        elif text.startswith("}}", j):
            depth -= 1
            j += 2
        else:
            j += 1
    return text[i:j - 2]


def _split_top(body):
    """Split template parameters on '|' outside nested templates and links."""
    parts, cur, depth, link, i = [], [], 0, 0, 0
    while i < len(body):
        two = body[i:i + 2]
        if two in ("{{", "}}", "[[", "]]"):
            depth += {"{{": 1, "}}": -1}.get(two, 0)
            link += {"[[": 1, "]]": -1}.get(two, 0)
            cur.append(two)
            i += 2
            continue
        if body[i] == "|" and depth == 0 and link == 0:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(body[i])
        i += 1
    parts.append("".join(cur))
    return parts


def template_fields(body):
    body = re.sub(r"<nowiki>(.*?)</nowiki>", lambda m: m.group(1).replace("|", "\0"),
                  body, flags=re.S)
    out = {}
    for part in _split_top(body):
        if "=" in part:
            key, value = part.split("=", 1)
            out[key.strip()] = value.strip().replace("\0", "|")
    return out


def plain(s):
    s = re.sub(r"<ref.*?</ref>", "", s, flags=re.S)
    s = re.sub(r"\[\[(?:[^\]|]*\|)?([^\]]*)\]\]", r"\1", s)
    s = re.sub(r"'''?", "", s)
    s = re.sub(r"<[^>]+>", "", s)
    s = re.sub(r"\{\{[^}]*\}\}", "", s)
    return re.sub(r"\s+", " ", html.unescape(s)).strip()


def _truthy(v):
    return v.strip().lower() in ("y", "yes", "true", "1")


def arg_kind(raw):
    """Map the wiki's free-form argument type to a small vocabulary (raw is kept too)."""
    t = (raw or "").lower().replace(" ", "")
    if not t:
        return "unknown"
    if t.startswith(("int", "integer", "uint", "short", "long", "bool", "0/1", "1/0",
                     "(0/1)", "0/1/2", "1/2", "bitmask", "char")):
        return "int"
    if t.startswith(("float", "double", "number")):
        return "float"
    if t in ("x/y/z", "x/y", "axis"):
        return "axis"
    if t in ("statname", "actorvalue", "actorvaluename"):
        return "actor_value"
    if t.startswith(("string", "editorid", "formattedstring", "formatable", "text")):
        return "string"
    if t.startswith(("array", "anyarray")):
        return "array"
    if t.startswith(("scriptvar", "var", "variable", "returnvariable")):
        return "variable"
    if t.startswith(("ref", "objectref", "actorref", "reference", "callingref")):
        return "reference"
    if t.startswith(("form", "baseform", "anyform", "object", "npc", "quest", "faction",
                     "formlist", "race", "cell", "worldspace", "global", "topic", "perk",
                     "weapon", "armor", "actor", "container", "script", "camerashot",
                     "actorbase", "regionid")):
        return "form"
    return "other"


def parse_wiki(path):
    ns = {"m": "http://www.mediawiki.org/xml/export-0.10/"}
    root = ET.parse(path).getroot()
    out = []
    for page in root.findall("m:page", ns):
        if page.find("m:ns", ns).text != "0":
            continue
        title = page.find("m:title", ns).text
        text = page.find("m:revision/m:text", ns).text or ""
        rec = parse_page(title, text)
        if rec:
            out.append(rec)
    return out


def _item_mod_handler(flag):
    """ppNVSE's OnAttachItemMod / OnDetachItemMod: priority, script, flag, then up to two
    filters, each a number ("1" the item mod, "2" the object) and a form."""
    opt = {"optional": True}
    return [{"name": "priority", "type": "int", "kind": "int"},
            {"name": "script", "type": "ref", "kind": "reference"},
            {"name": flag, "type": "bool", "kind": "int", **opt},
            {"name": "filterIndex1", "type": "string", "kind": "string", **opt},
            {"name": "filter1", "type": "form", "kind": "form", **opt},
            {"name": "filterIndex2", "type": "string", "kind": "string", **opt},
            {"name": "filter2", "type": "form", "kind": "form", **opt}]


# Parameters where a page's argument table is missing or short: what the game took
# [observed: the probe named] or what the page's own example passes [documented].
PARAM_FIXES = {
    # [observed: jip-probes rules1/rules2 (#1021): `let x := ref.SetNthPackage package
    # index` compiled in the game and replaced the package at the index].
    "SetNthPackage": [{"name": "package", "type": "form", "kind": "form"},
                      {"name": "index", "type": "int", "kind": "int"}],
    # [documented: GetCellEncounterZone's example "GetCellEncounterZone VikkiAndVance";
    # observed: showoff2, Doc Mitchell's house].
    "GetCellEncounterZone": [{"name": "cell", "type": "form", "kind": "form"}],
    # [documented: SetZoneMinLevel's example "SetZoneMinLevel YaoGauiDen01 10"; the table
    # lists the zone only].
    "SetZoneMinLevel": [{"name": "encounterZone", "type": "form", "kind": "form"},
                        {"name": "level", "type": "int", "kind": "int"}],
    # [documented: SetMusicTypeDB's example "SetMusicTypeDB DefaultSuccess -100"].
    "SetMusicTypeDB": [{"name": "musicType", "type": "ref", "kind": "reference"},
                       {"name": "dB", "type": "float", "kind": "float"}],
    # [documented: GetAttachedMod's examples, the weapon then the slot ("1",
    # "Barrel Device"); the table lists the weapon only].
    "GetAllAttachedWeaponMods": [{"name": "weapon", "type": "ref", "kind": "reference"},
                                 {"name": "slot", "type": "string", "kind": "string",
                                  "optional": True}],
    # [documented: OnAttachItemMod's example "OnAttachItemMod 1 OnAttachScript 1 "1"
    # itemMod "2" WeapNV9mmPistol": each filter is its number then its form, and the
    # removal form "OnAttachItemMod OnAttachScript 0" has two arguments; OnDetachItemMod
    # the same].
    "OnAttachItemMod": _item_mod_handler("runOnReconstruct"),
    "OnDetachItemMod": _item_mod_handler("runOnDeconstruct"),
    # [documented: the casino pages' examples, "GetCasinoChip GomorrahCasinoData",
    # "SetCasinoChip GomorrahCasinoData TheTopsChip", "GetCasinoWinnings TheTopsCasinoData",
    # "SetCasinoWinnings TheTopsCasinoData 5000"; the tables name the casino only or no type].
    "GetCasinoChip": [{"name": "casino", "type": "form", "kind": "form"}],
    "SetCasinoChip": [{"name": "casino", "type": "form", "kind": "form"},
                      {"name": "chip", "type": "form", "kind": "form"}],
    "GetCasinoWinnings": [{"name": "casino", "type": "form", "kind": "form"}],
    "SetCasinoWinnings": [{"name": "casino", "type": "form", "kind": "form"},
                          {"name": "chips", "type": "int", "kind": "int"}],
    # [documented: AddConditionFormTwo's example "AddConditionFormTwo StealthBoy 0" adds
    # to Base Effect 0's list; every other FormTwo function (GetConditionFormTwoCount,
    # DeleteConditionFormTwo, ...) takes `First` after the entity; the table leaves it out].
    # [documented: ToggleCraftingMessages' example "ToggleCraftingMessages 0 // Disables the
    # crafting messages"; the table gives the argument a type but no name, which the
    # compiler does not count].
    "ToggleCraftingMessages": [{"name": "enable", "type": "int", "kind": "int"}],
    # [documented: DisableAlt's example "TerminalRef.DisableAlt" passes nothing; the table
    # writes the optional mark as "Optional = Optional", which the parser does not read].
    "DisableAlt": [{"name": "FadeOut", "type": "Int", "kind": "int", "optional": True}],
    "AddConditionFormTwo": [{"name": "FormTwoEntity", "type": "ref", "kind": "reference"},
                            {"name": "First", "type": "int", "kind": "int"},
                            {"name": "Condition", "type": "int", "kind": "int",
                             "optional": True}],
}

# Pages whose template `name` is another function's (copied from a sibling page): the
# page title is the function's name [documented: each page's title and its example,
# e.g. "GetConditionFormOneFloat HitReactionIdles 0 1"; the template says
# GetConditionFormOneInt / SetConditionFormOneInt].
NAME_FIXES = {
    "GetConditionFormOneFloat": "GetConditionFormOneFloat",
    "SetConditionFormOneFloat": "SetConditionFormOneFloat",
    "SetConditionFormOneForm": "SetConditionFormOneForm",
    # The template's name has a space for the underscore, so only "Matrix" was kept
    # [documented: the page's title and its link, Matrix_ApplyOperationWithScalar].
    "Matrix ApplyOperationWithScalar": "Matrix_ApplyOperationWithScalar",
}


def parse_page(title, text):
    body = _template_body(text, "Function")
    if body is None:
        return None
    f = template_fields(body)
    name = (plain(f.get("name", "")) or title).split()[0]
    name = NAME_FIXES.get(title, name)
    rec = {"name": name, "page": title, "origin": f.get("origin", "").strip() or None}
    if f.get("alias"):
        rec["aliases"] = [a for a in (x.strip() for x in re.split(r"[,/]| or ",
                                                                  plain(f["alias"])))
                          if a and a.lower() != name.lower() and IDENT.fullmatch(a)]
    if f.get("originVersion"):
        rec["origin_version"] = plain(f["originVersion"])
    if f.get("referenceType"):
        rec["calling_reference"] = plain(f["referenceType"])
    rtype, rname = plain(f.get("returnType", "")), plain(f.get("returnVal", ""))
    if rtype or rname:
        rec["returns"] = {"type": rtype or None, "name": rname or None}
    params = []
    for m in re.finditer(r"\{\{\s*FunctionArgument(.*?)\}\}", f.get("arguments", ""), re.S):
        af = template_fields(m.group(1))
        raw = plain(af.get("Type", "")) or None
        # `{{FunctionArgument |Name = |Type = }}`: the page's empty placeholder for a
        # function that takes nothing (GetCallingScript, KillAll2, ...), not an argument.
        if raw is None and not plain(af.get("Name", "")):
            continue
        p = {"name": plain(af.get("Name", "")), "type": raw, "kind": arg_kind(raw)}
        if _truthy(af.get("Optional", af.get("optional", ""))):
            p["optional"] = True
        params.append(p)
    if name in PARAM_FIXES:
        params = [dict(x) for x in PARAM_FIXES[name]]
    if params:
        rec["params"] = params
    cond = plain(f.get("conditionFunc", "")).split()
    if cond:
        rec["wiki_condition_func"] = cond[0]
    if _truthy(f.get("consoleOnly", "")):
        rec["console_only"] = True
    rec["categories"] = sorted({c.strip().replace("_", " ")
                                for c in re.findall(r"\[\[Category:([^\]|]+)", text)})
    return rec


# --------------------------------------------------------------------------- xEdit table

def parse_xedit(path):
    """Condition-function table: index -> (name, [parameter type names])."""
    table = {}
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = re.match(r"\s*\(Index:\s*(\d+);\s*Name:\s*'([^']+)'(.*)\)", line)
            if m:
                table[int(m.group(1))] = (m.group(2), re.findall(r"ParamType\d:\s*(\w+)",
                                                                 m.group(3)))
    return table


def xedit_from_catalogue(path):
    """The same table rebuilt from an earlier catalogue's `condition_index` and
    `xedit_param_types` (for regenerating without a copy of the xEdit source)."""
    with open(path, encoding="utf-8") as fh:
        doc = json.load(fh)
    return {f["condition_index"]: (f["name"], list(f.get("xedit_param_types") or []))
            for f in doc["functions"] if f.get("condition_index") is not None}


# --------------------------------------------------------------------------- plugins

def iter_script_units(buf, plugin):
    """Yield embedded/standalone scripts (SCHR..SCDA..SCTX) and CTDA function indices."""
    pos, n = 0, len(buf)
    while pos + 24 <= n:
        sig = buf[pos:pos + 4]
        size, flags, form_id = struct.unpack_from("<III", buf, pos + 4)
        if sig == b"GRUP":
            pos += 24
            continue
        data = buf[pos + 24:pos + 24 + size]
        pos += 24 + size
        if flags & 0x00040000:
            try:
                data = zlib.decompressobj().decompress(data[4:])
            except zlib.error:
                continue
        rtype = sig.decode("ascii", "replace")
        i, big, cur = 0, None, None
        while i + 6 <= len(data):
            sub = data[i:i + 4]
            sz = struct.unpack_from("<H", data, i + 4)[0]
            i += 6
            if sub == b"XXXX":
                big = struct.unpack_from("<I", data, i)[0]
                i += sz
                continue
            if big is not None:
                sz, big = big, None
            body = data[i:i + sz]
            i += sz
            if sub == b"SCHR":
                if cur:
                    yield cur
                cur = {"plugin": plugin, "record": rtype, "form_id": form_id,
                       "scda": b"", "sctx": ""}
            elif sub == b"SCDA" and cur is not None:
                cur["scda"] = body
            elif sub == b"SCTX" and cur is not None:
                cur["sctx"] = body.rstrip(b"\0").decode("cp1252", "replace")
            elif sub == b"CTDA" and len(body) >= 10:
                yield {"ctda": struct.unpack_from("<H", body, 8)[0], "record": rtype}
        if cur:
            yield cur


def statements(scda):
    """Top-level statements: (opcode, body, reference_prefixed). None if malformed."""
    out, i, ref = [], 0, False
    while i < len(scda):
        if i + 4 > len(scda):
            return None
        op, ln = struct.unpack_from("<HH", scda, i)
        if op == 0x1C:  # reference prefix: u16 reference index, then the statement
            ref = True
            i += 4
            continue
        out.append((op, scda[i + 4:i + 4 + ln], ref))
        ref = False
        i += 4 + ln
        if i > len(scda):
            return None
    return out


def expression_calls(body):
    """Function opcodes inside an expression body ('X' + u16 opcode). Heuristic scan."""
    found, i = [], 0
    while True:
        i = body.find(b"X", i)
        if i < 0 or i + 3 > len(body):
            return found
        op = struct.unpack_from("<H", body, i + 1)[0]
        if 0x1000 <= op < 0x1800:
            found.append(op)
        i += 1


def source_lines(sctx):
    """Source lines that compile to one statement each (no blanks/comments/declarations)."""
    out = []
    for line in sctx.replace("\r", "").split("\n"):
        line = re.sub(r";.*", "", line).strip()
        if line and not DECLARATION.match(line):
            out.append(line)
    return out


def scan_masters(data_dir, canonical):
    """Collect opcode evidence and usage.

    canonical maps a lowercase identifier (documented name or alias) to its function key.
    Top-level call statements give the called name directly (first identifier of the
    aligned source line). Calls inside expressions are attributed by line co-occurrence:
    per opcode, the function key with the highest Jaccard index over expression lines.
    """
    top = collections.defaultdict(collections.Counter)      # opcode -> first identifier
    line_ops = collections.Counter()
    line_tok = collections.Counter()
    line_co = collections.defaultdict(collections.Counter)
    calls = collections.Counter()
    scripts_with = collections.defaultdict(set)
    begin = collections.defaultdict(collections.Counter)
    stmt_counts = collections.Counter()
    ctda = collections.Counter()
    stats = collections.Counter()
    spelled = collections.Counter()                          # (key, source spelling)
    for plugin in MASTERS:
        path = os.path.join(data_dir, plugin)
        if not os.path.exists(path):
            continue
        with open(path, "rb") as fh:
            buf = fh.read()
        for unit in iter_script_units(buf, plugin):
            if "ctda" in unit:
                ctda[unit["ctda"]] += 1
                continue
            if not unit["scda"] or not unit["sctx"]:
                continue
            stats["scripts"] += 1
            stmts = statements(unit["scda"])
            if stmts is None:
                stats["malformed"] += 1
                continue
            lines = source_lines(unit["sctx"])
            aligned = len(lines) == len(stmts)
            stats["aligned" if aligned else "unaligned"] += 1
            key = (unit["plugin"], unit["form_id"], unit["record"])
            for idx, (op, body, _ref) in enumerate(stmts):
                stmt_counts[op] += 1
                if op >= 0x1000:
                    calls[op] += 1
                    scripts_with[op].add(key)
                if op == 0x10 and len(body) >= 2 and aligned:
                    m = re.match(r"begin\s+([A-Za-z]+)", lines[idx], re.I)
                    if m:
                        begin[struct.unpack_from("<H", body, 0)[0]][m.group(1)] += 1
                if not aligned:
                    continue
                line = lines[idx]
                if op >= 0x1000:
                    m = re.match(r"^(?:[A-Za-z0-9_]+\s*\.\s*)?([A-Za-z_][A-Za-z0-9_]*)", line)
                    if m:
                        top[op][m.group(1)] += 1
                elif op in EXPRESSION_STATEMENTS:
                    ops = set(expression_calls(body))
                    toks = IDENT.findall(re.sub(r'"[^"]*"', "", line))
                    keys = set()
                    for t in toks:
                        k = canonical.get(t.lower())
                        if k:
                            keys.add(k)
                            spelled[(k, t)] += 1
                        else:
                            keys.add("?" + t.lower())
                            spelled[("?" + t.lower(), t)] += 1
                    line_ops.update(ops)
                    line_tok.update(keys)
                    for o in ops:
                        calls[o] += 1
                        scripts_with[o].add(key)
                        line_co[o].update(keys)
    # Expression evidence: the documented function with the best line-level Jaccard index;
    # an undocumented identifier only when no documented one reaches 0.5 and it reaches 0.9.
    expr = {}
    for o, n in line_ops.items():
        scored = sorted(((c / (n + line_tok[k] - c), k, c) for k, c in line_co[o].items()),
                        reverse=True)
        known = [s for s in scored if not s[1].startswith("?")]
        if known and known[0][0] >= 0.5:
            j, k, c = known[0]
        elif scored and scored[0][0] >= 0.9:
            j, k, c = scored[0]
        else:
            continue
        spell = max((s for s in spelled if s[0] == k), key=lambda s: spelled[s])[1]
        expr[o] = {"key": k, "spelling": spell, "lines": c, "jaccard": round(j, 3)}
    return {"top": top, "expr": expr, "calls": calls,
            "scripts": {o: len(s) for o, s in scripts_with.items()},
            "begin": begin, "statements": stmt_counts, "ctda": ctda, "stats": stats}


# --------------------------------------------------------------------------- merge

def canonical_map(wiki, xedit):
    """Lowercase identifier -> function key (lowercase documented name)."""
    canon = {}
    for rec in wiki:
        for n in [rec["name"], rec["page"]] + rec.get("aliases", []):
            canon.setdefault(n.lower(), rec["name"].lower())
    for name, _ in xedit.values():
        canon.setdefault(name.lower(), name.lower())
    return canon


MIN_SPELLING_USES = 3   # an undocumented spelling needs this many aligned call lines


def resolve_opcodes(scan, canon, xedit):
    """Opcode -> function key, plus spellings, undocumented names and leftovers."""
    resolved, spellings, extra, conflicts = {}, collections.defaultdict(collections.Counter), {}, []
    for op in sorted(set(scan["top"]) | set(scan["expr"])):
        known, unknown = collections.Counter(), collections.Counter()
        shown = {}
        for tok, c in scan["top"].get(op, {}).items():
            k = canon.get(tok.lower())
            if k:
                known[k] += c
                spellings[k][tok] += c
            else:
                shown.setdefault(tok.lower(), tok)
                unknown[shown[tok.lower()]] += c
        e = scan["expr"].get(op)
        if e and not e["key"].startswith("?"):
            known[e["key"]] += e["lines"]
            spellings[e["key"]][e["spelling"]] += e["lines"]
        elif e:
            unknown[e["spelling"]] += e["lines"]
        if known:
            key = known.most_common(1)[0][0]
            for tok, c in unknown.items():
                if c >= MIN_SPELLING_USES:
                    spellings[key][tok] += c
            noise = {k: c for k, c in known.items() if k != key}
            noise.update({t: c for t, c in unknown.items() if c < MIN_SPELLING_USES})
            if noise:
                conflicts.append({"opcode": f"0x{op:04X}", "chosen": key,
                                  "chosen_lines": known[key], "other_tokens": noise})
        else:
            xname = xedit.get(op - 0x1000, (None,))[0]
            tok = unknown.most_common(1)[0][0]
            key = (xname or tok).lower()
            extra.setdefault(key, xname or tok)
            spellings[key][tok] += unknown[tok]
        resolved[op] = key
    return resolved, spellings, extra, conflicts


def build(wiki, scan, xedit, notes):
    canon = canonical_map(wiki, xedit)
    xedit_by_name = {name.lower(): (idx, params) for idx, (name, params) in xedit.items()}
    resolved, spellings, extra, conflicts = resolve_opcodes(scan, canon, xedit)
    observed = collections.defaultdict(dict)    # function key -> {opcode: "observed"}
    for op, key in resolved.items():
        observed[key][op] = "observed"

    entries = [dict(rec) for rec in wiki]
    wiki_keys = {rec["name"].lower() for rec in wiki}
    xedit_names = {name.lower(): name for name, _ in xedit.values()}
    for key in observed:
        if key not in extra:
            extra[key] = xedit_names.get(key) or spellings[key].most_common(1)[0][0]
    for key, shown in sorted(extra.items()):
        if key not in wiki_keys:
            entries.append({"name": shown, "page": None, "origin": "observed-only",
                            "categories": []})

    anomalies = []
    for e in entries:
        key = e["name"].lower()
        vanilla = e["origin"] in VANILLA_ORIGINS or e["origin"] == "observed-only"
        e["family"] = "vanilla" if vanilla else "extender"
        if not vanilla and core_family(e):
            e["core_family"] = core_family(e)
        if not vanilla and jip_family(e):
            e["jip_family"] = jip_family(e)
            e["jip_probe"] = jip_probe(e)
        if e.get("page"):
            e["wiki"] = WIKI_URL + e["page"].replace(" ", "_")
        ops = observed.get(key, {})
        xi = xedit_by_name.get(key)
        if xi and xi[0] < 0x1000 and vanilla:
            e["condition_index"] = xi[0]
            e["xedit_param_types"] = xi[1]
        if ops:
            e["opcodes"] = [f"0x{o:04X}" for o in sorted(ops)]
            e["opcode_evidence"] = "observed"
            if len(ops) > 1:
                anomalies.append({"function": e["name"], "opcodes": e["opcodes"]})
            if xi and xi[0] + 0x1000 not in ops:
                anomalies.append({"function": e["name"], "opcodes": e["opcodes"],
                                  "xedit_index": xi[0]})
        elif xi and vanilla and xi[0] < 0x1000:
            e["opcodes"] = [f"0x{xi[0] + 0x1000:04X}"]
            e["opcode_evidence"] = "xedit_index"
        if ops:
            e["script_calls"] = sum(scan["calls"][o] for o in ops)
            e["scripts_using"] = sum(scan["scripts"].get(o, 0) for o in ops)
            documented = {key} | {a.lower() for a in e.get("aliases", [])}
            seen = set()
            sp = []
            for s, _ in spellings[key].most_common():
                if s.lower() not in documented and s.lower() not in seen:
                    seen.add(s.lower())
                    sp.append(s)
            if sp:
                e["observed_spellings"] = sp
        if xi and xi[0] in scan["ctda"]:
            e["condition_uses"] = scan["ctda"][xi[0]]
        n = notes.get(e.get("page") or e["name"])
        if n:
            for k in ("summary", "notes", "bugs"):
                if n.get(k):
                    e[k] = n[k]
    order = {"vanilla": 0, "extender": 1}
    entries.sort(key=lambda e: (order[e["family"]], e["name"].lower()))
    begin = [{"code": f"0x{c:04X}", "block": names.most_common(1)[0][0],
              "scripts": sum(names.values())}
             for c, names in sorted(scan["begin"].items())]
    stmts = [{"opcode": f"0x{o:04X}", "statement": STATEMENT_OPCODES.get(o, "?"),
              "count": c} for o, c in sorted(scan["statements"].items()) if o < 0x1000]
    return {"schema": SCHEMA,
            "sources": {
                "wiki": "GECK wiki, Category:Functions (All), Special:Export (structured "
                        "template fields only; prose paraphrased in notes.json)",
                "masters": MASTERS,
                "xedit": "TES5Edit/TES5Edit dev-4.1.6 Core/wbDefinitionsFNV.pas "
                         "(MPL-2.0), condition-function table"},
            "scan": dict(scan["stats"]),
            "statement_opcodes": stmts,
            "begin_blocks": begin,
            "opcode_conflicts": conflicts,
            "unresolved_opcodes": [{"opcode": f"0x{o:04X}", "calls": c}
                                   for o, c in sorted(scan["calls"].items())
                                   if o not in resolved],
            "anomalies": anomalies,
            "functions": entries}


def render_json(doc):
    """One list element per line, so a regenerated catalogue diffs line by line."""
    parts = []
    for key, value in doc.items():
        if isinstance(value, list) and value:
            items = ",\n".join("  " + json.dumps(v, ensure_ascii=False) for v in value)
            parts.append(f" {json.dumps(key)}: [\n{items}\n ]")
        else:
            parts.append(f" {json.dumps(key)}: {json.dumps(value, ensure_ascii=False)}")
    return "{\n" + ",\n".join(parts) + "\n}\n"


# --------------------------------------------------------------------------- markdown

def render_md(doc):
    fns = [f for f in doc["functions"] if f["family"] == "vanilla"]
    out = ["# Script function catalogue: base game (generated)", "",
           "Generated by `tools/capture/script_functions.py` from "
           "`schemas/script/functions.json`; do not edit by hand. Conventions and tags: "
           "`docs/spec-notes/script-functions.md`.", "",
           "Columns: opcode (`obs` = observed in shipped bytecode, `xe` = xEdit condition "
           "index + 0x1000), calls / scripts = uses in the shipped masters' compiled scripts, "
           "cond = uses in CTDA conditions, params (`?` = optional).", ""]
    by_cat = collections.defaultdict(list)
    for f in fns:
        cats = [c for c in f.get("categories", []) if c not in (
            "Functions", "Functions (GECK)", "Functions (FO3)", "Functions (Vegas)",
            "Condition Functions", "Console Functions", "Deprecated Functions",
            "Savebaked Functions", "Engine Differences (Fallout)", "Conditions",
            "Unused Functions", "Functions (Consoles)")]
        by_cat[cats[0] if cats else "Uncategorised"].append(f)
    for cat in sorted(by_cat):
        out += [f"## {cat}", "",
                "| Function | Opcode | Calls / scripts | Cond | Params | Returns | Summary |",
                "|---|---|---|---|---|---|---|"]
        for f in sorted(by_cat[cat], key=lambda f: f["name"].lower()):
            name = f"[{f['name']}]({f['wiki']})" if f.get("wiki") else f["name"]
            if f.get("aliases"):
                name += " (" + ", ".join(f["aliases"]) + ")"
            if f.get("calling_reference"):
                name = "ref." + name
            op = ""
            if f.get("opcodes"):
                op = " ".join(f["opcodes"]) + (" obs" if f["opcode_evidence"] == "observed"
                                               else " xe")
            use = f"{f['script_calls']} / {f['scripts_using']}" if f.get("script_calls") else ""
            params = ", ".join(p["name"] + ("?" if p.get("optional") else "") + ":" + p["kind"]
                               for p in f.get("params", []))
            ret = (f.get("returns") or {}).get("type") or ""
            summ = (f.get("summary") or "").replace("|", "/")
            out.append(f"| {name} | {op} | {use} | {f.get('condition_uses', '')} | "
                       f"{params.replace('|', '/')} | {ret.replace('|', '/')} | {summ} |")
        out.append("")
    return "\n".join(out) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--wiki", required=True)
    ap.add_argument("--data", required=True)
    ap.add_argument("--xedit")
    ap.add_argument("--xedit-from-catalogue", metavar="FUNCTIONS_JSON",
                    help="take the xEdit table from an earlier functions.json instead")
    ap.add_argument("--notes")
    ap.add_argument("--out", default="schemas/script/functions.json")
    ap.add_argument("--out-md", default="docs/spec-notes/script-functions-catalogue.md")
    args = ap.parse_args(argv)
    try:
        wiki = parse_wiki(args.wiki)
        if args.xedit:
            xedit = parse_xedit(args.xedit)
        elif args.xedit_from_catalogue:
            xedit = xedit_from_catalogue(args.xedit_from_catalogue)
        else:
            xedit = {}
        notes = {}
        if args.notes:
            with open(args.notes, encoding="utf-8") as fh:
                notes = {n["page"]: n for n in json.load(fh)["functions"]}
    except (OSError, ET.ParseError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    scan = scan_masters(args.data, canonical_map(wiki, xedit))
    doc = build(wiki, scan, xedit, notes)
    with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(render_json(doc))
    with open(args.out_md, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(render_md(doc))
    fam = collections.Counter(f["family"] for f in doc["functions"])
    print(json.dumps({"functions": dict(fam), "scan": doc["scan"],
                      "with_opcode": sum(1 for f in doc["functions"] if f.get("opcodes")),
                      "conflicts": len(doc["opcode_conflicts"]),
                      "anomalies": doc["anomalies"]}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
