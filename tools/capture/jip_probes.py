#!/usr/bin/env python3
"""JIP LN NVSE probes without the GECK (docs/handoff/jip-tasks.md, task J3).

  jip_probes.py job [--set S] [-o JOB.steps.json]       write the capture job
  jip_probes.py collect OUT.jsonl [--set S] [-o golden]  capture output -> fnv.ext.probes/1
  jip_probes.py compare GOLDEN.json                the golden against the expectations here
  jip_probes.py steps [--dir DIR]                  every set's job into tools/capture/jip-probes/

Each probe is a function script that nvf_capture compiles inside the running game and
whose result it records (docs/spec-notes/xnvse-capture.md §4–§5): JIP is loaded in the
owner's game, so read-only JIP calls with number, string or inline-array arguments can be
probed unattended through capture_run.py. Only getters are probed here: nothing that
changes the game, the save or a file. A probe's expected value is filled only where the
function's wiki page states the exact result [documented: the page named]; the others are
recorded as observed. The golden goes to `<goldens>/jip/<set>.json` (private).

Standard library only.
"""
import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import capture_steps as cs  # noqa: E402
import ext_probes as ep  # noqa: E402


# (id, script body after `Begin Function { }`, text result?, expected or None, page)
# A body that needs a ref variable declares it before Begin (only that works at run time).
PROBES = [
    ("fsin.90", "SetFunctionValue (fsin 90)", False, "1", "Fsin (degrees)"),
    ("fsin.30", "SetFunctionValue (fsin 30)", False, "0.5", "Fsin"),
    ("fcos.60", "SetFunctionValue (fcos 60)", False, "0.5", "Fcos (degrees)"),
    ("fcos.0", "SetFunctionValue (fcos 0)", False, "1", "Fcos"),
    ("ftan.45", "SetFunctionValue (ftan 45)", False, "1", "Ftan (degrees)"),
    ("fasin.1", "SetFunctionValue (fasin 1)", False, "90", "Fasin (degrees)"),
    ("fasin.out", "SetFunctionValue (fasin 2)", False, None, ""),
    ("facos.0", "SetFunctionValue (facos 0)", False, "90", "Facos (degrees)"),
    ("fatan.1", "SetFunctionValue (fatan 1)", False, "45", "Fatan (degrees)"),
    ("fatan2.q1", "SetFunctionValue (fatan2 1 1)", False, "45", "Fatan2 (y, x)"),
    ("fatan2.q2", "SetFunctionValue (fatan2 1 -1)", False, "135", "Fatan2 (quadrant from both signs)"),
    ("fatan2.q3", "SetFunctionValue (fatan2 -1 -1)", False, "-135", "Fatan2"),
    ("fsqrt.16", "SetFunctionValue (fsqrt 16)", False, "4", "Fsqrt"),
    ("fsqrt.neg", "SetFunctionValue (fsqrt -1)", False, None, ""),
    ("maxof.3", "SetFunctionValue (GetMaxOf 1 5 3)", False, "5", "GetMaxOf"),
    ("maxof.neg", "SetFunctionValue (GetMaxOf -1 -5)", False, "-1", "GetMaxOf"),
    ("minof.3", "SetFunctionValue (GetMinOf 1 5 3)", False, "1", "GetMinOf"),
    ("minof.5", "SetFunctionValue (GetMinOf 9 8 7 6 5)", False, "5", "GetMinOf (up to five values)"),
    ("av.unknown", "SetFunctionValue (StringToActorValue \"NotAnActorValue\")", False, "-1", "StringToActorValue (-1 for no match)"),
    ("av.strength", "SetFunctionValue (StringToActorValue \"Strength\")", False, None, ""),
    ("av.damagethreshold", "SetFunctionValue (StringToActorValue \"DamageThreshold\")", False, None, ""),
    ("av.case", "SetFunctionValue (StringToActorValue \"strength\")", False, None, ""),
    ("modname.0", "SetFunctionValue (GetModName 0)", True, "FalloutNV", "GetModName"),
    ("modname.0ext", "SetFunctionValue (GetModName 0 1)", True, "FalloutNV.esm", "GetModName (1 keeps the extension)"),
    ("modname.99", "SetFunctionValue (GetModName 99)", True, None, ""),
    ("headerversion.fnv", "SetFunctionValue (GetPluginHeaderVersion \"FalloutNV.esm\")", False, None, ""),
    ("validatemod.0", "SetFunctionValue (ValidateModIndex 0)", False, None, ""),
    ("validatemod.200", "SetFunctionValue (ValidateModIndex 200)", False, None, ""),
    ("formfrommod.player", "ref rF\nBegin Function { }\nlet rF := GetFormFromMod \"FalloutNV.esm\" \"14\"\nSetFunctionValue (GetFormIDString rF)", True, "00000014", "GetFormFromMod (ID without the mod index)"),
    ("formfrommod.zeros", "ref rF\nBegin Function { }\nlet rF := GetFormFromMod \"FalloutNV.esm\" \"000014\"\nSetFunctionValue (GetFormIDString rF)", True, "00000014", "GetFormFromMod (leading zeros may be dropped)"),
    ("stringtoref.player", "ref rF\nBegin Function { }\nlet rF := StringToRef \"00000014\"\nSetFunctionValue (GetFormIDString rF)", True, "00000014", "StringToRef (from GetFormIDString's text)"),
    ("fileexists.bsa", "SetFunctionValue (FileExists \"meshes\\ammo\\10mmammo.nif\")", False, None, ""),
    ("fileexists.none", "SetFunctionValue (FileExists \"meshes\\nvforever_no_such_file.nif\")", False, "0", "FileExists"),
    ("fileexists.folder", "SetFunctionValue (FileExists \"NVSE\" 1)", False, None, ""),
    ("lnversion", "SetFunctionValue (GetLNVersion)", False, "0", "GetLNVersion (always 0)"),
    ("nvseversionfull", "SetFunctionValue (GetNVSEVersionFull)", False, None, ""),
    ("random.single", "SetFunctionValue (GetRandomInRange 5 6)", False, "5", "GetRandomInRange (min <= x < max)"),
    ("gravity.x", "SetFunctionValue (GetLocalGravity X)", False, "0", "GetLocalGravity (X and Y are 0 unless set)"),
    ("gravity.z", "SetFunctionValue (GetLocalGravity Z)", False, None, ""),
    ("debugmode.tgm", "SetFunctionValue (GetDebugModeState 0)", False, None, ""),
    ("difficulty", "SetFunctionValue (GetGameDifficulty)", False, None, ""),
    ("dayspassed.same", "SetFunctionValue ((GetGameDaysPassed) == (GetGameDaysPassed 2281 10 13))", False, "1", "GetGameDaysPassed (no arguments = since 13 October 2281)"),
]


def _form(var, fid):
    """A ref variable set to a FalloutNV.esm form (ID without the mod index)."""
    return f'let {var} := GetFormFromMod "FalloutNV.esm" "{fid}"\n'


def _with(fid, body):
    return "ref rF\nBegin Function { }\n" + _form("rF", fid) + body


SUB = "This subject has a submarine as a subsequence"
# Second set: JIP calls that take forms or strings (batch 2). The forms are vanilla
# FalloutNV.esm records whose IDs come from `fnvtool esm inventory` [observed]; editor IDs
# cannot be used at run time, so each is fetched with GetFormFromMod. Long game text
# (descriptions, objective text) is recorded by its length only. A string_var parameter
# refuses a literal string ("script did not compile"), so the regex targets go through
# sv_Construct [observed: jip_rx_trial, 2026-10-26].
FORM_PROBES = [
    ("regex.match", f'SetFunctionValue (sv_RegexMatch (sv_Construct "Whatchamacallit") "(What)(.*)")', False, "1", "Sv RegexMatch"),
    ("regex.nomatch", 'SetFunctionValue (sv_RegexMatch (sv_Construct "Whatchamacallit") "x(.*)")', False, None, ""),
    ("regex.partial", 'SetFunctionValue (sv_RegexMatch (sv_Construct "Whatchamacallit") "What")', False, None, ""),
    ("regex.replace", f'SetFunctionValue (sv_RegexReplace (sv_Construct "{SUB}") "blip!|\\b(sub)([^ ]*)")', True, "This blip! has a blip! as a blip!", "Sv RegexReplace"),
    ("regex.search", f'SetFunctionValue (ar_Size (sv_RegexSearch (sv_Construct "{SUB}") "\\b(sub)([^ ]*)"))', False, "3", "Sv RegexSearch"),
    ("regex.search0", f'SetFunctionValue ((sv_RegexSearch (sv_Construct "{SUB}") "\\b(sub)([^ ]*)")[0])', True, None, ""),
    ("regex.validate", 'SetFunctionValue (ValidateRegex "[")', False, "5", "ValidateRegex (5 = Error_Brack)"),
    ("reftostring.base", _with("104C0C", "SetFunctionValue (RefToString rF)"), True, "FalloutNV.esm:104C0C", "RefToString"),
    ("reftostring.player", _with("14", "SetFunctionValue (rF.RefToString)"), True, None, ""),
    ("stringtoref.modkey", 'ref rF\nBegin Function { }\nlet rF := StringToRef "FalloutNV.esm:104C0C"\nSetFunctionValue (GetFormIDString rF)', True, "00104C0C", "StringToRef (from RefToString's text)"),
    ("lngetname.npc", _with("104C0C", "SetFunctionValue (LNGetName rF)"), True, None, ""),
    ("lngetname.cell", _with("106185", "SetFunctionValue (LNGetName rF)"), True, "Prospector Saloon", "LNGetName"),
    ("loadscreen.texture", _with("73857", "SetFunctionValue (GetLoadScreenTexture rF)"), True, "interface\\loading\\loading_billboard05.dds", "GetLoadScreenTexture"),
    ("formdesc.perk.len", _with("31DBA", "SetFunctionValue (sv_Length (GetFormDescription rF))"), False, None, ""),
    ("formdesc.npc.len", _with("104C0C", "SetFunctionValue (sv_Length (GetFormDescription rF))"), False, None, ""),
    ("objective.len", _with("10A214", "SetFunctionValue (sv_Length (GetObjectiveText rF 30))"), False, None, ""),
    ("objective.missing.len", _with("10A214", "SetFunctionValue (sv_Length (GetObjectiveText rF 9999))"), False, None, ""),
    ("bodypart.torso", _with("10CD74", "SetFunctionValue (GetBodyPartName rF 0)"), True, None, ""),
    ("bodypart.weapon", _with("10CD74", "SetFunctionValue (GetBodyPartName rF 14)"), True, None, ""),
    ("perkentry.0", _with("31DBA", "SetFunctionValue (GetNthPerkEntryString rF 0)"), True, None, ""),
    ("muzzleflash", _with("8F20F", "SetFunctionValue (GetProjectileMuzzleFlash rF)"), True, None, ""),
    ("fontfile.2", "SetFunctionValue (GetFontFile 2)", True, None, ""),
    ("fontfile.9", "SetFunctionValue (GetFontFile 9)", True, None, ""),
    ("stringsetting", 'SetFunctionValue (GetStringSetting "sTraitMenuTitleText")', True, None, ""),
]

def _pl(body, base=None):
    """A script with rP = the player reference (and rB = a base form) set first."""
    head = "ref rP\nref rB\nBegin Function { }\n" + _form("rP", "14")
    if base:
        head += _form("rB", base)
    return head + body


def _classes(name):
    """Bit c set when AuxiliaryVariableGetAll c (classes 0-3) holds `name` as a key."""
    return " + ".join(f'(ar_HasKey (rP.AuxVarGetAll {c}) "{name}") * {1 << c}' for c in range(4))


# Third set: JIP's variable stores (auxiliary variables and RefMap arrays). These probes
# write, but only into the stores of the running session: capture_run.py loads a save
# and quits without saving, every name used is erased again in the same script, and the
# temporary-looking names start with "*". The wiki's concept pages (Auxiliary Variables,
# RefMap Array, Template:ModIndexOverride) are not in the export, so the class each name
# prefix selects is observed here, not looked up.
STORE_PROBES = [
    ("aux.float", _pl('rP.AuxVarSetFlt "*nvfF" 3.5\nlet fV := rP.AuxVarGetFlt "*nvfF"\nrP.AuxVarErase "*nvfF"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "3.5", "AuxiliaryVariableSetFloat / GetFloat"),
    ("aux.append.size", _pl('rP.AuxVarSetFlt "*nvfL" 1\nrP.AuxVarSetFlt "*nvfL" 2 -1\nrP.AuxVarSetFlt "*nvfL" 3 -1\nlet fV := rP.AuxVarSize "*nvfL"\nrP.AuxVarErase "*nvfL"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "3", "AuxiliaryVariableSetFloat (index -1 appends)"),
    ("aux.last", _pl('rP.AuxVarSetFlt "*nvfL" 1\nrP.AuxVarSetFlt "*nvfL" 2 -1\nlet fV := rP.AuxVarGetFlt "*nvfL" -1\nrP.AuxVarErase "*nvfL"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "2", "AuxiliaryVariableGetFloat (index -1 is the last element)"),
    ("aux.gap.size", _pl('rP.AuxVarSetFlt "*nvfG" 1 5\nlet fV := rP.AuxVarSize "*nvfG"\nrP.AuxVarErase "*nvfG"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.missing.float", _pl('SetFunctionValue (rP.AuxVarGetFlt "*nvfNone")'), False, None, ""),
    ("aux.missing.type", _pl('SetFunctionValue (rP.AuxVarType "*nvfNone")'), False, "0", "AuxiliaryVariableGetType (0: not defined)"),
    ("aux.missing.size", _pl('SetFunctionValue (rP.AuxVarSize "*nvfNone")'), False, "0", "AuxiliaryVariableGetSize (0 if not defined)"),
    ("aux.type.float", _pl('rP.AuxVarSetFlt "*nvfT" 1\nlet fV := rP.AuxVarType "*nvfT"\nrP.AuxVarErase "*nvfT"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "1", "AuxiliaryVariableGetType (1 float)"),
    ("aux.type.ref", _pl('rP.AuxVarSetRef "*nvfT" rP\nlet fV := rP.AuxVarType "*nvfT"\nrP.AuxVarErase "*nvfT"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "2", "AuxiliaryVariableGetType (2 reference)"),
    ("aux.type.string", _pl('rP.AuxVarSetStr "*nvfT" "abc"\nlet fV := rP.AuxVarType "*nvfT"\nrP.AuxVarErase "*nvfT"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "4", "AuxiliaryVariableGetType (4 string)"),
    ("aux.float.of.string", _pl('rP.AuxVarSetStr "*nvfT" "abc"\nlet fV := rP.AuxVarGetFlt "*nvfT"\nrP.AuxVarErase "*nvfT"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.string", _pl('rP.AuxVarSetStr "*nvfS" "abc"\nSetFunctionValue (rP.AuxVarGetStr "*nvfS")\nrP.AuxVarErase "*nvfS"'), True, "abc", "AuxiliaryVariableSetString / GetString"),
    ("aux.overwrite.type", _pl('rP.AuxVarSetFlt "*nvfT" 1\nrP.AuxVarSetStr "*nvfT" "abc"\nlet fV := rP.AuxVarType "*nvfT"\nrP.AuxVarErase "*nvfT"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.erase.returns", _pl('rP.AuxVarSetFlt "*nvfE" 1\nrP.AuxVarSetFlt "*nvfE" 2 -1\nrP.AuxVarSetFlt "*nvfE" 3 -1\nlet fV := rP.AuxVarErase "*nvfE" 0\nrP.AuxVarErase "*nvfE"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "2", "AuxiliaryVariableErase (returns the new size)"),
    ("aux.erase.shifts", _pl('rP.AuxVarSetFlt "*nvfE" 1\nrP.AuxVarSetFlt "*nvfE" 2 -1\nrP.AuxVarSetFlt "*nvfE" 3 -1\nrP.AuxVarErase "*nvfE" 0\nlet fV := rP.AuxVarGetFlt "*nvfE" 0\nrP.AuxVarErase "*nvfE"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.erase.all", _pl('rP.AuxVarSetFlt "*nvfE" 1\nrP.AuxVarSetFlt "*nvfE" 2 -1\nlet fV := rP.AuxVarErase "*nvfE"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.owner.base", _pl('AuxVarSetFlt "*nvfO" 7 0 rB\nlet fV := (AuxVarGetFlt "*nvfO" 0 rB) * 10 + (rP.AuxVarGetFlt "*nvfO")\nAuxVarErase "*nvfO" -1 rB\nSetFunctionValue fV', "104C0C").replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("aux.case", _pl('rP.AuxVarSetFlt "*nvfC" 4\nlet fV := rP.AuxVarGetFlt "*NVFc"\nrP.AuxVarErase "*nvfC"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
]
for _name in ("nvfP", "*nvfP", "_nvfP", "*_nvfP"):
    _tag = _name.replace("*", "t").replace("_", "u")
    STORE_PROBES.append((f"aux.class.{_tag}", _pl(f'rP.AuxVarSetFlt "{_name}" 1\nlet fV := {_classes(_name)}\nrP.AuxVarErase "{_name}"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""))
STORE_PROBES += [
    ("refmap.float", _pl('rP.RefMapSetFlt "*nvfM" 4.5\nlet fV := rP.RefMapGetFlt "*nvfM"\nRefMapDestroy "*nvfM"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "4.5", "RefMapArraySetFloat / GetFloat"),
    ("refmap.size", _pl('rP.RefMapSetFlt "*nvfM" 1\nRefMapSetFlt "*nvfM" 2 rB\nrP.RefMapSetFlt "*nvfM" 3\nlet fV := RefMapArrayGetSize "*nvfM"\nRefMapDestroy "*nvfM"\nSetFunctionValue fV', "104C0C").replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("refmap.type", _pl('rP.RefMapSetStr "*nvfM" "abc"\nlet fV := rP.RefMapGetType "*nvfM"\nRefMapDestroy "*nvfM"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "4", "RefMapArrayGetType (4 string)"),
    ("refmap.missing.type", _pl('SetFunctionValue (rP.RefMapGetType "*nvfNone")'), False, "0", "RefMapArrayGetType (0: no such key)"),
    ("refmap.missing.float", _pl('SetFunctionValue (rP.RefMapGetFlt "*nvfNone")'), False, "0", "RefMapArrayGetFloat (0 if not defined)"),
    ("refmap.first.key", _pl('rP.RefMapSetFlt "*nvfM" 1\nlet rB := (RefMapFirst "*nvfM")[0]\nRefMapDestroy "*nvfM"\nSetFunctionValue (GetFormIDString rB)'), True, "00000014", "RefMapArrayGetFirst (element 0 is the key)"),
    ("refmap.keys", _pl('rP.RefMapSetFlt "*nvfM" 1\nRefMapSetFlt "*nvfM" 2 rB\nlet fV := ar_Size (RefMapKeys "*nvfM")\nRefMapDestroy "*nvfM"\nSetFunctionValue fV', "104C0C").replace("ref rB\n", "ref rB\nfloat fV\n"), False, "2", "RefMapArrayGetKeys"),
    ("refmap.next.end", _pl('rP.RefMapSetFlt "*nvfM" 1\nlet fV := ar_Size (RefMapFirst "*nvfM")\nlet fV := fV * 10 + ar_Size (RefMapNext "*nvfM")\nRefMapDestroy "*nvfM"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, None, ""),
    ("refmap.destroyed", _pl('rP.RefMapSetFlt "*nvfM" 1\nRefMapDestroy "*nvfM"\nSetFunctionValue (RefMapArrayGetSize "*nvfM")'), False, None, ""),
    ("refmap.validate", _pl('rP.RefMapSetFlt "*nvfM" 1\nlet fV := RefMapValidate "*nvfM"\nRefMapDestroy "*nvfM"\nSetFunctionValue fV').replace("ref rB\n", "ref rB\nfloat fV\n"), False, "1", "RefMapArrayValidate (returns the size)"),
]

def _on_damage(params, body):
    """Register a health-damage handler on the player: a lambda with `params`."""
    return _pl(f"SetOnHealthDamageEventHandler (Begin Function {{{params}}}\n{body}\nEnd) 1 rP\nSetFunctionValue 1")


def _read(expr, text=False):
    return _pl(f"SetFunctionValue ({expr})")


# Fourth set: JIP event handlers, registered with xNVSE lambdas (a runtime-compiled
# script cannot declare a UDF form). Lambda parameters must be typed inside the braces
# (`{float fD, ref rS}`); parameters declared before Begin do not compile [observed:
# jip_ev_trial2, 2026-10-26]; `this` does not compile in a runtime-compiled lambda, GetSelf
# does [observed: jip_ev_trial3]. The steps depend on each other and run in order: register
# every handler, cause one event (DamageAV Health 1 on the player), read what the
# handlers stored in `*` auxiliary variables, undo. Writes stay in the session: the run
# never saves, the handlers die with the session, the last step restores the Health
# point and erases the variables.
EVENT_PROBES = [
    ("ev.reg.amount", _on_damage("float fD, ref rS", '  rP.AuxVarSetFlt "*nvfHD" fD\n  rP.AuxVarSetRef "*nvfHS" rS\n  rP.AuxVarSetFlt "*nvfHT" (GetSelf == rP)'), False, "1", "SetOnHealthDamageEventHandler (registers)"),
    ("ev.reg.order1", _on_damage("float fD, ref rS", '  rP.AuxVarSetFlt "*nvfHO" 1 -1'), False, "1", "SetOnHealthDamageEventHandler"),
    ("ev.reg.order2", _on_damage("float fD, ref rS", '  rP.AuxVarSetFlt "*nvfHO" 2 -1'), False, "1", "SetOnHealthDamageEventHandler"),
    ("ev.reg.noparams", _on_damage(" ", '  rP.AuxVarSetFlt "*nvfHN" 1'), False, "1", "SetOnHealthDamageEventHandler"),
    ("ev.reg.oneparam", _on_damage("float fD", '  rP.AuxVarSetFlt "*nvfH1" fD'), False, "1", "SetOnHealthDamageEventHandler"),
    ("loop.reg", _pl('SetGameMainLoopCallback (Begin Function { }\n  rP.AuxVarSetFlt "*nvfLC" ((rP.AuxVarGetFlt "*nvfLC") + 1)\n  rP.AuxVarSetFlt "*nvfLT" (GetSelf == rP)\nEnd) 1 1 1\nSetFunctionValue 1'), False, "1", "SetGameMainLoopCallback"),
    ("ev.damage", _pl("rP.DamageAV Health 1\nSetFunctionValue 1"), False, "1", "DamageAV"),
    ("ev.amount", _read('rP.AuxVarGetFlt "*nvfHD"'), False, "-1", "SetOnHealthDamageEventHandler (the damage is negative)"),
    ("ev.source.type", _read('rP.AuxVarType "*nvfHS"'), False, None, ""),
    ("ev.this", _read('rP.AuxVarGetFlt "*nvfHT"'), False, "1", "SetOnHealthDamageEventHandler (the actor is the calling reference)"),
    ("ev.order.size", _read('rP.AuxVarSize "*nvfHO"'), False, None, ""),
    ("ev.order.first", _read('rP.AuxVarGetFlt "*nvfHO" 0'), False, None, ""),
    ("ev.noparams.ran", _read('rP.AuxVarGetFlt "*nvfHN"'), False, None, ""),
    ("ev.oneparam.ran", _read('rP.AuxVarGetFlt "*nvfH1"'), False, None, ""),
    ("loop.ran", _read('(rP.AuxVarGetFlt "*nvfLC") > 0'), False, "1", "SetGameMainLoopCallback (runs every frame)"),
    ("loop.this", _read('rP.AuxVarGetFlt "*nvfLT"'), False, "1", "SetGameMainLoopCallback (on the player when not called on a reference)"),
    ("ev.cleanup", _pl('rP.RestoreAV Health 1\n' + "".join(f'rP.AuxVarErase "*nvf{n}"\n' for n in ("HD", "HS", "HT", "HO", "HN", "H1", "LC", "LT")) + "SetFunctionValue 1"), False, "1", "cleanup"),
]

# Fifth set: JIP's hit information, read inside SetOnHitEventHandler lambdas (OnHit
# passes no arguments, so the lambdas take none). One handler per value appends it to a
# `*` auxiliary variable on every firing; then a frag mine explosion (MineFragExplosion,
# FalloutNV.esm 07078B [observed: fnvtool esm inventory]) is placed at the player, whose
# Health is first raised by 1000 so the blast cannot kill. Reads give each firing's
# value (index 0 = first firing). The last step takes the 1000 back, heals and erases.
HIT_VALUES = [
    ("count", "1"),
    ("damage", "rP.GetHitHealthDamage"),
    ("location", "rP.GetHitLocation"),
    ("flag.explosion", "rP.GetHitExtendedFlag 8"),
    ("flag.critical", "rP.GetHitExtendedFlag 1"),
    ("projectile.isref", "IsReference (rP.GetHitProjectile)"),
    ("projectile.type", "GetType (rP.GetHitProjectile)"),
    ("attacker.isref", "IsReference (rP.GetHitAttacker)"),
    ("weapon.valid", "IsFormValid (rP.GetHitWeapon)"),
    ("pos.x", "(rP.GetHit3DData 0)[0]"),
    ("health", "rP.GetAV Health"),
]
# Expected values the wiki states for an explosion hit [documented: the page named].
HIT_EXPECTED = {
    "hit.location.0": ("-1", "GetHitLocation (always -1 for explosion hits)"),
    "hit.location.1": ("-1", "GetHitLocation (always -1 for explosion hits)"),
    "hit.flag.explosion.1": ("1", "GetHitExtendedFlag (8: explosion hit)"),
    "hit.pos.x.0": ("0", "GetHit3DData (all 0 for explosion hits)"),
    "hit.health.0": ("1200", "SetOnHitEventHandler (runs before the damage)"),
}


def _hit_steps():
    steps = [("hit.boost", _pl("rP.ModAV Health 1000\nSetFunctionValue (rP.GetAV Health)"), False, None, "")]
    for name, expr in HIT_VALUES:
        steps.append((f"hit.reg.{name}", _pl(f'SetOnHitEventHandler (Begin Function {{ }}\n  rP.AuxVarSetFlt "*nvfH.{name}" ({expr}) -1\nEnd) 1 rP\nSetFunctionValue 1'), False, "1", "SetOnHitEventHandler (registers)"))
    steps.append(("hit.boom", _pl("rP.PlaceAtMe rB 1\nSetFunctionValue 1", "7078B"), False, "1", "PlaceAtMe"))
    steps.append(("hit.settle", "SetFunctionValue 1", False, "1", "a step for the blast to land"))
    for name, _ in HIT_VALUES:
        steps.append((f"hit.{name}.size", _read(f'rP.AuxVarSize "*nvfH.{name}"'), False, None, ""))
        for i in range(2):
            exp, page = HIT_EXPECTED.get(f"hit.{name}.{i}", (None, ""))
            steps.append((f"hit.{name}.{i}", _read(f'rP.AuxVarGetFlt "*nvfH.{name}" {i}'), False, exp, page))
    steps.append(("hit.health.after", _read("rP.GetAV Health"), False, None, ""))
    steps.append(("hit.cleanup", _pl("rP.ModAV Health -1000\nrP.RestoreAV Health 2000\n" + "".join(f'rP.AuxVarErase "*nvfH.{n}"\n' for n, _ in HIT_VALUES) + "SetFunctionValue 1"), False, "1", "cleanup"))
    return steps


HIT_PROBES = _hit_steps()

# Sixth set: JIP getters of record fields (weapons, ammo, projectiles, explosions,
# armor), on the forms the wiki pages use as examples; IDs from fnvtool esm inventory
# [observed]. NVForever reads the same fields from the records, so these pin which field
# each trait number means. Read-only.
REC = {
    "slug.proj": "1572C5", "ammo12": "8ECF5", "slug.ammo": "13E447", "gren.expl": "7EA31",
    "gren.proj": "7EA30", "laser": "4336", "hunting": "4333", "sledge": "4352", "rev44": "8F215",
    "pistol10": "434F", "carbine": "8F21E", "gatling": "432E", "metal": "3307F", "leather": "20423",
    "ammo10": "4241", "proj10": "2CD5F", "t45d": "4443E",
}
REC_EXPECTED = {
    "killimpulse.laser": ("0", "GetWeaponKillImpulse (WeapLaserRifle: 0)"),
    "killimpulse.hunting": ("26", "GetWeaponKillImpulse (WeapHuntingRifle: 26)"),
    "impulsedist.laser": ("0", "GetWeaponImpulseDistance (WeapLaserRifle: 0)"),
    "vatseffect.sledge": ("0010F09F", "GetWeaponVATSEffect (WeapSuperSledge: MaulerKnockdownSpell)"),
}


def _rec(pid, key, expr, text=False):
    exp, page = REC_EXPECTED.get(pid, (None, ""))
    return (pid, _with(REC[key], f"SetFunctionValue ({expr})"), text, exp, page)


RECORD_PROBES = (
    [_rec(f"projtrait.slug.{i}", "slug.proj", f"GetProjectileTraitNumeric rF {i}") for i in range(15)]
    + [_rec(f"projtrait.gren.{i}", "gren.proj", f"GetProjectileTraitNumeric rF {i}") for i in (0, 1, 2, 3, 5, 6)]
    + [_rec(f"projflag.slug.{i}", "slug.proj", f"GetProjectileFlag rF {i}") for i in range(12)]
    + [_rec(f"projflag.gren.{i}", "gren.proj", f"GetProjectileFlag rF {i}") for i in (0, 1, 10)]
    + [_rec("projexpl.gren", "gren.proj", "GetFormIDString (GetProjectileExplosion rF)", True),
       _rec("projexpl.slug", "slug.proj", "GetFormIDString (GetProjectileExplosion rF)", True),
       _rec("projlight.slug", "slug.proj", "GetFormIDString (GetProjectileLight rF)", True)]
    + [_rec(f"expltrait.gren.{i}", "gren.expl", f"GetExplosionTraitNumeric rF {i}") for i in range(8)]
    + [_rec(f"explflag.gren.{i}", "gren.expl", f"GetExplosionFlag rF {i}") for i in range(1, 7)]
    + [_rec(f"expltraitform.gren.{i}", "gren.expl", f"GetFormIDString (GetExplosionTraitForm rF {i})", True) for i in range(7)]
    + [_rec(f"ammotrait.ammo12.{i}", "ammo12", f"GetAmmoTraitNumeric rF {i}") for i in range(4)]
    + [_rec(f"ammotrait.ammo10.{i}", "ammo10", f"GetAmmoTraitNumeric rF {i}") for i in range(4)]
    + [_rec("ammoproj.slug", "slug.ammo", "GetFormIDString (GetAmmoProjectile rF)", True),
       _rec("ammoproj.ammo10", "ammo10", "GetFormIDString (GetAmmoProjectile rF)", True)]
    + [_rec("killimpulse.laser", "laser", "GetWeaponKillImpulse rF"),
       _rec("killimpulse.hunting", "hunting", "GetWeaponKillImpulse rF"),
       _rec("impulsedist.laser", "laser", "GetWeaponImpulseDistance rF"),
       _rec("impulsedist.hunting", "hunting", "GetWeaponImpulseDistance rF"),
       _rec("critflags.laser", "laser", "GetWeaponCritFlags rF"),
       _rec("critflags.hunting", "hunting", "GetWeaponCritFlags rF"),
       _rec("vatseffect.sledge", "sledge", "GetFormIDString (GetWeaponVATSEffect rF)", True),
       _rec("vatseffect.laser", "laser", "GetFormIDString (GetWeaponVATSEffect rF)", True),
       _rec("onhit.rev44", "rev44", "GetWeaponOnHitSetting rF"),
       _rec("semiauto.rev44.0", "rev44", "GetWeaponSemiAutoFireDelay rF 0"),
       _rec("semiauto.rev44.1", "rev44", "GetWeaponSemiAutoFireDelay rF 1"),
       _rec("impactset.pistol10", "pistol10", "GetFormIDString (GetImpactDataSet rF)", True),
       _rec("model.carbine.0", "carbine", "GetWeaponModel rF 0", True),
       _rec("model.gatling.3", "gatling", "GetWeaponModel rF 3", True),
       _rec("model.pistol10.7", "pistol10", "GetWeaponModel rF 7", True),
       _rec("armorclass.metal", "metal", "GetArmorClass rF"),
       _rec("armorclass.leather", "leather", "GetArmorClass rF"),
       _rec("armorrct.metal", "metal", "GetArmorRCT rF", True),
       _rec("armorrct.t45d", "t45d", "GetArmorRCT rF", True)]
)

# Seventh set: core NVSE/FOSE form and reference getters (not JIP: asked for by the
# extender core work to check its VM), on vanilla FalloutNV.esm forms; IDs from fnvtool
# esm inventory [observed]. Read-only. Reference rows read the loaded save's state (Doc
# Mitchell's cell), so they can differ in a run that starts from no save.
NF = {
    "pistol10": "434F", "hunting": "4333", "ammo10": "4241", "leather": "20423", "caps": "F",
    "stimpak": "15169", "doc": "104C0C", "docref": "104C0F", "player": "14", "playerbase": "7",
    "saloon": "106185", "vcg02": "10A214", "actionboy": "31DBA", "slugproj": "1572C5",
    "list127": "1613D5",
}


def _nf(pid, key, expr, text=False):
    return (pid, _with(NF[key], f"SetFunctionValue ({expr})"), text, None, "")


def _nfid(pid, key, expr):
    return _nf(pid, key, f"GetFormIDString ({expr})", True)


_WEAPON_GETTERS = ["GetWeaponClipRounds", "GetAttackDamage", "GetWeaponType", "GetWeaponSkill",
                   "GetWeaponMinRange", "GetWeaponMaxRange", "GetWeaponFireRate", "GetWeaponSpread",
                   "GetWeaponMinSpread", "GetWeaponAnimJamTime", "GetWeaponHandGrip", "GetWeaponNumProjectiles",
                   "GetWeaponCritChance", "GetWeaponCritDamage", "GetWeaponActionPoints", "GetWeaponReach",
                   "GetWeaponSightFOV", "GetWeaponRequiredStrength", "GetWeaponRequiredSkill",
                   "GetWeaponResistType", "GetWeaponAttackAnimation", "GetWeaponAimArc", "GetWeaponLongBursts",
                   "GetWeaponFlags1", "GetWeaponFlags2", "GetWeaponBaseVATSChance"]

NVSE_FORM_PROBES = (
    [_nf(f"type.{k}", k, "GetType rF") for k in ("pistol10", "ammo10", "leather", "caps", "stimpak", "doc",
                                                 "docref", "player", "saloon", "vcg02", "actionboy", "slugproj", "list127")]
    + [_nfid("base.docref", "docref", "rF.GetBaseObject"),
       _nfid("base.player", "player", "rF.GetBaseObject"),
       _nf("isref.docref", "docref", "IsReference rF"),
       _nf("isref.doc", "doc", "IsReference rF"),
       _nf("valid.pistol10", "pistol10", "IsFormValid rF"),
       _nfid("cell.docref", "docref", "rF.GetParentCell"),
       _nf("formid.pistol10", "pistol10", "GetFormIDString rF", True),
       _nf("name.pistol10", "pistol10", "GetName rF", True),
       _nf("name.caps", "caps", "GetName rF", True),
       _nf("name.saloon", "saloon", "GetName rF", True),
       _nf("name.docref", "docref", "GetName rF", True),
       _nf("name.vcg02", "vcg02", "GetName rF", True)]
    + [_nf(f"weight.{k}", k, "GetWeight rF") for k in ("pistol10", "leather", "caps", "stimpak", "ammo10")]
    + [_nf(f"value.{k}", k, "GetItemValue rF") for k in ("pistol10", "leather", "caps", "stimpak", "ammo10")]
    + [_nf(f"health.{k}", k, "GetBaseHealth rF") for k in ("pistol10", "leather")]
    + [_nf(f"equiptype.{k}", k, "GetEquipType rF") for k in ("pistol10", "leather", "stimpak", "ammo10")]
    + [_nfid("repairlist.pistol10", "pistol10", "GetRepairList rF"),
       _nf("model.pistol10", "pistol10", "GetModelPath rF", True),
       _nf("icon.pistol10", "pistol10", "GetIconPath rF", True),
       _nf("ar.leather", "leather", "GetArmorArmorRating rF"),
       _nf("dt.leather", "leather", "GetArmorDT rF"),
       _nfid("ammo.pistol10", "pistol10", "GetWeaponAmmo rF"),
       _nfid("projectile.pistol10", "pistol10", "GetWeaponProjectile rF"),
       _nfid("criteffect.pistol10", "pistol10", "GetWeaponCritEffect rF")]
    + [_nf(f"weap.{g[9:].lower() if g.startswith('GetWeapon') else g[3:].lower()}.{k}", k, f"{g} rF")
       for g in _WEAPON_GETTERS for k in ("pistol10", "hunting")]
    + [_nf("list.count", "list127", "ListGetCount rF"),
       _nfid("list.nth0", "list127", "ListGetNthForm rF 0"),
       _nf("list.index", "list127", "ListGetFormIndex rF (ListGetNthForm rF 0)"),
       _nf("modindex.fnv", "pistol10", 'GetModIndex "FalloutNV.esm"'),
       _nf("sourcemod.pistol10", "pistol10", "GetSourceModIndex rF"),
       _nf("nthmod.0", "pistol10", "GetNthModName 0", True),
       _nf("scripted.pistol10", "pistol10", "IsScripted rF"),
       _nfid("script.doc", "doc", "GetScript rF"),
       _nfid("race.doc", "doc", "GetRace rF")]
)

# Eighth set: NVSE's variables by name (HasVariable, GetVariable, SetVariable,
# GetRefVariable, SetRefVariable). The quest is ExtProbeLoops06Q of the owner's test
# plugin NVFExtProbe.esp (loaded in the game and in the lab; its script declares
# `int iR`, `ref rForm`, `ref rList`, `int iDone`, tools/capture/ext-probes/loops/06),
# and the vanilla VCG02 (VCG02SCRIPT: nTargetCount, nGeckosKilled, bCheyenneAlive,
# bShootingTutorialActive) and Doc Mitchell (GSDocMitchellScript: DoOnce,
# BeginMedicalServicesBAD, Timer, PlayerLeft) [observed: fnvtool esm decode]. Sets write
# quest variables of the session only (the run never saves); each probe puts back what
# it changed where the old value can be read.
def _q(body):
    return ("ref rQ\nref rV\nref rD\nref rP\nfloat fO\nfloat fV\nBegin Function { }\n"
            'let rQ := GetFormFromMod "NVFExtProbe.esp" "B40"\n'
            + _form("rV", "10A214") + _form("rD", "104C0F") + _form("rP", "14") + body)


VAR_PROBES = [
    ("has.int", _q('SetFunctionValue (HasVariable "iDone" rQ)'), False, "1", "HasVariable"),
    ("has.ref", _q('SetFunctionValue (HasVariable "rList" rQ)'), False, "1", "HasVariable (any variable type)"),
    ("has.case", _q('SetFunctionValue (HasVariable "IDONE" rQ)'), False, None, ""),
    ("has.none", _q('SetFunctionValue (HasVariable "nvfNone" rQ)'), False, "0", "HasVariable"),
    ("has.vanilla", _q('SetFunctionValue (HasVariable "nGeckosKilled" rV)'), False, "1", "HasVariable"),
    ("has.ref.calling", _q('SetFunctionValue (rD.HasVariable "PlayerLeft")'), False, "1", "HasVariable (calling reference)"),
    ("has.base", _q('SetFunctionValue (HasVariable "PlayerLeft" (rD.GetBaseObject))'), False, None, ""),
    ("has.noscript", _q('SetFunctionValue (HasVariable "iDone" rP)'), False, None, ""),
    ("set.get", _q('let fO := GetVariable "iR" rQ\nSetVariable "iR" 7 rQ\nlet fV := GetVariable "iR" rQ\nSetVariable "iR" fO rQ\nSetFunctionValue fV'), False, "7", "SetVariable / GetVariable"),
    ("set.fraction", _q('let fO := GetVariable "iR" rQ\nSetVariable "iR" 2.75 rQ\nlet fV := GetVariable "iR" rQ\nSetVariable "iR" fO rQ\nSetFunctionValue fV'), False, None, ""),
    ("set.vanilla", _q('let fO := GetVariable "bShootingTutorialActive" rV\nSetVariable "bShootingTutorialActive" 5 rV\nlet fV := GetVariable "bShootingTutorialActive" rV\nSetVariable "bShootingTutorialActive" fO rV\nSetFunctionValue fV'), False, "5", "SetVariable / GetVariable"),
    ("set.float.calling", _q('let fO := rD.GetVariable "Timer"\nrD.SetVariable "Timer" 1.5\nlet fV := rD.GetVariable "Timer"\nrD.SetVariable "Timer" fO\nSetFunctionValue fV'), False, "1.5", "SetVariable / GetVariable (calling reference)"),
    ("get.missing", _q('SetFunctionValue (GetVariable "nvfNone" rQ)'), False, None, ""),
    ("get.refvar.as.number", _q('SetRefVariable "rForm" rP rQ\nSetFunctionValue (GetVariable "rForm" rQ)'), False, None, ""),
    ("ref.set.get", _q('SetRefVariable "rForm" rP rQ\nSetFunctionValue (GetFormIDString (GetRefVariable "rForm" rQ))'), True, "00000014", "SetRefVariable / GetRefVariable"),
    ("ref.get.intvar", _q('SetFunctionValue (GetFormIDString (GetRefVariable "iDone" rQ))'), True, None, ""),
    ("ref.get.missing", _q('SetFunctionValue (GetFormIDString (GetRefVariable "nvfNone" rQ))'), True, None, ""),
    ("ref.set.form", _q('SetRefVariable "rForm" rV rQ\nSetFunctionValue (GetFormIDString (GetRefVariable "rForm" rQ))'), True, "0010A214", "SetRefVariable (any form)"),
]

# Ninth set: reference walking in named cells (GetNumRefsInCell, GetFirstRefInCell),
# not around the player, so it does not depend on where the save stands. Doc Mitchell's
# house and the Prospector Saloon are not loaded in the owner's save (it stands at the
# airport), where only persistent references are seen [documented: GetFirstRefInCell].
# Form type codes 0 any, 200 actors, 201 items, 42 NPC_ [documented: GetRefsEtc pages;
# Form Type IDs]. Read-only.
CELLS = {"doc": "103DF9", "saloon": "106185"}
REF_PROBES = (
    [(f"num.{c}.{t}", _with(fid, f"SetFunctionValue (GetNumRefsInCell rF {t} 0 0)"), False, None, "")
     for c, fid in CELLS.items() for t in (0, 200, 201, 42)]
    + [(f"num.{c}.taken", _with(fid, "SetFunctionValue (GetNumRefsInCell rF 0 0 1)"), False, None, "")
       for c, fid in CELLS.items()]
    + [(f"first.{c}.200", _with(fid, "SetFunctionValue (GetFormIDString (GetFirstRefInCell rF 200 0 0))"), True, None, "")
       for c, fid in CELLS.items()]
)

# Tenth set: control bindings (GetControl code type, GetAltControl code) for the 28
# control codes, keyboard (type 0) and mouse (type 1), plus one code past the range.
# They read the owner's key bindings. Read-only.
CONTROL_PROBES = (
    [(f"control.{c}.{t}", f"SetFunctionValue (GetControl {c} {t})", False, None, "") for c in range(29) for t in (0, 1)]
    + [(f"control.{c}.default", f"SetFunctionValue (GetControl {c})", False, None, "") for c in (0, 4, 27)]
    + [(f"alt.{c}", f"SetFunctionValue (GetAltControl {c})", False, None, "") for c in range(29)]
)

# Eleventh set: doors, factions and NPC appearance on vanilla forms (Doc Mitchell's
# house door, Doc, Sunny Smiles, Trudy; NCR and Goodsprings factions); IDs from fnvtool
# esm inventory [observed]. Read-only.
AF = {"door": "103E61", "doc": "104C0C", "sunny": "104E84", "trudy": "104C6C",
      "ncr": "A46E7", "goodsprings": "104C6E", "bos": "1D3FE"}


def _af(pid, key, expr, text=False):
    return (pid, _with(AF[key], f"SetFunctionValue ({expr})"), text, None, "")


ACTOR_PROBES = (
    [_af("door.linked", "door", "GetFormIDString (rF.GetLinkedDoor)", True),
     _af("door.teleportcell", "door", "GetFormIDString (rF.GetTeleportCell)", True),
     _af("door.isload", "door", "rF.IsLoadDoor")]
    + [x for k in ("doc", "sunny", "trudy") for x in (
        [_af(f"factions.{k}", k, "GetBaseNumFactions rF")]
        + [_af(f"faction.{k}.{i}", k, f"GetFormIDString (GetBaseNthFaction {i} rF)", True) for i in range(3)]
        + [_af(f"rank.{k}.{i}", k, f"GetBaseNthRank {i} rF") for i in range(3)]
        + [_af(f"hair.{k}", k, "GetFormIDString (GetHair rF)", True),
           _af(f"eyes.{k}", k, "GetFormIDString (GetEyes rF)", True),
           _af(f"hairlength.{k}", k, "GetHairLength rF"),
           _af(f"haircolor.{k}.0", k, "GetHairColor 0 rF"),
           _af(f"haircolor.{k}.1", k, "GetHairColor 1 rF"),
           _af(f"haircolor.{k}.2", k, "GetHairColor 2 rF"),
           _af(f"haircolor.{k}.3", k, "GetHairColor 3 rF"),
           _af(f"height.{k}", k, "GetNPCHeight rF"),
           _af(f"weight.{k}", k, "GetNPCWeight rF"),
           _af(f"flagslow.{k}", k, "GetActorBaseFlagsLow rF"),
           _af(f"flagshigh.{k}", k, "GetActorBaseFlagsHigh rF")])]
    + [_af(f"ranks.{k}", k, "GetNumRanks rF") for k in ("ncr", "goodsprings", "bos")]
    + [_af(f"ranknames.{k}.size", k, "ar_Size (GetFactionRankNames rF)") for k in ("ncr", "goodsprings", "bos")]
    + [_af("ranknames.bos.0", "bos", "(GetFactionRankNames rF)[0]", True)]
)

# Twelfth set: base-form setters read back in the same session (SetWeaponClipRounds,
# SetWeight, SetName, SetBaseItemValue, SetObjectHealth): whether the change shows through
# the getter, and through a second form of access (a reference's base form, LNGetName).
# Each probe puts the old value back. Session only: the run never saves.
def _set(pid, key, body, text=False, exp=None, page=""):
    return (pid, ("ref rF\nref rD\nfloat fO\nfloat fV\nBegin Function { }\n" + _form("rF", NF[key])
                  + _form("rD", NF["docref"]) + body), text, exp, page)


SETTER_PROBES = [
    _set("clip.set.get", "pistol10", "let fO := GetWeaponClipRounds rF\nSetWeaponClipRounds 30 rF\nlet fV := GetWeaponClipRounds rF\nSetWeaponClipRounds fO rF\nSetFunctionValue fV", exp="30", page="SetWeaponClipRounds"),
    _set("clip.restored", "pistol10", "SetFunctionValue (GetWeaponClipRounds rF)"),
    _set("weight.set.get", "caps", "let fO := GetWeight rF\nSetWeight 0.5 rF\nlet fV := GetWeight rF\nSetWeight fO rF\nSetFunctionValue fV", exp="0.5", page="SetWeight"),
    _set("value.set.get", "pistol10", "let fO := GetItemValue rF\nSetBaseItemValue rF 999\nlet fV := GetItemValue rF\nSetBaseItemValue rF fO\nSetFunctionValue fV"),
    _set("health.set.get", "pistol10", "let fO := GetBaseHealth rF\nSetObjectHealth 123 rF\nlet fV := GetBaseHealth rF\nSetObjectHealth fO rF\nSetFunctionValue fV"),
    _set("name.set.get", "pistol10", 'SetName "NVF Pistol" rF\nSetFunctionValue (GetName rF)\nSetName "10mm Pistol" rF', text=True, exp="NVF Pistol", page="SetName"),
    _set("name.set.lngetname", "pistol10", 'SetName "NVF Pistol" rF\nSetFunctionValue (LNGetName rF)\nSetName "10mm Pistol" rF', text=True),
    _set("name.restored", "pistol10", "SetFunctionValue (GetName rF)", text=True),
    _set("name.npc.via.ref", "doc", 'SetName "NVF Doc" rF\nSetFunctionValue (GetName rD)\nSetName "Doc Mitchell" rF', text=True),
    _set("name.npc.ref.lngetname", "doc", 'SetName "NVF Doc" rF\nSetFunctionValue (rD.LNGetName)\nSetName "Doc Mitchell" rF', text=True),
]

# Thirteenth set: packages, quest objectives, ownership, model and texture paths and the
# raw form flags, on vanilla forms. Read-only. Ids ending in `.save` read state of the
# loaded save (Doc Mitchell's current package, equipment, the player's perks), so a run
# without the owner's save may differ there.
def _m(pid, body, text=False):
    return (pid, "ref rD\nref rF\nref rP\nref rG\nBegin Function { }\n" + _form("rD", NF["docref"])
            + _form("rP", NF["player"]) + body, text, None, "")


def _mf(pid, key, expr, text=False):
    return _m(pid, _form("rF", NF[key]) + f"SetFunctionValue ({expr})", text)


MISC_PROBES = (
    [_m("pkg.count", "SetFunctionValue (rD.GetPackageCount)"),
     _m("pkg.nth0", "SetFunctionValue (GetFormIDString (rD.GetNthPackage 0))", True),
     _m("pkg.nth1", "SetFunctionValue (GetFormIDString (rD.GetNthPackage 1))", True),
     _m("pkg.nth0.location", "let rG := rD.GetNthPackage 0\nSetFunctionValue (GetFormIDString (GetPackageLocation rG))", True),
     _m("pkg.nth0.distance", "let rG := rD.GetNthPackage 0\nSetFunctionValue (GetPackageTargetDistance rG)"),
     _m("pkg.current.save", "SetFunctionValue (GetFormIDString (rD.GetCurrentPackage))", True),
     _mf("obj.count", "vcg02", "GetQuestObjectiveCount rF")]
    + [_mf(f"obj.nth{i}", "vcg02", f"GetNthQuestObjective rF {i}") for i in range(4)]
    + [_m("owner.cell.docref", "SetFunctionValue (GetFormIDString (rD.GetParentCellOwner))", True),
       _m("owner.rank.docref", "SetFunctionValue (rD.GetOwningFactionRequiredRank)"),
       _m("equip.health.doc.save", "SetFunctionValue (rD.GetEquippedCurrentHealth 5)"),
       _m("equip.modflags.doc.save", "SetFunctionValue (rD.GetEquippedWeaponModFlags)"),
       _mf("perk.player.save", "actionboy", "rP.GetPerkRank rF"),
       _mf("altperk.player.save", "actionboy", "rP.GetAltPerkRank rF")]
    + [_mf(f"biped.leather.{c}", "leather", f"GetBipedModelPath {c} rF", True) for c in range(4)]
    + [_mf(f"texture.{k}", k, "GetTexturePath rF", True) for k in ("pistol10", "stimpak", "actionboy", "caps")]
    + [_mf(f"flags.{k}.{w}", k, f"GetFlags{w.capitalize()} rF") for k in ("pistol10", "caps", "doc", "leather", "vcg02")
       for w in ("low", "high")]
)

# Fourteenth set: quaternion and vector functions, the legacy ones (results written to
# variables named by strings) and their 6.4.8 "Ex" forms (variables passed directly).
# Each probe returns the result components as one text, "%.4f" each, in the order the
# function writes them (W X Y Z, or X Y Z). Read-only.
_V4 = "float fQw\nfloat fQx\nfloat fQy\nfloat fQz\nBegin Function { }\n"
_OUT4 = 'SetFunctionValue (sv_Construct "%.4f %.4f %.4f %.4f" fQw fQx fQy fQz)'
_OUT3 = 'SetFunctionValue (sv_Construct "%.4f %.4f %.4f" fQx fQy fQz)'


def _q4(pid, call):
    return (pid, _V4 + call + "\n" + _OUT4, True, None, "")


def _q3(pid, call):
    return (pid, _V4 + call + "\n" + _OUT3, True, None, "")


_N4 = '"fQw" "fQx" "fQy" "fQz"'
_N3 = '"fQx" "fQy" "fQz"'
MATH_PROBES = [
    _q4("qfromeuler.x90", f"QFromEuler {_N4} 90 0 0"),
    _q4("qfromeuler.y90", f"QFromEuler {_N4} 0 90 0"),
    _q4("qfromeuler.z90", f"QFromEuler {_N4} 0 0 90"),
    _q4("qfromeuler.xyz", f"QFromEuler {_N4} 30 45 60"),
    _q4("qfromeuler.xyz.pitchyaw", f"QFromEuler {_N4} 30 45 60 1"),
    _q4("qfromeulerex.xyz", "QFromEulerEx fQw fQx fQy fQz 30 45 60"),
    _q3("qtoeuler.z90", f"QToEuler {_N3} 0.7071 0 0 0.7071"),
    _q3("qtoeuler.xyz", f"QToEuler {_N3} 0.8224 0.0222 0.4396 0.3604"),
    _q3("qtoeulerex.z90", "QToEulerEx fQx fQy fQz 0.7071 0 0 0.7071"),
    _q4("qmult.qq", f"QMultQuatQuat {_N4} 0.7071 0.7071 0 0 0.7071 0 0 0.7071"),
    _q4("qmultex.qq", "QMultQuatQuatEx fQw fQx fQy fQz 0.7071 0.7071 0 0 0.7071 0 0 0.7071"),
    _q3("qmult.qv", f"QMultQuatVector3 {_N3} 0.7071 0 0 0.7071 1 0 0"),
    _q3("qmultex.qv", "QMultQuatVector3Ex fQx fQy fQz 0.7071 0 0 0.7071 1 0 0"),
    _q4("qnorm", f"QNormalize {_N4} 2 0 0 2"),
    _q4("qnormex", "QNormalizeEx fQw fQx fQy fQz 2 0 0 2"),
    _q4("qinterp.half", f"QInterpolate {_N4} 1 0 0 0 0.7071 0 0 0.7071 0.5"),
    _q4("qinterp.half.sli", f"QInterpolate {_N4} 1 0 0 0 0.7071 0 0 0.7071 0.5 1"),
    _q4("qinterpex.half", "QInterpolateEx fQw fQx fQy fQz 1 0 0 0 0.7071 0 0 0.7071 0.5"),
    _q4("qfromaa.z90", f"QFromAxisAngle {_N4} 0 0 1 90"),
    _q4("qfromaaex.z90", "QFromAxisAngleEx fQw fQx fQy fQz 0 0 1 90"),
    _q3("v3norm", f"V3Normalize {_N3} 3 0 4"),
    _q3("v3normex", "V3NormalizeEx fQx fQy fQz 3 0 4"),
    _q3("v3cross.xy", f"V3Crossproduct {_N3} 1 0 0 0 1 0"),
    _q3("v3crossex.xy", "V3CrossproductEx fQx fQy fQz 1 0 0 0 1 0"),
]

# Fifteenth set: mod local data (Set/Get/Exists/Remove/GetAllModLocalData) with a
# number, a string and a form, and the keys of GetUserTime (values change, so only the
# shape is probed). Mod local data belongs to the calling script's mod; a
# runtime-compiled script belongs to none, whatever that means for the store. Every key
# set is removed in the same probe. Session only.
def _ml(pid, body, text=False, exp=None, page=""):
    return (pid, "ref rF\nfloat fRes\nBegin Function { }\n" + _form("rF", NF["pistol10"]) + body, text, exp, page)


DATA_PROBES = [
    _ml("mld.number", 'SetModLocalData "nvfNum" 5.5\nlet fRes := GetModLocalData "nvfNum"\nRemoveModLocalData "nvfNum"\nSetFunctionValue fRes', exp="5.5", page="SetModLocalData / GetModLocalData"),
    _ml("mld.string", 'SetModLocalData "nvfStr" "abc"\nSetFunctionValue (GetModLocalData "nvfStr")\nRemoveModLocalData "nvfStr"', True, "abc", "SetModLocalData (a string)"),
    _ml("mld.form", 'SetModLocalData "nvfForm" rF\nSetFunctionValue (GetFormIDString (GetModLocalData "nvfForm"))\nRemoveModLocalData "nvfForm"', True, "0000434F", "SetModLocalData (a form)"),
    _ml("mld.exists", 'SetModLocalData "nvfNum" 1\nlet fRes := ModLocalDataExists "nvfNum"\nRemoveModLocalData "nvfNum"\nSetFunctionValue fRes', exp="1", page="ModLocalDataExists"),
    _ml("mld.exists.none", 'SetFunctionValue (ModLocalDataExists "nvfNone")', exp="0", page="ModLocalDataExists"),
    _ml("mld.get.none", 'SetFunctionValue (GetModLocalData "nvfNone")', exp="0", page="GetModLocalData (0 when no data)"),
    _ml("mld.case", 'SetModLocalData "nvfCase" 7\nlet fRes := GetModLocalData "NVFCASE"\nRemoveModLocalData "nvfCase"\nSetFunctionValue fRes'),
    _ml("mld.remove.existed", 'SetModLocalData "nvfNum" 1\nSetFunctionValue (RemoveModLocalData "nvfNum")', exp="1", page="RemoveModLocalData"),
    _ml("mld.remove.none", 'SetFunctionValue (RemoveModLocalData "nvfNone")', exp="0", page="RemoveModLocalData (0 if none existed)"),
    _ml("mld.all.size", 'SetModLocalData "nvfA" 1\nSetModLocalData "nvfB" 2\nlet fRes := ar_Size (GetAllModLocalData)\nRemoveModLocalData "nvfA"\nRemoveModLocalData "nvfB"\nSetFunctionValue fRes'),
    _ml("mld.all.haskey", 'SetModLocalData "nvfA" 1\nlet fRes := ar_HasKey (GetAllModLocalData) "nvfA"\nRemoveModLocalData "nvfA"\nSetFunctionValue fRes'),
    _ml("usertime.size", "SetFunctionValue (ar_Size (GetUserTime))"),
] + [_ml(f"usertime.has.{k}", f'SetFunctionValue (ar_HasKey (GetUserTime) "{k}")', exp="1", page="GetUserTime")
     for k in ("Year", "Month", "DayOfWeek", "Day", "Hour", "Minute", "Second", "Millisecond")]

# Sixteenth set: inventory references and clones. A 10mm pistol is added to the player,
# equipped, looked at through foreach (inventory references), unequipped and removed,
# one step each, in order; clones are made with TempCloneForm (never saved). Session
# only. The player may own 10mm pistols in the save, so counts are compared before and
# after the add.
def _inv(pid, body, text=False, exp=None, page=""):
    return (pid, "ref rPlayer\nref rGun\nref rItem\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"])
            + _form("rGun", NF["pistol10"]) + body, text, exp, page)


def _each(expr):
    """fRes := expr for the pistol's inventory reference, computed inside foreach: after a
    container foreach the loop variable is no longer a valid form, and calling a function
    on it ends the function without a value; a break alone is fine [observed:
    foreach.break.alone, foreach.read.after(.valid), foreach.break.then.read]."""
    return ("let fRes := -1\nforeach rItem <- rPlayer\n  if rItem.GetBaseObject == rGun\n"
            f"    let fRes := {expr}\n  endif\nloop\nSetFunctionValue fRes")
INV_PROBES = [
    # Ordered steps (the game runs one per loop; an added or equipped item may only show
    # up a loop later): count, add, look, equip, look, unequip, look, remove.
    _inv("inv.count.before", "SetFunctionValue (ar_Size (rPlayer.GetInvRefsForItem rGun))"),
    _inv("inv.add", "rPlayer.AddItem rGun 1 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _inv("inv.count.after", "SetFunctionValue (ar_Size (rPlayer.GetInvRefsForItem rGun))"),
    _inv("invref.found", _each("rItem.GetBaseObject == rGun"), exp="1", page="foreach over a container"),
    _inv("invref.foreach", _each("rItem.IsInventoryRef"), exp="1", page="IsInventoryRef"),
    # Which part ended the function without a value: a break out of a container foreach,
    # reading the loop variable after the loop, or both.
    _inv("foreach.break.alone", "foreach rItem <- rPlayer\n  if rItem.GetBaseObject == rGun\n    break\n  endif\nloop\nSetFunctionValue 1"),
    _inv("foreach.read.after", "foreach rItem <- rPlayer\nloop\nSetFunctionValue (rItem.IsInventoryRef)"),
    _inv("foreach.read.after.valid", "foreach rItem <- rPlayer\nloop\nSetFunctionValue (IsFormValid rItem)"),
    _inv("foreach.break.then.read", "foreach rItem <- rPlayer\n  if rItem.GetBaseObject == rGun\n    break\n  endif\nloop\nSetFunctionValue (rItem.IsInventoryRef)"),
    _inv("inv.equip", "rPlayer.EquipItem rGun 0 1\nSetFunctionValue 1", exp="1", page="EquipItem"),
    _inv("equipped.after.equip", _each("rItem.IsEquipped")),
    _inv("getequipped.after.equip", "SetFunctionValue (rPlayer.GetEquipped rGun)", exp="1", page="GetEquipped"),
    _inv("inv.unequip", "rPlayer.UnequipItem rGun 0 1\nSetFunctionValue 1", exp="1", page="UnequipItem"),
    _inv("equipped.not", _each("rItem.IsEquipped")),
    _inv("inv.remove", "rPlayer.RemoveItem rGun 1 1\nSetFunctionValue 1", exp="1", page="RemoveItem"),
    _inv("clone.iscloned", "let rItem := TempCloneForm rGun\nSetFunctionValue (IsClonedForm rItem)", exp="1", page="IsClonedForm"),
    _inv("clone.original", "SetFunctionValue (IsClonedForm rGun)", exp="0", page="IsClonedForm"),
    _inv("clone.name", "let rItem := TempCloneForm rGun\nSetFunctionValue (GetName rItem)", True),
    _inv("clone.modindex", "let rItem := TempCloneForm rGun\nSetFunctionValue (GetSourceModIndex rItem)"),
    _inv("clone.type", "let rItem := TempCloneForm rGun\nSetFunctionValue (GetType rItem)"),
    _inv("clone.clip", "let rItem := TempCloneForm rGun\nSetFunctionValue (GetWeaponClipRounds rItem)"),
    _inv("clone.indexed.iscloned", "let rItem := TempCloneForm rGun 1\nSetFunctionValue (IsClonedForm rItem)"),
    _inv("clone.indexed.modindex", "let rItem := TempCloneForm rGun 1\nSetFunctionValue (GetSourceModIndex rItem)"),
]

# Seventeenth set: the extender core's second-pass families. Weapon mods, faction
# arrays, race lists, ownership and key names are read-only; the perk, item-walk and
# disabled-key rows change the session only (each is undone in a later step) and run in
# order. Ids ending in `.save` read state of the owner's save. Forms from fnvtool esm
# inventory and world refs [observed]: WeapNVVarmintRifle 07EA24, Caucasian 000019,
# Toughness 031DE0, Doc's dresser 103E37 (persistent), Sunny Smiles' reference 104E85 (in the
# Prospector Saloon).
SF = dict(NF, varmint="7EA24", sledge="4352", caucasian="19", toughness="31DE0", dresser="103E37",
          door="103E61", sunnyref="104E85", sunny="104E84")


def _s2(pid, body, text=False, exp=None, page=""):
    return (pid, "ref rPlayer\nref rObj\nref rOther\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"])
            + body, text, exp, page)


def _s2f(pid, key, expr, text=False, exp=None, page=""):
    return _s2(pid, _form("rObj", SF[key]) + f"SetFunctionValue ({expr})", text, exp, page)


_WALK = ("let fRes := 0\nlet rOther := rPlayer.GetFirstRefForItem rObj\nwhile IsFormValid rOther\n"
         "  let fRes += 1\n  let rOther := rPlayer.GetNextRefForItem rObj\nloop\nSetFunctionValue fRes")


SECOND_PROBES = (
    [_s2f(f"mod.{w}.{i}", w, f"GetFormIDString (GetWeaponItemMod {i} rObj)", True) for w in ("pistol10", "varmint", "sledge") for i in (1, 2, 3)]
    + [_s2f(f"modeffect.{w}.{i}", w, f"GetWeaponItemModEffect {i} rObj") for w in ("pistol10", "varmint", "sledge") for i in (1, 2, 3)]
    + [_s2f(f"modvalue{v}.{w}.{i}", w, f"GetWeaponItemModValue{v} {i} rObj") for v in (1, 2) for w in ("pistol10", "varmint") for i in (1, 2, 3)]
    + [x for k in ("doc", "sunny") for x in (
        [_s2f(f"basefactions.{k}.size", k, "ar_Size (GetBaseFactions rObj)")]
        + [_s2f(f"basefactions.{k}.{i}", k, f"GetFormIDString ((GetBaseFactions rObj)[{i}])", True) for i in range(3)]
        + [_s2f(f"baseranks.{k}.size", k, "ar_Size (GetBaseRanks rObj)"),
           _s2f(f"baseranks.{k}.0", k, "(GetBaseRanks rObj)[0]")])]
    + [_s2f("activefactions.sunnyref.size.save", "sunnyref", "ar_Size (rObj.GetActiveFactions)"),
       _s2f("activefactions.sunnyref.0.save", "sunnyref", "GetFormIDString ((rObj.GetActiveFactions)[0])", True),
       _s2f("activeranks.sunnyref.0.save", "sunnyref", "(rObj.GetActiveRanks)[0]"),
       _s2("activefactions.player.size.save", "SetFunctionValue (ar_Size (rPlayer.GetActiveFactions))")]
    + [_s2f("racename.doc", "doc", "GetRaceName rObj", True),
       _s2f("racename.sunny", "sunny", "GetRaceName rObj", True),
       _s2f("racehairs.size", "caucasian", "ar_Size (GetRaceHairs rObj)"),
       _s2f("racehairs.0", "caucasian", "GetFormIDString ((GetRaceHairs rObj)[0])", True),
       _s2f("raceeyes.size", "caucasian", "ar_Size (GetRaceEyes rObj)"),
       _s2f("raceeyes.0", "caucasian", "GetFormIDString ((GetRaceEyes rObj)[0])", True)]
    + [_s2(f"owned.{t}.{o}", _form("rObj", SF[t]) + _form("rOther", SF[o]) + "SetFunctionValue (rObj.IsOwned rOther)")
       for t in ("dresser", "door") for o in ("doc", "docref", "player")]
    + [_s2(f"hasownership.{o}.{t}", _form("rObj", SF[t]) + _form("rOther", SF[o]) + "SetFunctionValue (rOther.HasOwnership rObj)")
       for t in ("dresser", "door") for o in ("docref", "player")]
    + [_s2(f"keyname.{k}", f"SetFunctionValue (GetKeyName {k})", True) for k in (17, 57, 256, 1, 0)]
    + [_s2(f"iscontrol.{k}", f"SetFunctionValue (IsControl {k})") for k in (17, 99, 57)]
    + [_s2("pressedkeys.size", "SetFunctionValue (ar_Size (GetPressedKeys))"),
       _s2("disabledkeys.before", "SetFunctionValue (ar_Size (GetDisabledKeys))"),
       _s2("disablekey.34", "DisableKey 34\nSetFunctionValue 1"),
       _s2("disabledkeys.after", "SetFunctionValue (ar_Size (GetDisabledKeys))"),
       _s2("disabledkeys.after.0", "SetFunctionValue ((GetDisabledKeys)[0])"),
       _s2("enablekey.34", "EnableKey 34\nSetFunctionValue 1"),
       _s2("disabledkeys.end", "SetFunctionValue (ar_Size (GetDisabledKeys))")]
    + [_s2f("perk.before.save", "toughness", "rPlayer.GetPerkRank rObj"),
       _s2f("altperk.before.save", "toughness", "rPlayer.GetAltPerkRank rObj"),
       _s2("perk.add", _form("rObj", SF["toughness"]) + "rPlayer.AddPerk rObj\nSetFunctionValue 1", exp="1", page="AddPerk"),
       _s2f("perk.after.add", "toughness", "rPlayer.GetPerkRank rObj"),
       _s2("perk.remove", _form("rObj", SF["toughness"]) + "rPlayer.RemovePerk rObj\nSetFunctionValue 1", exp="1", page="RemovePerk"),
       _s2f("perk.after.remove", "toughness", "rPlayer.GetPerkRank rObj")]
    + [_s2f("items.before.save", "stimpak", "ar_Size (rPlayer.GetInvRefsForItem rObj)"),
       _s2("items.add", _form("rObj", SF["stimpak"]) + "rPlayer.AddItem rObj 3 1\nSetFunctionValue 1", exp="1", page="AddItem"),
       _s2("items.walk", _form("rObj", SF["stimpak"]) + _WALK),
       _s2f("items.invrefs", "stimpak", "ar_Size (rPlayer.GetInvRefsForItem rObj)"),
       _s2("items.remove", _form("rObj", SF["stimpak"]) + "rPlayer.RemoveItem rObj 3 1\nSetFunctionValue 1", exp="1", page="RemoveItem"),
       _s2("items.walk.pistol.save", _form("rObj", SF["pistol10"]) + _WALK)]
)

# Eighteenth set: GetKeyName for every code 0-265 (keyboard scancodes, then the mouse
# buttons from 256), one row each. Read-only.
KEYNAME_PROBES = [(f"key.{k}", f"SetFunctionValue (GetKeyName {k})", True, None, "") for k in range(266)]

# Nineteenth set: base packages and spells, equipment masks, object effects, class
# names, swimming and the dialogue getters (no conversation is running). The hunting
# rifle's record carries an EITM (162C8F) and the 10mm pistol none [observed: fnvtool esm
# decode]. Read-only.
SF.update(hunting="4333", metal="3307F", t45d="4443E")
THIRD_PROBES = (
    [_s2f(f"basepackages.{k}.size", k, "ar_Size (GetBasePackages rObj)") for k in ("doc", "sunny")]
    + [_s2f(f"basepackages.{k}.0", k, "GetFormIDString ((GetBasePackages rObj)[0])", True) for k in ("doc", "sunny")]
    + [_s2f(f"basespells.{k}.size", k, "ar_Size (GetBaseSpellListSpells rObj)") for k in ("doc", "sunny")]
    + [_s2f(f"basespells.{k}.0", k, "GetFormIDString ((GetBaseSpellListSpells rObj)[0])", True) for k in ("doc", "sunny")]
    + [_s2f(f"slotsmask.{k}", k, "GetEquipmentSlotsMask rObj") for k in ("leather", "metal", "t45d", "pistol10")]
    + [_s2f(f"bipedmask.{k}", k, "GetEquipmentBipedMask rObj") for k in ("leather", "metal", "t45d")]
    + [_s2f(f"objecteffect.{k}", k, "GetFormIDString (GetObjectEffect rObj)", True) for k in ("hunting", "pistol10", "t45d")]
    + [_s2f(f"classname.{k}", k, "GetNameOfClass rObj", True) for k in ("doc", "sunny", "docref")]
    + [_s2("swimming", "SetFunctionValue (IsPlayerSwimming)"),
       _s2("dialogue.target", "SetFunctionValue (GetFormIDString (rPlayer.GetDialogueTarget))", True),
       _s2("dialogue.subject", "SetFunctionValue (GetFormIDString (rPlayer.GetDialogueSubject))", True),
       _s2("dialogue.speaker", "SetFunctionValue (GetFormIDString (rPlayer.GetDialogueSpeaker))", True)]
)

# Twentieth set: the extender core's "other" group. Steps run in order; the inventory
# rows add a 10mm pistol, 20 rounds of 10mm and 3 stimpaks to the player, equip the
# pistol, move stimpaks to Doc Mitchell's reference, and the last steps take all of it
# back (session only; the run never saves). Forms from fnvtool esm inventory [observed]:
# Ammo556mm 004240, AmmoList10mm 1537E6 (the pistol's NAM0), the leveled character
# VarVHDNCRRanger9Veteran 17A2FE, scripts GSDocMitchellScript 107230 and VCG02SCRIPT
# 10A1F0; ExtProbeLoops06Q of NVFExtProbe.esp holds `int iR`. Ids ending in `.save` read
# state of the owner's save or of the start position.
FX = dict(SF, ammo556="4240", ammolist10="1537E6", lvln="17A2FE", docscript="107230", vcg02script="10A1F0",
          ammo10="4241", cell="DEB16")
_H4 = ("ref rPlayer\nref rObj\nref rOther\nref rDoc\nref rItem\nfloat fRes\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]))


def _f4(pid, body, text=False, exp=None, page=""):
    return (pid, _H4 + body, text, exp, page)


def _o4(key, other=None):
    out = _form("rObj", FX[key])
    return out + (_form("rOther", FX[other]) if other else "")


def _in(base, expr):
    """fRes := expr for the player's inventory reference of `base` (rObj), in the loop."""
    return (_form("rObj", FX[base]) + "let fRes := -1\nforeach rItem <- rPlayer\n  if rItem.GetBaseObject == rObj\n"
            f"    let fRes := {expr}\n  endif\nloop\nSetFunctionValue fRes")


FOURTH_PROBES = [
    # 1. BuildRef (decimal FormIDs; hex literals do not compile at run time) and local indexes.
    _f4("buildref.player", "SetFunctionValue (GetFormIDString (BuildRef 0 20))", True),
    _f4("buildref.doc", "SetFunctionValue (GetFormIDString (BuildRef 0 1068044))", True),
    _f4("buildref.hex", "SetFunctionValue (GetFormIDString (BuildRef 0 0x104C0C))", True),
    _f4("localrefindex.docref", "SetFunctionValue (rDoc.GetLocalRefIndex)"),
    _f4("localrefindex.player", "SetFunctionValue (rPlayer.GetLocalRefIndex)"),
    # 2. Inside a runtime-compiled query.
    _f4("selfalt", "SetFunctionValue (GetFormIDString (GetSelfAlt))", True),
    _f4("currentscript.valid", "SetFunctionValue (IsFormValid (GetCurrentScript))"),
    _f4("currentscript.type", "SetFunctionValue (GetType (GetCurrentScript))"),
    _f4("currentscript.modindex", "SetFunctionValue (GetSourceModIndex (GetCurrentScript))"),
    # 3. Grids (the candidate starts in cell 000DEB16, where the save stands).
    _f4("gridstoload", "SetFunctionValue (GetGridsToLoad)"),
    _f4("ingrid.doc.0", "SetFunctionValue (GetInGrid rDoc 0)"),
    _f4("ingrid.doc.1", "SetFunctionValue (GetInGrid rDoc 1)"),
    _f4("ingridincell.player.0", _o4("cell") + "SetFunctionValue (GetInGridInCell rObj rPlayer 0)"),
    # 9 (read-only, before the inventory steps). Actors and leveled lists.
    _f4("ageclass.doc", _o4("doc") + "SetFunctionValue (GetAgeClass rObj)"),
    _f4("ageclass.sunny", _o4("sunny") + "SetFunctionValue (GetAgeClass rObj)"),
    _f4("headparts.doc.size", _o4("doc") + "SetFunctionValue (ar_Size (GetHeadParts rObj))"),
    _f4("headparts.doc.0", _o4("doc") + "SetFunctionValue (GetFormIDString ((GetHeadParts rObj)[0]))", True),
    _f4("levchar.size", _o4("lvln") + "SetFunctionValue (ar_Size (GetLevCharacterRefs rObj))"),
    _f4("levchar.0", _o4("lvln") + "SetFunctionValue (GetFormIDString ((GetLevCharacterRefs rObj)[0]))", True),
    _f4("headingx.player.doc.save", "SetFunctionValue (rPlayer.GetHeadingAngleX rDoc)"),
    # 10. Versions, script comparison, ResetAllVariables.
    _f4("fose.version", "SetFunctionValue (GetFOSEVersion)"),
    _f4("fose.revision", "SetFunctionValue (GetFOSERevision)"),
    _f4("fose.beta", "SetFunctionValue (GetFOSEBeta)"),
    _f4("comparescripts.same", _o4("docscript", "docscript") + "SetFunctionValue (CompareScripts rObj rOther)"),
    _f4("comparescripts.diff", _o4("docscript", "vcg02script") + "SetFunctionValue (CompareScripts rObj rOther)"),
    _f4("resetallvars.quest", 'let rObj := GetFormFromMod "NVFExtProbe.esp" "B40"\nSetVariable "iR" 7 rObj\n'
        'rObj.ResetAllVariables\nSetFunctionValue (GetVariable "iR" rObj)'),
    _f4("resetallvars.after", 'let rObj := GetFormFromMod "NVFExtProbe.esp" "B40"\nSetFunctionValue (GetVariable "iR" rObj)'),
    # 6. Base-form setters read back, then put back.
    _f4("playable.before", _o4("pistol10") + "SetFunctionValue (IsPlayable rObj)"),
    _f4("playable.set0", _o4("pistol10") + "SetIsPlayable 0 rObj\nlet fRes := IsPlayable rObj\nSetIsPlayable 1 rObj\nSetFunctionValue fRes"),
    _f4("questitem.before", _o4("pistol10") + "SetFunctionValue (IsQuestItem rObj)"),
    _f4("questitem.set1", _o4("pistol10") + "SetQuestItem 1 rObj\nlet fRes := IsQuestItem rObj\nSetQuestItem 0 rObj\nSetFunctionValue fRes"),
    _f4("questitem.after", _o4("pistol10") + "SetFunctionValue (IsQuestItem rObj)"),
    # 4. Ammo, with the pistol and 20 rounds added and the pistol equipped.
    _f4("inv.add.pistol", _o4("pistol10") + "rPlayer.AddItem rObj 1 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _f4("inv.add.ammo", _o4("ammo10") + "rPlayer.AddItem rObj 20 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _f4("inv.equip.pistol", _o4("pistol10") + "rPlayer.EquipItem rObj 0 1\nSetFunctionValue 1", exp="1", page="EquipItem"),
    # GetWeaponCanUseAmmo takes the weapon as the calling reference and the ammo as its one
    # argument [documented: GetWeaponCanUseAmmo]. Called with two form arguments
    # (`GetWeaponCanUseAmmo rWeapon rAmmo`) it crashed the game (2026-10-27, first try of
    # this set) [observed], so only the documented form is probed, on the pistol's
    # inventory reference.
    _f4("ammo.canuse.invref.10", _form("rOther", FX["ammo10"]) + _in("pistol10", "rItem.GetWeaponCanUseAmmo rOther")),
    _f4("ammo.canuse.invref.556", _form("rOther", FX["ammo556"]) + _in("pistol10", "rItem.GetWeaponCanUseAmmo rOther")),
    _f4("ammo.hasequipped.10", _o4("ammo10") + "SetFunctionValue (rPlayer.HasAmmoEquipped rObj)"),
    _f4("ammo.hasequipped.556", _o4("ammo556") + "SetFunctionValue (rPlayer.HasAmmoEquipped rObj)"),
    _f4("ammo.equippedcanuse.10", _o4("ammo10") + "SetFunctionValue (rPlayer.GetEquippedWeaponCanUseAmmo rObj)"),
    _f4("ammo.equippedcanuse.556", _o4("ammo556") + "SetFunctionValue (rPlayer.GetEquippedWeaponCanUseAmmo rObj)"),
    _f4("ammo.inlist", _o4("ammolist10") + "SetFunctionValue (rPlayer.IsEquippedAmmoInList rObj)"),
    _f4("ammo.usesammolist", _o4("ammolist10") + "SetFunctionValue (rPlayer.GetEquippedWeaponUsesAmmoList rObj)"),
    _f4("ammo.current", "SetFunctionValue (GetFormIDString (GetPlayerCurrentAmmo))", True),
    _f4("ammo.rounds", "SetFunctionValue (GetPlayerCurrentAmmoRounds)"),
    # 5. Condition of the equipped pistol and of its inventory reference.
    _f4("health.equipped.5", "SetFunctionValue (rPlayer.GetEquippedCurrentHealth 5)"),
    _f4("health.invref", _in("pistol10", "rItem.GetCurrentHealth")),
    _f4("health.set50", _in("pistol10", "rItem.SetCurrentHealth 50") .replace("let fRes := rItem.SetCurrentHealth 50", "rItem.SetCurrentHealth 50\n    let fRes := 1")),
    _f4("health.invref.after", _in("pistol10", "rItem.GetCurrentHealth")),
    _f4("health.equipped.5.after", "SetFunctionValue (rPlayer.GetEquippedCurrentHealth 5)"),
    # 7. Inventory-reference actions on a stack of 3 stimpaks, and on the pistol.
    _f4("ir.add.stimpak", _o4("stimpak") + "rPlayer.AddItem rObj 3 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _f4("ir.count.player.0.save", _o4("stimpak") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _f4("ir.count.doc.0", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    _f4("ir.refcount", _in("stimpak", "rItem.GetRefCount")),
    _f4("ir.copyir", _in("stimpak", "rItem.CopyIR rDoc")),
    _f4("ir.count.player.1.save", _o4("stimpak") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _f4("ir.count.doc.1", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    _f4("ir.copyiralt.valid", _in("stimpak", "IsFormValid (rItem.CopyIRAlt rDoc)")),
    _f4("ir.count.doc.2", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    _f4("ir.removemeir.none", _in("stimpak", "rItem.RemoveMeIR")),
    _f4("ir.count.player.2.save", _o4("stimpak") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _f4("ir.readd.stimpak", _o4("stimpak") + "rPlayer.AddItem rObj 3 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _f4("ir.removemeir.doc", _in("stimpak", "rItem.RemoveMeIR rDoc")),
    _f4("ir.count.player.3.save", _o4("stimpak") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _f4("ir.count.doc.3", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    _f4("ir.unequipme", _in("pistol10", "rItem.UnequipMe") .replace("let fRes := rItem.UnequipMe", "rItem.UnequipMe\n    let fRes := 1")),
    _f4("ir.isequipped.after.unequipme", _in("pistol10", "rItem.IsEquipped")),
    _f4("ir.equipme", _in("pistol10", "rItem.EquipMe") .replace("let fRes := rItem.EquipMe", "rItem.EquipMe\n    let fRes := 1")),
    _f4("ir.isequipped.after.equipme", _in("pistol10", "rItem.IsEquipped")),
    _f4("ir.unequip.again", _o4("pistol10") + "rPlayer.UnequipItem rObj 0 1\nSetFunctionValue 1"),
    _f4("ir.equipitem2", _o4("pistol10") + "rPlayer.EquipItem2 rObj 0 1\nSetFunctionValue 1"),
    _f4("ir.getequipped.after.equipitem2", _o4("pistol10") + "SetFunctionValue (rPlayer.GetEquipped rObj)"),
    # 8. CreateTempRef, kept in a temporary aux variable for the next frame's read.
    _f4("tempref.valid", _o4("stimpak") + 'let rItem := CreateTempRef rObj\nrPlayer.AuxVarSetRef "*nvfTR" rItem\nSetFunctionValue (IsFormValid rItem)'),
    _f4("tempref.base", _o4("stimpak") + "let rItem := CreateTempRef rObj\nSetFunctionValue (GetFormIDString (rItem.GetBaseObject))", True),
    # `rItem.GetRefCount` on a CreateTempRef reference crashed the game (2026-10-27)
    # [observed]: not probed.
    _f4("tempref.isinvref", _o4("stimpak") + "let rItem := CreateTempRef rObj\nSetFunctionValue (rItem.IsInventoryRef)"),
    _f4("tempref.next.frame.valid", 'let rItem := rPlayer.AuxVarGetRef "*nvfTR"\nSetFunctionValue (IsFormValid rItem)'),
    # Clean-up: everything added comes back off.
    _f4("cleanup.player", _o4("pistol10", "ammo10") + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\n"
        "rPlayer.RemoveItem rOther 20 1\n" + _form("rObj", FX["stimpak"])
        + 'rPlayer.RemoveItem rObj 3 1\nrPlayer.AuxVarErase "*nvfTR"\nSetFunctionValue 1', exp="1", page="cleanup"),
    _f4("cleanup.doc", _o4("stimpak") + "let fRes := rDoc.GetItemCount rObj\nrDoc.RemoveItem rObj fRes 1\nSetFunctionValue (rDoc.GetItemCount rObj)", exp="0", page="cleanup"),
]

# Twenty-first set: the functions the owner's mod list uses most that NVForever lacks
# (docs/handoff/modlist-scan-2026-10-26.md): JIP's AddItemAlt, RemoveItemTarget,
# EquipItemAlt, MessageExAlt, IsSpellTargetAlt, IsInAir, StopIdle, GetGameDaysPassed,
# SetGameHour, GetWeaponRefModFlags, GetPerkModifier, LU's GetDistance3D and
# JohnnyGuitar's EditorIDToFormID, GetEditorID and Clamp (JohnnyGuitar 528 is loaded in
# the owner's game). Steps in order; items added are taken back at the end, and
# SetGameHour only runs when TimeScale (global 00003A) is 30, since the page says it can
# crash otherwise, and the hour is put back from a `*` aux variable. Session only.
FX.update(gamehour="38", timescale="3A", chetref="104C79", dochair="2A185")
FF = FX
FIFTH_PROBES = [
    # JohnnyGuitar.
    _f4("jg.edid.to.form", 'SetFunctionValue (GetFormIDString (EditorIDToFormID "Weap10mmPistol"))', True),
    _f4("jg.edid.to.form.case", 'SetFunctionValue (GetFormIDString (EditorIDToFormID "weap10mmpistol"))', True),
    _f4("jg.edid.to.form.ref", 'SetFunctionValue (GetFormIDString (EditorIDToFormID "DocMitchellREF"))', True),
    _f4("jg.edid.to.form.none", 'SetFunctionValue (GetFormIDString (EditorIDToFormID "nvfNoSuchEditorID"))', True),
    _f4("jg.getedid.pistol", _o4("pistol10") + "SetFunctionValue (GetEditorID rObj)", True),
    _f4("jg.getedid.docref", "SetFunctionValue (GetEditorID rDoc)", True),
    _f4("jg.getedid.player", "SetFunctionValue (GetEditorID rPlayer)", True),
    _f4("jg.clamp.high", "SetFunctionValue (Clamp 5 0 3)"),
    _f4("jg.clamp.low", "SetFunctionValue (Clamp -1 0 3)"),
    _f4("jg.clamp.inside", "SetFunctionValue (Clamp 2.5 0 3)"),
    # LU / JIP read-only.
    _f4("distance3d.door.dresser", _o4("door", "dresser") + "SetFunctionValue (rObj.GetDistance3D rOther)"),
    _f4("distance3d.doc.door.save", _o4("door") + "SetFunctionValue (rDoc.GetDistance3D rObj)"),
    _f4("inair.player", "SetFunctionValue (rPlayer.IsInAir)"),
    _f4("spelltargetalt.stimpak.save", _o4("stimpak") + "SetFunctionValue (rPlayer.IsSpellTargetAlt rObj)"),
    _f4("perkmodifier.0.save", "SetFunctionValue (rPlayer.GetPerkModifier 0 10)"),
    _f4("dayspassed.noargs.save", "SetFunctionValue (GetGameDaysPassed)"),
    _f4("dayspassed.since.2281.10.13.save", "SetFunctionValue (GetGameDaysPassed 2281 10 13)"),
    _f4("dayspassed.same.again", "SetFunctionValue ((GetGameDaysPassed) == (GetGameDaysPassed 2281 10 13))"),
    _f4("globals.timescale", _form("rObj", FF["timescale"]) + "SetFunctionValue (GetGlobalValue rObj)"),
    _f4("stopidle.player", "rPlayer.StopIdle\nSetFunctionValue 1"),
    _f4("messageexalt", 'MessageExAlt 1 "NVForever probe"\nSetFunctionValue 1'),
    # SetGameHour, guarded by TimeScale 30.
    _f4("gamehour.set", _form("rObj", FF["timescale"]) + _form("rOther", FF["gamehour"])
        + "let fRes := -1\nif (GetGlobalValue rObj) == 30\n  let fRes := GetGlobalValue rOther\n"
        '  rPlayer.AuxVarSetFlt "*nvfGH" fRes\n  if fRes < 22\n    SetGameHour (fRes + 1)\n  endif\nendif\nSetFunctionValue (fRes >= 0)'),
    _f4("gamehour.after.minus.before", _form("rOther", FF["gamehour"])
        + 'let fRes := (GetGlobalValue rOther) - (rPlayer.AuxVarGetFlt "*nvfGH")\n'
        'SetFunctionValue ((fRes > 0.99) && (fRes < 1.01))'),
    _f4("gamehour.restore", _form("rObj", FF["timescale"])
        + 'let fRes := rPlayer.AuxVarGetFlt "*nvfGH"\nrPlayer.AuxVarSetFlt "*nvfGD" (GetGameDaysPassed)\n'
        'if (GetGlobalValue rObj) == 30\n  if fRes < 22\n    SetGameHour fRes\n  endif\nendif\n'
        'rPlayer.AuxVarErase "*nvfGH"\nSetFunctionValue 1'),
    # Setting an earlier hour: does the clock go back, or on to that hour the next day?
    _f4("gamehour.restore.hours.passed", 'let fRes := ((GetGameDaysPassed) - (rPlayer.AuxVarGetFlt "*nvfGD")) * 24\n'
        'rPlayer.AuxVarErase "*nvfGD"\nSetFunctionValue (sv_Construct "%.0f" fRes)', True),
    # The owner's Amata companion mod (AMFAmata.esp): its unbuilt functions, read-only or
    # changed and put back in the same step, on Doc Mitchell, Sunny Smiles and Chet.
    # MessageBoxExAlt is left out: it opens a message box, which would hold the run.
    _f4("amata.sethair", _o4("sunny", "dochair") + "let rItem := GetHair rObj\nSetHair rOther rObj\n"
        "let fRes := (GetHair rObj) == rOther\nSetHair rItem rObj\nSetFunctionValue fRes"),
    _f4("amata.sethair.restored", _o4("sunny") + "SetFunctionValue (GetFormIDString (GetHair rObj))", True),
    _f4("amata.sethaircolor", _o4("sunny") + "let fRes := GetHairColor 0 rObj\nrPlayer.AuxVarSetFlt \"*nvfHC\" fRes\n"
        "SetHairColor 1193046 rObj\nSetFunctionValue (GetHairColor 0 rObj)"),
    _f4("amata.sethaircolor.parts", _o4("sunny") + 'SetFunctionValue (sv_Construct "%.0f %.0f %.0f" (GetHairColor 1 rObj) (GetHairColor 2 rObj) (GetHairColor 3 rObj))', True),
    _f4("amata.sethaircolor.restore", _o4("sunny") + 'SetHairColor (rPlayer.AuxVarGetFlt "*nvfHC") rObj\nrPlayer.AuxVarErase "*nvfHC"\nSetFunctionValue (GetHairColor 0 rObj)'),
    _f4("amata.modbaseav", "let fRes := rDoc.GetBaseAV Strength\nrDoc.ModBaseActorValue Strength 2\n"
        "let fRes := (rDoc.GetBaseAV Strength) - fRes\nrDoc.ModBaseActorValue Strength -2\nSetFunctionValue fRes"),
    _f4("amata.modbaseav.after", "SetFunctionValue (rDoc.GetBaseAV Strength)"),
    _f4("amata.effecttrait.get", _o4("stimpak") + "SetFunctionValue (GetNthEffectTraitNumeric rObj 0 0)"),
    _f4("amata.effecttrait.get.duration", _o4("stimpak") + "SetFunctionValue (GetNthEffectTraitNumeric rObj 0 2)"),
    _f4("amata.effecttrait.set", _o4("stimpak") + "let fRes := GetNthEffectTraitNumeric rObj 0 0\nSetNthEffectTraitNumeric rObj 0 0 (fRes + 5)\n"
        "let fRes := (GetNthEffectTraitNumeric rObj 0 0) - fRes\nSetNthEffectTraitNumeric rObj 0 0 ((GetNthEffectTraitNumeric rObj 0 0) - 5)\nSetFunctionValue fRes"),
    _f4("amata.effecttrait.after", _o4("stimpak") + "SetFunctionValue (GetNthEffectTraitNumeric rObj 0 0)"),
    _f4("amata.valuealt.stimpak", _o4("stimpak") + "SetFunctionValue (GetValueAlt rObj)"),
    _f4("amata.valuealt.pistol", _o4("pistol10") + "SetFunctionValue (GetValueAlt rObj)"),
    _f4("amata.merchant.chet", _o4("chetref") + "SetFunctionValue (GetFormIDString (rObj.GetMerchantContainer))", True),
    _f4("amata.merchant.doc", "SetFunctionValue (GetFormIDString (rDoc.GetMerchantContainer))", True),
    _f4("amata.isfleeing.doc", "SetFunctionValue (rDoc.IsFleeing)"),
    _f4("amata.allitemrefs.doc.size.save", "SetFunctionValue (ar_Size (rDoc.GetAllItemRefs))"),
    _f4("amata.allitemrefs.player.size.save", "SetFunctionValue (ar_Size (rPlayer.GetAllItemRefs))"),
    _f4("amata.calcdamage.pistol.save", _o4("pistol10") + "SetFunctionValue (rPlayer.GetCalculatedWeaponDamage rObj)"),
    _f4("amata.crosshairex.save", "SetFunctionValue (GetFormIDString (GetCrosshairRefEx))", True),
    _f4("amata.teammatekillable", "SetFunctionValue (rDoc.ToggleTeammateKillable 1)"),
    _f4("amata.teammatekillable.off", "SetFunctionValue (rDoc.ToggleTeammateKillable 0)"),
    _f4("amata.wheeldisabled", "rDoc.SetWheelDisabled 1\nrDoc.SetWheelDisabled 0\nSetFunctionValue 1"),
    _f4("amata.update3d", "rDoc.Update3D\nSetFunctionValue 1"),
    _f4("amata.nohealthreset", "ToggleNoHealthReset 1 0.5\nToggleNoHealthReset 0 0.5\nSetFunctionValue 1"),
    # Items: AddItemAlt with a condition and equip, mod flags, EquipItemAlt, RemoveItemTarget.
    _f4("additemalt.stimpak", _o4("stimpak") + "rPlayer.AddItemAlt rObj 2 1 0 1\nSetFunctionValue 1", exp="1", page="AddItemAlt"),
    _f4("additemalt.pistol", _o4("pistol10") + "rPlayer.AddItemAlt rObj 1 0.5 1 1\nSetFunctionValue 1", exp="1", page="AddItemAlt"),
    _f4("additemalt.pistol.equipped", _o4("pistol10") + "SetFunctionValue (rPlayer.GetEquipped rObj)"),
    _f4("additemalt.pistol.health", _in("pistol10", "rItem.GetCurrentHealth")),
    _f4("modflags.invref", _in("pistol10", "rItem.GetWeaponRefModFlags")),
    _f4("modflags.equipped", "SetFunctionValue (rPlayer.GetEquippedWeaponModFlags)"),
    _f4("equipitemalt.unequip", _o4("pistol10") + "rPlayer.UnequipItem rObj 0 1\nSetFunctionValue 1"),
    _f4("equipitemalt", _o4("pistol10") + "rPlayer.EquipItemAlt rObj 0 1\nSetFunctionValue 1"),
    _f4("equipitemalt.equipped", _o4("pistol10") + "SetFunctionValue (rPlayer.GetEquipped rObj)"),
    _f4("amata.currentammo", "SetFunctionValue (GetFormIDString (rPlayer.GetCurrentAmmo))", True),
    _f4("amata.itemrefhealth.0", _in("pistol10", "rItem.GetItemRefCurrentHealth 0")),
    _f4("amata.itemrefhealth.1", _in("pistol10", "rItem.GetItemRefCurrentHealth 1")),
    _f4("amata.itemrefhealth.2", _in("pistol10", "rItem.GetItemRefCurrentHealth 2")),
    # Does every freshly added item come in below its base health (the pistol read 79.92)?
    _f4("fresh.leather.add", _o4("leather") + "rPlayer.AddItem rObj 1 1\nSetFunctionValue 1", exp="1", page="AddItem"),
    _f4("fresh.leather.health", _in("leather", "rItem.GetCurrentHealth")),
    _f4("fresh.leather.base", _o4("leather") + "SetFunctionValue (GetBaseHealth rObj)"),
    _f4("removeitemtarget.doc.before", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    _f4("removeitemtarget", _o4("stimpak") + "rPlayer.RemoveItemTarget rObj rDoc 2 0\nSetFunctionValue 1"),
    _f4("removeitemtarget.doc.after", _o4("stimpak") + "SetFunctionValue (rDoc.GetItemCount rObj)"),
    # Clean-up.
    _f4("cleanup.fifth", _o4("pistol10", "stimpak") + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\n"
        "rDoc.RemoveItem rOther 2 1\n" + _form("rObj", FX["leather"]) + "rPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1",
        exp="1", page="cleanup"),
]

# Twenty-second set: the xNVSE "other" group and the player-control locks, as the
# extender core asked (its values are hypotheses until these rows). Steps in order; every
# change is put back in the same or a later step (session only). Forms from fnvtool esm
# inventory and world refs [observed]: VCG01RemoveDocCarebearModeTriggerSCRIPT 11E65E,
# the Prospector Saloon's door 10618E (persistent), key HVCollarKey 175A56, NCR 0A46E7,
# BoS 01D3FE. A string_var is declared before Begin for Sv_Set.
FX.update(carebear="11E65E", saloondoor="10618E", collarkey="175A56", bos="1D3FE", docbase="104C0C")
_H5 = ("ref rPlayer\nref rObj\nref rOther\nref rDoc\nref rItem\nfloat fRes\nstring_var sText\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]))


def _f5(pid, body, text=False, exp=None, page=""):
    return (pid, _H5 + body, text, exp, page)


def _flags(j):
    """The 7 Alt flags with only flag j set."""
    return " ".join("1" if i == j else "0" for i in range(7))


_SEEN = " + ".join(f"(GetPlayerControlsDisabledAlt 1 {_flags(j)}) * {1 << j}" for j in range(7))
NVSE_FIFTH_PROBES = (
    # 1. Each DisablePlayerControlsAltEx bit, read back through the Alt getter's 7 flags
    # (bit j of the result: flag j seen as disabled), then enabled again.
    [_f5(f"controls.altex.bit{k}", f"DisablePlayerControlsAltEx {1 << k}\nlet fRes := {_SEEN}\n"
         f"EnablePlayerControlsAltEx {1 << k}\nSetFunctionValue fRes") for k in range(8)]
    + [_f5("controls.altex.none.after", f"SetFunctionValue ({_SEEN})")]
    # 2. Does the vanilla getter see an Alt lock?
    + [_f5("controls.alt.vanilla.sees", "DisablePlayerControlsAlt 1 0 0 0 0 0 0\nlet fRes := GetPlayerControlsDisabled 1 0 0 0 0 0 0\n"
           "EnablePlayerControlsAlt 1 0 0 0 0 0 0\nSetFunctionValue fRes"),
       # 3. A missing Alt argument.
       _f5("controls.alt.missing.args", "DisablePlayerControlsAlt 1\nlet fRes := GetPlayerControlsDisabledAlt 0 0 1\n"
           "let fRes := fRes * 10 + (GetPlayerControlsDisabledAlt 1 1 0 0 0 0 0 0)\nEnablePlayerControlsAlt 1\nSetFunctionValue fRes")]
    # 4. Console echo.
    + [_f5("consoleecho.start", "SetFunctionValue (GetConsoleEcho)"),
       _f5("consoleecho.set0.returns", "let fRes := SetConsoleEcho 0\nlet fRes := fRes * 10 + (GetConsoleEcho)\nSetConsoleEcho 1\nSetFunctionValue fRes")]
    # 5. Default forms.
    + [_f5("defaultform.0", "SetFunctionValue (GetFormIDString (GetNthDefaultForm 0))", True),
       _f5("defaultforms.size", "SetFunctionValue (ar_Size (GetDefaultForms))"),
       _f5("defaultform.set0", _o4("pistol10") + "let rItem := GetNthDefaultForm 0\nSetNthDefaultForm 0 rObj\n"
           "let rOther := GetNthDefaultForm 0\nSetNthDefaultForm 0 rItem\nSetFunctionValue (GetFormIDString rOther)", True),
       _f5("defaultform.0.restored", "SetFunctionValue (GetFormIDString (GetNthDefaultForm 0))", True)]
    # 6. Doc's front door (a load door): teleport target, then pointed at the saloon's door and back.
    + [_f5(f"door.teleport.{a.lower()}", _o4("door") + f"SetFunctionValue (rObj.GetDoorTeleport{a})") for a in ("X", "Y", "Z", "Rot")]
    + [_f5("door.teleport.set", _o4("door", "saloondoor") + "let rItem := rObj.GetLinkedDoor\n"
           'rPlayer.AuxVarSetFlt "*nvfDX" (rObj.GetDoorTeleportX)\nrPlayer.AuxVarSetFlt "*nvfDY" (rObj.GetDoorTeleportY)\n'
           'rPlayer.AuxVarSetFlt "*nvfDZ" (rObj.GetDoorTeleportZ)\nrPlayer.AuxVarSetFlt "*nvfDR" (rObj.GetDoorTeleportRot)\n'
           'rPlayer.AuxVarSetRef "*nvfDL" rItem\nrObj.SetDoorTeleport rOther\nSetFunctionValue (GetFormIDString (rObj.GetLinkedDoor))', True),
       _f5("door.teleport.set.x", _o4("door") + "SetFunctionValue (rObj.GetDoorTeleportX)"),
       _f5("door.teleport.restore", _o4("door") + 'let rItem := rPlayer.AuxVarGetRef "*nvfDL"\n'
           'rObj.SetDoorTeleport rItem (rPlayer.AuxVarGetFlt "*nvfDX") (rPlayer.AuxVarGetFlt "*nvfDY") (rPlayer.AuxVarGetFlt "*nvfDZ") (rPlayer.AuxVarGetFlt "*nvfDR")\n'
           + "".join(f'rPlayer.AuxVarErase "*nvfD{c}"\n' for c in "XYZRL") + "SetFunctionValue (GetFormIDString (rObj.GetLinkedDoor))", True),
       _f5("door.teleport.restored.x", _o4("door") + "SetFunctionValue (rObj.GetDoorTeleportX)")]
    # 7-8. Explicit references and script commands of a vanilla script.
    + [_f5("explicitrefs.count", _o4("carebear") + "SetFunctionValue (GetNumExplicitRefs rObj)"),
       _f5("explicitrefs.0", _o4("carebear") + "SetFunctionValue (GetFormIDString (GetNthExplicitRef 0 rObj))", True),
       _f5("hasscriptcommand.4192", _o4("carebear") + "SetFunctionValue (HasScriptCommand 4192 rObj)"),
       _f5("hasscriptcommand.4109", _o4("carebear") + "SetFunctionValue (HasScriptCommand 4109 rObj)"),
       _f5("hasscriptcommand.4192.event0", _o4("carebear") + "SetFunctionValue (HasScriptCommand 4192 rObj 0)"),
       _f5("hasscriptcommand.4192.event1", _o4("carebear") + "SetFunctionValue (HasScriptCommand 4192 rObj 1)")]
    # 9. Permanent flags.
    + [_f5("permanent.param", "SetFunctionValue (GetPermanent rDoc)"),
       _f5("permanent.calling", "SetFunctionValue (rDoc.GetPermanent)"),
       _f5("permanent.set", "let fRes := GetPermanent rDoc\nSetPermanent 1 rDoc\nlet fRes := fRes * 10 + (GetPermanent rDoc)\n"
           "let fRes := fRes * 10 + (rDoc.GetPermanent)\nSetPermanent 0 rDoc\nSetFunctionValue fRes")]
    # 10. RemoveScript, then the script put back with SetScript.
    + [_f5("removescript", _o4("docbase") + "let rItem := RemoveScript rObj\nlet rOther := rDoc.GetScript\n"
           "SetScript rItem rObj\nSetFunctionValue (sv_Construct \"%z %z\" (GetFormIDString rItem) (GetFormIDString rOther))", True),
       _f5("removescript.restored", "SetFunctionValue (GetFormIDString (rDoc.GetScript))", True)]
    # 11. RunScript on scripts without a Function block.
    + [_f5("runscript.quest", _o4("vcg02script") + "SetFunctionValue (RunScript rObj)"),
       _f5("runscript.object", _o4("carebear") + "SetFunctionValue (RunScript rObj)")]
    # 12. SetEditorID on a clone.
    + [_f5("seteditorid.clone", _o4("pistol10") + 'let rItem := TempCloneForm rObj\nSetEditorID rItem "nvfCloneProbe"\n'
           'SetFunctionValue (sv_Construct "%z %g" (GetEditorID rItem) ((EditorIDToFormID "nvfCloneProbe") == rItem))', True)]
    # 13-15. Formatted names and strings.
    + [_f5("setnameex", _o4("pistol10") + 'SetNameEx "Gun Five" rObj\nlet sText := GetName rObj\nSetNameEx "10mm Pistol" rObj\nSetFunctionValue sText', True),
       _f5("setnameex.format", _o4("pistol10") + 'SetNameEx "Gun %g" 5 rObj\nlet sText := GetName rObj\nSetNameEx "10mm Pistol" rObj\nSetFunctionValue sText', True),
       _f5("setnameex.restored", _o4("pistol10") + "SetFunctionValue (GetName rObj)", True),
       _f5("rankname.set", _o4("bos") + 'let sText := GetNthFactionRankName rObj 0\nSetNthFactionRankNameEx "Boss %g" 1 rObj 0\n'
           'let sText := GetNthFactionRankName rObj 0\nSetNthFactionRankNameEx "Squire" rObj 0\nSetFunctionValue sText', True),
       _f5("rankname.restored", _o4("bos") + "SetFunctionValue (GetNthFactionRankName rObj 0)", True),
       _f5("sv_set", 'Sv_Set "n=%g" 3 sText\nSetFunctionValue sText', True)]
    # 16. Power armour and controls flags.
    + [_f5("powerarmor.leather", _o4("leather") + "SetFunctionValue (IsPowerArmor rObj)"),
       _f5("powerarmor.t45d", _o4("t45d") + "SetFunctionValue (IsPowerArmor rObj)"),
       _f5("powerarmor.set", _o4("leather") + "SetIsPowerArmor 1 rObj\nlet fRes := IsPowerArmor rObj\nSetIsPowerArmor 0 rObj\nSetFunctionValue fRes"),
       _f5("iscontrol.88.before", "SetFunctionValue (IsControl 88)"),
       _f5("iscontrol.88.set", "SetIsControl 88 1\nlet fRes := IsControl 88\nSetIsControl 88 0\nSetFunctionValue fRes"),
       _f5("iscontrol.88.after", "SetFunctionValue (IsControl 88)")]
    # 17. Hotkeys (the player carries stimpaks in the save; hotkey 2's item is put back).
    + [_f5("hotkey.2.before.save", "SetFunctionValue (GetFormIDString (GetHotkeyItem 2))", True),
       _f5("hotkey.2.set", _o4("stimpak") + 'let rItem := GetHotkeyItem 2\nrPlayer.AuxVarSetRef "*nvfHK" rItem\n'
           "SetHotkeyItem 2 rObj\nSetFunctionValue (GetFormIDString (GetHotkeyItem 2))", True),
       _f5("hotkey.2.clear", "ClearHotkey 2\nSetFunctionValue (GetFormIDString (GetHotkeyItem 2))", True),
       _f5("hotkey.2.restore", 'let rItem := rPlayer.AuxVarGetRef "*nvfHK"\nif IsFormValid rItem\n  SetHotkeyItem 2 rItem\nendif\n'
           'rPlayer.AuxVarErase "*nvfHK"\nSetFunctionValue 1'),
       _f5("hotkey.3.not.carried", _o4("collarkey") + 'let rItem := GetHotkeyItem 3\nSetHotkeyItem 3 rObj\nlet rOther := GetHotkeyItem 3\n'
           "if IsFormValid rItem\n  SetHotkeyItem 3 rItem\nelse\n  ClearHotkey 3\nendif\nSetFunctionValue (GetFormIDString rOther)", True)]
    # 18. Owned items: condition and owner of the new item.
    + [_f5("owned.add", _o4("pistol10", "docbase") + "rPlayer.AddItemOwnership rObj 1 rOther 0\nSetFunctionValue 1"),
       _f5("owned.health", _in("pistol10", "rItem.GetCurrentHealth")),
       _f5("owned.owner", _in("pistol10", "GetFormIDString (rItem.GetOwner)").replace("let fRes := GetFormIDString", "let sText := GetFormIDString").replace("SetFunctionValue fRes", "SetFunctionValue sText"), True),
       _f5("owned.remove", _o4("pistol10") + "rPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1"),
       _f5("ownedpercent.add", _o4("pistol10", "docbase") + "rPlayer.AddItemHealthPercentOwner rObj 1 0.5 rOther 0\nSetFunctionValue 1"),
       _f5("ownedpercent.health", _in("pistol10", "rItem.GetCurrentHealth")),
       _f5("ownedpercent.remove", _o4("pistol10") + "rPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1")]
    # 19. Keys on Doc's door (unlocked) and on his dresser.
    + [_f5("openkey.door", _o4("door") + "SetFunctionValue (GetFormIDString (rObj.GetOpenKey))", True),
       # `rDoor.SetOpenKey rKey` on Doc's unlocked door crashed the game (2026-10-27) [observed]: not probed.
       _f5("openkey.door.restored", _o4("door") + "SetFunctionValue (GetFormIDString (rObj.GetOpenKey))", True),
       _f5("openkey.dresser", _o4("dresser") + "SetFunctionValue (GetFormIDString (rObj.GetOpenKey))", True)]
    # 21. The fly camera.
    + [_f5("tfc.before", "SetFunctionValue (GetTFC)"),
       _f5("tfc.toggle", "ToggleFlyCam\nSetFunctionValue 1"),
       _f5("tfc.after", "SetFunctionValue (GetTFC)"),
       _f5("tfc.toggle.back", "ToggleFlyCam\nSetFunctionValue 1"),
       _f5("tfc.end", "SetFunctionValue (GetTFC)")]
    # 22. The clip of the equipped pistol.
    + [_f5("clip.add", _o4("pistol10", "ammo10") + "rPlayer.AddItem rObj 1 1\nrPlayer.AddItem rOther 20 1\nSetFunctionValue 1"),
       _f5("clip.equip", _o4("pistol10") + "rPlayer.EquipItem rObj 0 1\nSetFunctionValue 1"),
       _f5("clip.before", "SetFunctionValue (GetPlayerCurrentAmmoRounds)"),
       _f5("clip.set3", "SetPlayerCurrentAmmoRounds 3\nSetFunctionValue 1"),
       _f5("clip.after", "SetFunctionValue (GetPlayerCurrentAmmoRounds)"),
       _f5("clip.cleanup", _o4("pistol10", "ammo10") + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\n"
           "rPlayer.RemoveItem rOther 20 1\nSetFunctionValue 1", exp="1", page="cleanup")]
)

# Twenty-third set: tokens, INI settings, event-handler lists, command opcodes and the
# debug functions, probed before NVForever builds them. Steps in order; every change
# is put back (session only). The player carries stimpaks in the owner's save; the
# collar key (175A56) is not carried. The INI settings read are in Fallout.ini /
# FalloutPrefs.ini of the owner's install.
_HE = ("ref rPlayer\nref rObj\nref rOther\nfloat fRes\nstring_var sText\narray_var aList\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]))


def _f6(pid, body, text=False, exp=None, page=""):
    return (pid, _HE + body, text, exp, page)


_LAMBDA = "(Begin Function {ref rA, ref rB}\nEnd)"
SIXTH_PROBES = [
    # Tokens: value and ref on a carried item and on an item not carried.
    _f6("token.value.before", _o4("stimpak") + "SetFunctionValue (rPlayer.GetTokenValue rObj)"),
    _f6("token.ref.before", _o4("stimpak") + "SetFunctionValue (GetFormIDString (rPlayer.GetTokenRef rObj))", True),
    _f6("token.setvalue", _o4("stimpak") + "rPlayer.SetTokenValue rObj 65.5\nSetFunctionValue (rPlayer.GetTokenValue rObj)"),
    _f6("token.value.next", _o4("stimpak") + "SetFunctionValue (rPlayer.GetTokenValue rObj)"),
    _f6("token.setref", _o4("stimpak", "pistol10") + "rPlayer.SetTokenRef rObj rOther\nSetFunctionValue (GetFormIDString (rPlayer.GetTokenRef rObj))", True),
    _f6("token.setboth", _o4("stimpak", "docbase") + "rPlayer.SetTokenValueAndRef rObj 7 rOther\n"
        'SetFunctionValue (sv_Construct "%g %z" (rPlayer.GetTokenValue rObj) (GetFormIDString (rPlayer.GetTokenRef rObj)))', True),
    _f6("token.notcarried", _o4("collarkey") + "rPlayer.SetTokenValue rObj 3\nSetFunctionValue (rPlayer.GetTokenValue rObj)"),
    # INI settings.
    _f6("ini.grids", 'SetFunctionValue (GetNumericINISetting "uGridsToLoad:General")'),
    _f6("ini.grids.alias", 'SetFunctionValue (GetNumericIniSetting "uGridsToLoad:General")'),
    _f6("ini.fov", 'SetFunctionValue (GetNumericINISetting "fDefaultFOV:Display")'),
    _f6("ini.case", 'SetFunctionValue (GetNumericINISetting "UGRIDSTOLOAD:GENERAL")'),
    _f6("ini.missing", 'SetFunctionValue (GetNumericINISetting "nvfNoSuchSetting:General")'),
    _f6("ini.nocategory", 'SetFunctionValue (GetNumericINISetting "uGridsToLoad")'),
    _f6("ini.string", 'SetFunctionValue (GetStringIniSetting "sLanguage:General")', True),
    _f6("ini.string.missing", 'SetFunctionValue (GetStringIniSetting "nvfNoSuchSetting:General")', True),
    _f6("ini.set.numeric", 'let fRes := GetNumericINISetting "fDefaultFOV:Display"\n'
        'let fRes := (SetNumericINISetting "fDefaultFOV:Display" 80) * 1000 + (GetNumericINISetting "fDefaultFOV:Display")\n'
        'SetNumericINISetting "fDefaultFOV:Display" 75\nSetFunctionValue fRes'),
    _f6("ini.set.numeric.missing", 'SetFunctionValue (SetNumericINISetting "nvfNoSuchSetting:General" 1)'),
    _f6("ini.fov.after", 'SetFunctionValue (GetNumericINISetting "fDefaultFOV:Display")'),
    # The original's value (ENGLISH, observed above) is put back as a literal: a string_var
    # given to SetStringIniSetting is taken as its name ("sText") [observed: first run].
    _f6("ini.set.string", 'SetStringIniSetting "sLanguage:General" "NVF"\n'
        'let fRes := (GetStringIniSetting "sLanguage:General") == "NVF"\nSetStringIniSetting "sLanguage:General" "ENGLISH"\nSetFunctionValue fRes'),
    _f6("ini.set.string.var", 'let sText := "NVF"\nSetStringIniSetting "sLanguage:General" sText\n'
        'let sText := GetStringIniSetting "sLanguage:General"\nSetStringIniSetting "sLanguage:General" "ENGLISH"\nSetFunctionValue sText', True),
    _f6("ini.string.after", 'SetFunctionValue (GetStringIniSetting "sLanguage:General")', True),
    _f6("gmst.setex", 'let sText := GetStringSetting "sTraitMenuTitleText"\nSetStringGameSettingEx "sTraitMenuTitleText|NVF title"\n'
        'let fRes := (GetStringSetting "sTraitMenuTitleText") == "NVF title"\nSetStringSetting "sTraitMenuTitleText" sText\nSetFunctionValue fRes'),
    # Event-handler lists around one registered lambda (removed at the end).
    _f6("events.count.before", 'SetFunctionValue (ar_Size (GetEventHandlers "OnActorEquip"))'),
    _f6("events.register", f'SetEventHandler "OnActorEquip" {_LAMBDA}\nSetFunctionValue (ar_Size (GetEventHandlers "OnActorEquip"))'),
    _f6("events.all.haskey", 'SetFunctionValue (ar_HasKey (GetEventHandlers) "OnActorEquip")'),
    _f6("events.all.type", 'SetFunctionValue (TypeOf (GetEventHandlers))', True),
    _f6("events.filtered.type", 'SetFunctionValue (TypeOf (GetEventHandlers "OnActorEquip"))', True),
    _f6("events.unknown.size", 'SetFunctionValue (ar_Size (GetEventHandlers "nvfNoSuchEvent"))'),
    _f6("events.count.after", 'let aList := GetEventHandlers "OnActorEquip"\nSetFunctionValue (ar_Size aList)'),
    # Commands and debug.
    _f6("opcode.activate", 'SetFunctionValue (GetCommandOpcode "Activate")', exp="4109", page="GetCommandOpcode (Activate 4109)"),
    _f6("opcode.case", 'SetFunctionValue (GetCommandOpcode "activate")'),
    _f6("opcode.nvse", 'SetFunctionValue (GetCommandOpcode "GetNVSEVersion")'),
    _f6("opcode.svlength", 'SetFunctionValue (GetCommandOpcode "Sv_Length")'),
    _f6("opcode.jip", 'SetFunctionValue (GetCommandOpcode "GetLNVersion")'),
    _f6("opcode.alias", 'SetFunctionValue (GetCommandOpcode "GetType")'),
    _f6("opcode.unknown", 'SetFunctionValue (GetCommandOpcode "nvfNoSuchCommand")'),
    _f6("debugmode", "SetFunctionValue (GetDebugMode)"),
    _f6("debugmode.0", "SetFunctionValue (GetDebugMode 0)"),
    _f6("printdebug", 'PrintDebug "nvf"\nSetFunctionValue 1'),
    _f6("assert.true", "Assert 1\nSetFunctionValue 1"),
    _f6("assert.false", "Assert 0\nSetFunctionValue 1"),
    _f6("ar_dumpid", "let aList := Ar_List 1 2\nAr_DumpID aList\nSetFunctionValue 1"),
    _f6("debugselection", "SetFunctionValue (GetFormIDString (GetDebugSelection))", True),
]

# Twenty-fourth set: the extender core's packages, paths and form flags, as it asked.
# Paths and flags are changed and put back; the package edits on Doc Mitchell run last
# and are not put back (RemoveAllPackages empties his list; session only, never saved).
# Forms from fnvtool esm inventory [observed]: DefaultStayAtEditorLocation 0200D5,
# WeapSniperRifle 004353.
FX.update(stayput="200D5", sniper="4353")
_H7 = ("ref rPlayer\nref rObj\nref rOther\nref rDoc\nref rItem\nfloat fRes\nstring_var sText\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]))


def _f7(pid, body, text=False, exp=None, page=""):
    return (pid, _H7 + body, text, exp, page)


SEVENTH_PROBES = [
    # Paths.
    _f7("scope.sniper", _o4("sniper") + "SetFunctionValue (GetScopeModelPath rObj)", True),
    _f7("scope.set", _o4("sniper") + 'let sText := GetScopeModelPath rObj\nSetScopeModelPath "nvf\\scope.nif" rObj\n'
        "let fRes := (GetScopeModelPath rObj) == \"nvf\\scope.nif\"\nSetScopeModelPath \"interface\\HUD\\mildotscope.NIF\" rObj\nSetFunctionValue fRes"),
    _f7("scope.restored", _o4("sniper") + "SetFunctionValue (GetScopeModelPath rObj)", True),
] + [_f7(f"bipedicon.leather.{c}", _o4("leather") + f"SetFunctionValue (GetBipedIconPath {c} rObj)", True) for c in range(4)] + [
    _f7("modelpathex", _o4("pistol10") + 'SetModelPathEx "nvf\\gun.nif" rObj\nlet sText := GetModelPath rObj\n'
        'SetModelPathEx "Weapons\\1HandPistol\\10mmPistol.NIF" rObj\nSetFunctionValue sText', True),
    _f7("modelpathex.restored", _o4("pistol10") + "SetFunctionValue (GetModelPath rObj)", True),
    _f7("bipedmodelpathex.format", _o4("leather") + 'SetBipedModelPathEx "b%g.nif" 2 1 rObj\nlet sText := GetBipedModelPath 1 rObj\n'
        'SetBipedModelPathEx "Armor\\LeatherArmor\\F\\OutfitF.NIF" 1 rObj\nSetFunctionValue sText', True),
    _f7("bipedmodelpathex.restored", _o4("leather") + "SetFunctionValue (GetBipedModelPath 1 rObj)", True),
    # Form flags.
    _f7("flags.pistol.low", _o4("pistol10") + "SetFunctionValue (GetFlagsLow rObj)"),
    _f7("flags.pistol.high", _o4("pistol10") + "SetFunctionValue (GetFlagsHigh rObj)"),
    _f7("flags.docref.low", "SetFunctionValue (GetFlagsLow rDoc)"),
    _f7("flags.docref.high", "SetFunctionValue (GetFlagsHigh rDoc)"),
    _f7("flags.docref.calling.low", "SetFunctionValue (rDoc.GetFlagsLow)"),
    _f7("flags.pistol.set", _o4("pistol10") + "let fRes := GetFlagsLow rObj\nSetFlagsLow 1024 rObj\nlet rItem := rObj\n"
        "let fRes := fRes * 10000 + (GetFlagsLow rObj)\nSetFlagsLow 9 rObj\nSetFunctionValue fRes"),
    _f7("flags.pistol.restored", _o4("pistol10") + "SetFunctionValue (GetFlagsLow rObj)"),
    # Hair and eyes flags.
    _f7("hairflags.doc", _o4("docbase") + "SetFunctionValue (GetHairFlags (GetHair rObj))"),
    _f7("eyesflags.doc", _o4("docbase") + "SetFunctionValue (GetEyesFlags (GetEyes rObj))"),
    _f7("hairflags.set", _o4("docbase") + "let rItem := GetHair rObj\nlet fRes := GetHairFlags rItem\nrPlayer.AuxVarSetFlt \"*nvfHF\" fRes\n"
        "SetHairFlags rItem 9\nSetFunctionValue (GetHairFlags rItem)"),
    _f7("hairflags.restore", _o4("docbase") + 'let rItem := GetHair rObj\nSetHairFlags rItem (rPlayer.AuxVarGetFlt "*nvfHF")\n'
        'rPlayer.AuxVarErase "*nvfHF"\nSetFunctionValue (GetHairFlags rItem)'),
    # Weapon-mod flags of the equipped weapon.
    _f7("modflags.add", _o4("pistol10", "ammo10") + "rPlayer.AddItem rObj 1 1\nrPlayer.AddItem rOther 20 1\nSetFunctionValue 1"),
    _f7("modflags.equip", _o4("pistol10") + "rPlayer.EquipItem rObj 0 1\nSetFunctionValue 1"),
    _f7("modflags.before", "SetFunctionValue (rPlayer.GetEquippedWeaponModFlags)"),
    _f7("modflags.set1", "rPlayer.SetEquippedWeaponModFlags 1\nSetFunctionValue (rPlayer.GetEquippedWeaponModFlags)"),
    _f7("modflags.set1.next", "SetFunctionValue (rPlayer.GetEquippedWeaponModFlags)"),
    _f7("modflags.cleanup", _o4("pistol10", "ammo10") + "rPlayer.SetEquippedWeaponModFlags 0\nrPlayer.UnequipItem rObj 0 1\n"
        "rPlayer.RemoveItem rObj 1 1\nrPlayer.RemoveItem rOther 20 1\nSetFunctionValue 1", exp="1", page="cleanup"),
    # Packages on Doc (last: not put back).
    _f7("pkg.nth0.before", "SetFunctionValue (GetFormIDString (rDoc.GetNthPackage 0))", True),
    _f7("pkg.count.before", "SetFunctionValue (rDoc.GetPackageCount)"),
    _f7("pkg.radius.0", "let rItem := rDoc.GetNthPackage 0\nSetFunctionValue (GetPackageLocationRadius rItem)"),
    _f7("pkg.radius.set", "let rItem := rDoc.GetNthPackage 0\nlet fRes := GetPackageLocationRadius rItem\nSetPackageLocationRadius rItem 321\n"
        "let fRes := fRes * 1000 + (GetPackageLocationRadius rItem)\nSetFunctionValue fRes"),
    _f7("pkg.locref.set", "let rItem := rDoc.GetNthPackage 0\nrDoc.SetPackageLocationReference rItem rPlayer\nSetFunctionValue 1"),
    _f7("pkg.target.set.calling", "let rItem := rDoc.GetNthPackage 0\nrDoc.SetPackageTargetReference rItem\nSetFunctionValue 1"),
    _f7("pkg.addat0", _o4("stayput") + "SetFunctionValue (rDoc.AddPackageAt rObj 0)"),
    _f7("pkg.nth0.after.add", "SetFunctionValue (GetFormIDString (rDoc.GetNthPackage 0))", True),
    _f7("pkg.count.after.add", "SetFunctionValue (rDoc.GetPackageCount)"),
    _f7("pkg.removeat0", "SetFunctionValue (GetFormIDString (rDoc.RemovePackageAt 0))", True),
    _f7("pkg.nth0.after.remove", "SetFunctionValue (GetFormIDString (rDoc.GetNthPackage 0))", True),
    _f7("pkg.addat.minus1", _o4("stayput") + "SetFunctionValue (rDoc.AddPackageAt rObj -1)"),
    _f7("pkg.count.after.minus1", "SetFunctionValue (rDoc.GetPackageCount)"),
    _f7("pkg.addat.past", _o4("stayput") + "SetFunctionValue (rDoc.AddPackageAt rObj 99)"),
    _f7("pkg.count.after.past", "SetFunctionValue (rDoc.GetPackageCount)"),
    _f7("pkg.removeall", "rDoc.RemoveAllPackages\nSetFunctionValue (GetFormIDString (rDoc.GetNthPackage 0))", True),
    _f7("pkg.count.after.removeall", "SetFunctionValue (rDoc.GetPackageCount)"),
]

# Twenty-fifth set: the extender core's last families, as it asked: animations and the
# generic form lists on Doc Mitchell, the con_ console functions, UI traits; and which
# references carry form flag 64 (Doc's reference reads 1097 in the game, 1033 + 64).
# Forms from fnvtool esm inventory [observed]: CaucasianOld 0042C3, EyebrowM 0B5BEA,
# BeardMustacheOld 07B47B. Game settings, INI values and head parts are put back; the
# package and animation edits on Doc are not (session only, never saved). Con_SCOF is
# left out: it writes a file into the owner's game folder (owner-review, J9).
FX.update(caucold="42C3", beardold="7B47B", list127="1613D5")
_H8 = ("ref rPlayer\nref rObj\nref rOther\nref rDoc\nref rBase\nref rF\nref rItem\nfloat fRes\nstring_var sText\n"
       "Begin Function { }\n" + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + _form("rBase", NF["doc"]))
_WALK8 = ("let fRes := 0\nlet sText := \"\"\nwhile IsFormValid rItem && fRes < 40\n"
          "  let sText += (GetFormIDString rItem) + (sv_Construct \":%g:%g \" (GetFlagsLow rItem) (GetType (rItem.GetBaseObject)))\n"
          "  let fRes += 1\n  let rItem := GetNextRef\nloop\nSetFunctionValue sText")


def _f8(pid, body, text=False, exp=None, page=""):
    return (pid, _H8 + body, text, exp, page)


def _id8(pid, expr):
    return _f8(pid, f"SetFunctionValue (GetFormIDString ({expr}))", True)


EIGHTH_PROBES = [
    # Form flags: single references, then walks of Doc's house (not loaded in the save:
    # persistent references only) and the player's own cell (loaded).
    _f8("flags.player", "SetFunctionValue (GetFlagsLow rPlayer)"),
    _f8("flags.docref", "SetFunctionValue (GetFlagsLow rDoc)"),
    _f8("flags.chet", _o4("chetref") + "SetFunctionValue (GetFlagsLow rObj)"),
    _f8("flags.saloondoor", _o4("saloondoor") + "SetFunctionValue (GetFlagsLow rObj)"),
    _f8("walk.doccell", _form("rF", CELLS["doc"]) + "let rItem := GetFirstRefInCell rF 0 0 0\n" + _WALK8, True),
    _f8("walk.here.actors", "let rItem := GetFirstRef 200 0 0\n" + _WALK8, True),
    _f8("walk.here", "let rItem := GetFirstRef 0 0 0\n" + _WALK8, True),
    # Animations (KFFZ): the base's list or the reference's.
    _f8("anim.base.0", "SetFunctionValue (GetNthAnimation 0 rBase)", True),
    _f8("anim.ref.0", "SetFunctionValue (rDoc.GetNthAnimation 0)", True),
    _f8("anim.add", 'SetFunctionValue (AddAnimation "nvf\\test.kf" rBase)'),
    _f8("anim.add.base.0", "SetFunctionValue (GetNthAnimation 0 rBase)", True),
    _f8("anim.add.ref.0", "SetFunctionValue (rDoc.GetNthAnimation 0)", True),
    _f8("anim.add2", 'SetFunctionValue (AddAnimation "nvf\\second.kf" rBase)'),
    _f8("anim.add2.1", "SetFunctionValue (GetNthAnimation 1 rBase)", True),
    _f8("anim.del", 'SetFunctionValue (DelAnimation "nvf\\test.kf" rBase)'),
    _f8("anim.del.0", "SetFunctionValue (GetNthAnimation 0 rBase)", True),
    _f8("anim.del.missing", 'SetFunctionValue (DelAnimation "nvf\\none.kf" rBase)'),
    _f8("anim.delall", "SetFunctionValue (DelAnimations rBase)"),
    _f8("anim.delall.0", "SetFunctionValue (GetNthAnimation 0 rBase)", True),
    # Generic form lists.
    _id8("generic.10.ref.0", "GenericGetForm 10 rDoc 0"),
    _id8("generic.10.base.0", "GenericGetForm 10 rBase 0"),
    _id8("generic.10.ref.1", "GenericGetForm 10 rDoc 1"),
    _id8("generic.5.ref.0", "GenericGetForm 5 rDoc 0"),
    _id8("generic.5.base.0", "GenericGetForm 5 rBase 0"),
    _f8("generic.race.0", _o4("caucold") + "SetFunctionValue (GetFormIDString (GenericGetForm 0 rObj 0))", True),
    _f8("generic.race.1", _o4("caucold") + "SetFunctionValue (GetFormIDString (GenericGetForm 1 rObj 0))", True),
    _f8("generic.list.13.0", _o4("list127") + "SetFunctionValue (GetFormIDString (GenericGetForm 13 rObj 0))", True),
    _f8("generic.list.13.1", _o4("list127") + "SetFunctionValue (GetFormIDString (GenericGetForm 13 rObj 1))", True),
    _f8("generic.10.replace", _o4("beardold") + "let rItem := GenericGetForm 10 rDoc 0\nrPlayer.AuxVarSetRef \"*nvfHP\" rItem\n"
        "SetFunctionValue (GetFormIDString (GenericReplaceForm 10 rDoc rObj 0))", True),
    _id8("generic.10.after.replace", "GenericGetForm 10 rDoc 0"),
    _f8("generic.10.restore", 'let rItem := rPlayer.AuxVarGetRef "*nvfHP"\nlet rOther := GenericReplaceForm 10 rDoc rItem 0\n'
        'rPlayer.AuxVarErase "*nvfHP"\nSetFunctionValue (GetFormIDString (GenericGetForm 10 rDoc 0))', True),
    _f8("generic.5.add", _o4("stayput") + "SetFunctionValue (GenericAddForm 5 rDoc rObj 0)"),
    _id8("generic.5.after.add", "rDoc.GetNthPackage 0"),
    _id8("generic.5.delete", "GenericDeleteForm 5 rDoc 0"),
    _id8("generic.5.after.delete", "rDoc.GetNthPackage 0"),
    _f8("generic.5.add.minus2", _o4("stayput") + "SetFunctionValue (GenericAddForm 5 rDoc rObj -2)"),
    _f8("generic.5.count.minus2", "SetFunctionValue (rDoc.GetPackageCount)"),
    _id8("generic.5.last.minus2", "rDoc.GetNthPackage ((rDoc.GetPackageCount) - 1)"),
    # Console functions.
    _f8("con.echo", "SetConsoleEcho 1\nSetFunctionValue (GetConsoleEcho)"),
    _f8("con.gmst.before", 'SetFunctionValue (GetNumericGameSetting "fJumpFallHeightMin")'),
    _f8("con.gmst.set", 'con_SetGameSetting "fJumpFallHeightMin" "100"\nSetFunctionValue (GetNumericGameSetting "fJumpFallHeightMin")'),
    _f8("con.gmst.restore", 'con_SetGameSetting "fJumpFallHeightMin" "600"\nSetFunctionValue (GetNumericGameSetting "fJumpFallHeightMin")'),
    _f8("con.scof.name", "SetFunctionValue (GetConsoleOutputFilename)", True),
    _f8("con.scof.has", "SetFunctionValue (HasConsoleOutputFilename)"),
    _f8("con.tfc.before", "SetFunctionValue (GetTFC)"),
    _f8("con.tfc.on", "Con_TFC\nSetFunctionValue (GetTFC)"),
    _f8("con.tfc.on.next", "SetFunctionValue (GetTFC)"),
    _f8("con.tfc.off", "Con_TFC\nSetFunctionValue (GetTFC)"),
    _f8("con.tfc.off.next", "SetFunctionValue (GetTFC)"),
    _f8("con.ini.get", 'SetFunctionValue (con_GetINISetting "fDefaultFOV:Display")'),
    _f8("con.ini.set", 'con_SetINISetting "fDefaultFOV:Display" "80"\nSetFunctionValue (GetNumericINISetting "fDefaultFOV:Display")'),
    _f8("con.ini.restore", 'con_SetINISetting "fDefaultFOV:Display" "75"\nSetFunctionValue (GetNumericINISetting "fDefaultFOV:Display")'),
    _f8("con.echo.off", "SetConsoleEcho 0\nSetFunctionValue (GetConsoleEcho)"),
    # UI traits.
    _f8("ui.x", 'SetFunctionValue (GetUIFloat "HUDMainMenu/_X")'),
    _f8("ui.visible", 'SetFunctionValue (GetUIFloat "HUDMainMenu/visible")'),
    _f8("ui.missing", 'SetFunctionValue (GetUIFloat "NVFNoMenu/_nvf")'),
    _f8("ui.user.set", 'SetUIFloat "HUDMainMenu/_nvfprobe" 5\nSetFunctionValue (GetUIFloat "HUDMainMenu/_nvfprobe")'),
    _f8("ui.user.next", 'SetFunctionValue (GetUIFloat "HUDMainMenu/_nvfprobe")'),
    _f8("ui.stringex", 'SetUIStringEx "HUDMainMenu/_nvfstr" "n%g" 3\nSetFunctionValue (GetUIString "HUDMainMenu/_nvfstr")', True),
]

# Twenty-sixth set (next batch N1): LU's leveled-list functions on an LVLI (Ammo10mm75
# 02935E, CapsFortuneFinder 0645D4), an LVLN (VarVHDNCRRanger9Veteran 17A2FE) and an LVLC
# (VEncTier4SupermutantMelee50 174435, adding VEncTier4SupermutantMinigun50 174434);
# forms from fnvtool esm inventory [observed]. CapsFortuneFinder is rebuilt to one entry
# (3 stimpaks, chance none 0) so AddItem of the list is certain, and the stimpaks are
# taken back. The list edits are not put back (LU's are not savebaked [documented:
# RemoveFormFromLeveledList]; session only, never saved). The versions the extender
# plugins report come first, under the names the wiki gives and the likely others.
FX.update(lvli="2935E", fortune="645D4", lvlc="174435", lvlc2="174434", stim="15169", capsf="F", docnpc="104C0C")
_HL = ("ref rPlayer\nref rLst\nref rObj\nref rOther\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"]))


def _lv(pid, lst, body, text=False):
    return (pid, _HL + _form("rLst", FX[lst]) + body, text, None, "")


def _lvid(pid, lst, expr):
    return _lv(pid, lst, f"SetFunctionValue (GetFormIDString ({expr}))", True)


_PLUGIN_NAMES = ["ShowOffNVSE Plugin", "ShowOff NVSE Plugin", "ShowOffNVSE", "kNVSE", "kNVSE Plugin",
                 "JIP LN NVSE", "JohnnyGuitarNVSE"]


def _lv_reads(tag, lst):
    return [
        _lv(f"{tag}.num", lst, "SetFunctionValue (GetNumLevItems rLst)"),
        _lv(f"{tag}.flags", lst, "SetFunctionValue (GetLeveledListFlags rLst)"),
        _lv(f"{tag}.chancenone", lst, "SetFunctionValue (GetChanceNone rLst)"),
        _lvid(f"{tag}.chancenoneglobal", lst, "GetChanceNoneGlobal rLst"),
        _lvid(f"{tag}.nth0", lst, "GetNthLevItem rLst 0"),
        _lv(f"{tag}.nth0.level", lst, "SetFunctionValue (GetNthLevItemLevel rLst 0)"),
        _lv(f"{tag}.nth0.count", lst, "SetFunctionValue (GetNthLevItemCount rLst 0)"),
        _lvid(f"{tag}.nth.past", lst, "GetNthLevItem rLst 99"),
        _lv(f"{tag}.nth.past.level", lst, "SetFunctionValue (GetNthLevItemLevel rLst 99)"),
        _lv(f"{tag}.index.level1", lst, "SetFunctionValue (GetLevItemIndexByLevel rLst 1)"),
        _lv(f"{tag}.index.level100", lst, "SetFunctionValue (GetLevItemIndexByLevel rLst 100)"),
    ]


LEVELED_PROBES = (
    [(f"plugin.version.{n}", f'SetFunctionValue (GetPluginVersion "{n}")', False, None, "") for n in _PLUGIN_NAMES]
    + [(f"plugin.installed.{n}", f'SetFunctionValue (IsPluginInstalled "{n}")', False, None, "") for n in _PLUGIN_NAMES]
    # Reads.
    + _lv_reads("lvli", "lvli") + _lv_reads("lvln", "lvln") + _lv_reads("lvlc", "lvlc")
    + [
        _lv("lvli.index.form0", "lvli", "let rObj := GetNthLevItem rLst 0\nSetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.index.missing", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.hasdeep.nth0", "lvli", "let rObj := GetNthLevItem rLst 0\nSetFunctionValue (LeveledListHasFormDeep rLst rObj)"),
        _lv("lvli.hasdeep.missing", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (LeveledListHasFormDeep rLst rObj)"),
        # LVLI: add, read the new entry, replace, remove.
        _lv("lvli.add", "lvli", _form("rObj", FX["capsf"]) + "LeveledListAddForm rLst rObj 7 25 1.0\nSetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvli.add.index", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.add.level", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetNthLevItemLevel rLst (GetLevItemIndexByForm rLst rObj))"),
        _lv("lvli.add.count", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetNthLevItemCount rLst (GetLevItemIndexByForm rLst rObj))"),
        _lv("lvli.add.index.level7", "lvli", "SetFunctionValue (GetLevItemIndexByLevel rLst 7)"),
        _lv("lvli.add.hasdeep", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (LeveledListHasFormDeep rLst rObj)"),
        _lv("lvli.add.twice", "lvli", _form("rObj", FX["capsf"]) + "LeveledListAddForm rLst rObj 3 5 1.0\nSetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvli.add.twice.index", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.replace", "lvli", _form("rObj", FX["capsf"]) + _form("rOther", FX["stim"])
            + "LeveledListReplaceForm rLst rObj rOther\nSetFunctionValue (GetLevItemIndexByForm rLst rOther)"),
        _lv("lvli.replace.old", "lvli", _form("rObj", FX["capsf"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.remove", "lvli", _form("rObj", FX["stim"]) + "SetFunctionValue (LeveledListRemoveForm rLst rObj)"),
        _lv("lvli.remove.num", "lvli", "SetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvli.remove.again", "lvli", _form("rObj", FX["stim"]) + "SetFunctionValue (LeveledListRemoveForm rLst rObj)"),
        # LVLI: Nth setters, chance none, flags.
        _lv("lvli.setcount", "lvli", "SetNthLevItemCount rLst 0 42\nSetFunctionValue (GetNthLevItemCount rLst 0)"),
        _lv("lvli.setlevel", "lvli", "SetNthLevItemLevel rLst 0 9\nSetFunctionValue (GetNthLevItemLevel rLst 0)"),
        _lvid("lvli.setlevel.nth0", "lvli", "GetNthLevItem rLst 0"),
        _lv("lvli.setitem", "lvli", _form("rObj", FX["stim"]) + "SetNthLevItem rLst 0 rObj\nSetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvli.chancenone.set", "lvli", "SetChanceNone rLst 35\nSetFunctionValue (GetChanceNone rLst)"),
        _lv("lvli.chancenone.big", "lvli", "SetChanceNone rLst 250\nSetFunctionValue (GetChanceNone rLst)"),
        _lv("lvli.flags.set", "lvli", "SetLeveledListFlags rLst 5\nSetFunctionValue (GetLeveledListFlags rLst)"),
        _lv("lvli.flags.set8", "lvli", "SetLeveledListFlags rLst 15\nSetFunctionValue (GetLeveledListFlags rLst)"),
        _lv("lvli.removenth", "lvli", "let fRes := GetNumLevItems rLst\nRemoveNthLevItem rLst 0\nSetFunctionValue (fRes * 100 + (GetNumLevItems rLst))"),
        _lv("lvli.removenth.past", "lvli", "let fRes := GetNumLevItems rLst\nRemoveNthLevItem rLst 99\nSetFunctionValue (fRes * 100 + (GetNumLevItems rLst))"),
        _lv("lvli.showoff.remove", "lvli", "let rObj := GetNthLevItem rLst 0\nSetFunctionValue (RemoveFormFromLeveledList rLst rObj)"),
        _lv("lvli.clear", "lvli", "LeveledListClear rLst\nSetFunctionValue (GetNumLevItems rLst)"),
        _lvid("lvli.clear.nth0", "lvli", "GetNthLevItem rLst 0"),
        # A list rebuilt to one entry, then added to the player.
        _lv("fortune.num.before", "fortune", "SetFunctionValue (GetNumLevItems rLst)"),
        _lv("fortune.rebuild", "fortune", _form("rObj", FX["stim"])
            + "LeveledListClear rLst\nSetChanceNone rLst 0\nSetLeveledListFlags rLst 0\nLeveledListAddForm rLst rObj 1 3 1.0\n"
            "SetFunctionValue (GetNumLevItems rLst)"),
        _lv("fortune.stim.before", "fortune", _form("rObj", FX["stim"]) + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
        _lv("fortune.additem", "fortune", "rPlayer.AddItem rLst 1 1\nSetFunctionValue 1"),
        _lv("fortune.stim.after", "fortune", _form("rObj", FX["stim"]) + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
        _lv("fortune.list.carried", "fortune", "SetFunctionValue (rPlayer.GetItemCount rLst)"),
        _lv("fortune.cleanup", "fortune", _form("rObj", FX["stim"]) + "rPlayer.RemoveItem rObj 3 1\nSetFunctionValue 1"),
        _lv("fortune.stim.cleaned", "fortune", _form("rObj", FX["stim"]) + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
        # LVLN and LVLC: add and remove.
        _lv("lvln.add", "lvln", _form("rObj", FX["docnpc"]) + "LeveledListAddForm rLst rObj 1 1 0\nSetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvln.add.index", "lvln", _form("rObj", FX["docnpc"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvln.add.count", "lvln", _form("rObj", FX["docnpc"]) + "SetFunctionValue (GetNthLevItemCount rLst (GetLevItemIndexByForm rLst rObj))"),
        _lv("lvln.remove", "lvln", _form("rObj", FX["docnpc"]) + "SetFunctionValue (LeveledListRemoveForm rLst rObj)"),
        _lv("lvln.remove.num", "lvln", "SetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvlc.add.list", "lvlc", _form("rObj", FX["lvlc2"]) + "LeveledListAddForm rLst rObj 2 1 0\nSetFunctionValue (GetNumLevItems rLst)"),
        _lv("lvlc.add.list.index", "lvlc", _form("rObj", FX["lvlc2"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
        _lv("lvlc.hasdeep.sub", "lvlc", _form("rObj", FX["lvlc2"]) + "let rOther := GetNthLevItem rObj 0\nSetFunctionValue (LeveledListHasFormDeep rLst rOther)"),
        _lv("lvlc.flags.set", "lvlc", "let fRes := GetLeveledListFlags rLst\nSetLeveledListFlags rLst 4\n"
            "let fRes := fRes * 10 + (GetLeveledListFlags rLst)\nSetFunctionValue fRes"),
        _lv("lvlc.clear", "lvlc", "LeveledListClear rLst\nSetFunctionValue (GetNumLevItems rLst)"),
        # Not a leveled list.
        _lv("notlist.num", "fortune", _form("rObj", FX["stim"]) + "SetFunctionValue (GetNumLevItems rObj)"),
        _lv("notlist.flags", "fortune", _form("rObj", FX["stim"]) + "SetFunctionValue (GetLeveledListFlags rObj)"),
    ]
)

# Twenty-seventh set (next batch N14): which references carry form flag 64 at run time.
# Doc reads 1097 where his ACHR record carries 0x400 (fnvtool esm inventory) [observed].
# Persistent ACHRs of Goodsprings (Chet, Joe Cobb 0xC00, Sunny, Trudy, Ringo, Easy Pete),
# the player, a loaded ACRE near the airport (14DEE2), REFRs whose record carries 0x40
# (Hidden From Local Map in the schema), an NPC and a creature placed with PlaceAtMe
# (non-persistent), the ACRE and the placed NPC killed, and Doc with his cell loaded (the
# player moved there, last). Session only, never saved.
_FREFS = {"doc": "104C0F", "chet": "104C79", "joecobb": "104C68", "sunny": "104E85", "trudy": "104C6D",
          "ringo": "104C7D", "easypete": "104C80", "player": "14", "acre": "14DEE2",
          "hidden.16941d": "16941D", "hidden.1690f1": "1690F1", "hidden.e7514": "E7514", "hidden.101a47": "101A47"}
_HF = ("ref rPlayer\nref rF\nref rItem\nfloat fRes\nstring_var sText\nBegin Function { }\n" + _form("rPlayer", NF["player"]))
_FLAGS_OF = ('if IsFormValid rF\n  SetFunctionValue (sv_Construct "%g %g" (GetFlagsLow rF) (rF.GetDisabled))\n'
             'else\n  SetFunctionValue \"none\"\nendif')
_AUXF = 'let rF := rPlayer.AuxVarGetRef "*nvfFL"\n'


def _fl(pid, body):
    return (pid, _HF + body, True, None, "")


def _flref(pid, fid, after=""):
    return _fl(pid, _form("rF", fid) + after + _FLAGS_OF)


FLAG_PROBES = (
    [_flref(f"ref.{k}", fid) for k, fid in _FREFS.items()]
    + [
        _flref("ref.doc.persistent", "104C0F", "SetFunctionValue (rF.IsPersistent)\nreturn\n"),
    ]
    # Whether bit 64 follows having spoken with the player.
    + [_flref(f"talked.{k}", _FREFS[k], "SetFunctionValue (rF.GetTalkedToPC)\nreturn\n")
       for k in ("doc", "chet", "joecobb", "sunny", "trudy", "ringo", "easypete")]
    + [
        _flref("acre.dead.before", "14DEE2", "SetFunctionValue (rF.GetDead)\nreturn\n"),
        # Placed NPC (Doc's base) and placed creature (the ACRE's base).
        _fl("placed.npc", _form("rItem", NF["doc"]) + 'let rF := rPlayer.PlaceAtMe rItem 1\nrPlayer.AuxVarSetRef "*nvfFL" rF\n' + _FLAGS_OF),
        _fl("placed.npc.next", _AUXF + _FLAGS_OF),
        _fl("placed.npc.next2", _AUXF + _FLAGS_OF),
        _fl("placed.npc.kill", _AUXF + "rF.Kill\n" + _FLAGS_OF),
        _fl("placed.npc.dead", _AUXF + _FLAGS_OF),
        _fl("placed.npc.dead2", _AUXF + _FLAGS_OF),
        _fl("placed.npc.getdead", _AUXF + "SetFunctionValue (rF.GetDead)"),
        _fl("placed.creature", _form("rItem", "14DEE2") + 'let rItem := rItem.GetBaseObject\nlet rF := rPlayer.PlaceAtMe rItem 1\n'
            'rPlayer.AuxVarSetRef "*nvfFL" rF\n' + _FLAGS_OF),
        _fl("placed.creature.next", _AUXF + _FLAGS_OF),
        _fl("placed.cleanup", _AUXF + 'rF.Disable\nrF.MarkForDelete\nrPlayer.AuxVarErase "*nvfFL"\nSetFunctionValue "1"'),
        # The loaded ACRE killed.
        _flref("acre.kill", "14DEE2", "rF.Kill\n"),
        _flref("acre.dead", "14DEE2"),
        _flref("acre.dead2", "14DEE2"),
        _flref("acre.getdead", "14DEE2", "SetFunctionValue (rF.GetDead)\nreturn\n"),
        # Doc with his cell loaded.
        _flref("doc.moveto", "104C0F", "rPlayer.MoveTo rF\n"),
        _flref("doc.loaded", "104C0F"),
        _flref("doc.loaded2", "104C0F"),
        _flref("doc.loaded3", "104C0F"),
        _fl("doc.loaded.player.cell", "SetFunctionValue (GetFormIDString (rPlayer.GetParentCell))"),
        _fl("doc.loaded.walk", "let rItem := GetFirstRef 0 0 0\nlet fRes := 0\nlet sText := \"\"\nwhile IsFormValid rItem && fRes < 40\n"
            "  let sText += (GetFormIDString rItem) + (sv_Construct \":%g \" (GetFlagsLow rItem))\n"
            "  let fRes += 1\n  let rItem := GetNextRef\nloop\nSetFunctionValue sText"),
    ]
)

# Twenty-eighth set (next batch N3): INI files under Data\Config, read and written by
# LU (GetINIFloat, GetINIString, SetINIFloat), ShowOff (GetINI…OrDefault_Cached,
# GetINI…OrCreate_Cached) and MCM (Get/SetModINISetting). Before the run the harness
# writes INI_FILE_TEXT to Data\Config\NVFExtProbe\probe.ini and Data\Config\NVFExtProbe.ini;
# every write names an NVFExtProbe path; after the run the files are read back (what the
# game wrote) and removed (owner-review: probe files in the game's Data\Config).
INI_FILE_TEXT = "[General]\nfNum=3.5\niInt=7\nsText=hello world\n[Other]\nfNum=-2\n"
_PI = '"NVFExtProbe\\probe.ini"'
_NEW = '"NVFExtProbe\\created.ini"'


def _ini(pid, body, text=False):
    return (pid, "float fRes\nstring_var sText\nBegin Function { }\n" + body, text, None, "")


def _gf(pid, key, path=_PI):
    return _ini(pid, f'SetFunctionValue (GetINIFloat "{key}" {path})')


INI_FILE_PROBES = [
    # LU reads.
    _gf("lu.float", "General:fNum"),
    _gf("lu.float.int", "General:iInt"),
    _gf("lu.float.other", "Other:fNum"),
    _gf("lu.float.slash", "General/fNum"),
    _gf("lu.float.backslash", "General\\fNum"),
    _gf("lu.float.case", "general:FNUM"),
    _gf("lu.float.string", "General:sText"),
    _gf("lu.float.missingkey", "General:fNone"),
    _gf("lu.float.missingfile", "General:fNum", '"NVFExtProbe\\none.ini"'),
    _gf("lu.float.noext", "General:fNum", '"NVFExtProbe\\probe"'),
    _gf("lu.float.top", "General:fNum", '"NVFExtProbe.ini"'),
    _ini("lu.string", f'SetFunctionValue (GetINIString "General:sText" {_PI})', True),
    _ini("lu.string.num", f'SetFunctionValue (GetINIString "General:fNum" {_PI})', True),
    _ini("lu.string.missing", f'SetFunctionValue (GetINIString "General:sNone" {_PI})', True),
    # ShowOff cached reads.
    _ini("so.float", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General:fNum" {_PI} 9)'),
    _ini("so.float.alias", f'SetFunctionValue (GetINIFloat_Cached "General:fNum" {_PI} 9)'),
    _ini("so.float.slash", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General/fNum" {_PI} 9)'),
    _ini("so.float.missingkey", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General:fNone" {_PI} 9)'),
    _ini("so.float.missingfile", 'SetFunctionValue (GetINIFloatOrDefault_Cached "General:fNum" "NVFExtProbe\\none.ini" 9)'),
    _ini("so.float.nodefault", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General:fNone" {_PI})'),
    _ini("so.float.string", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General:sText" {_PI} 9)'),
    _ini("so.string", f'SetFunctionValue (GetINIStringOrDefault_Cached "General:sText" {_PI} "dflt")', True),
    _ini("so.string.missing", f'SetFunctionValue (GetINIStringOrDefault_Cached "General:sNone" {_PI} "dflt")', True),
    _ini("so.string.num", f'SetFunctionValue (GetINIStringOrDefault_Cached "General:fNum" {_PI} "dflt")', True),
    # ShowOff creates (cache only: no SaveCachedIniFile).
    _ini("so.create.float", f'SetFunctionValue (GetINIFloatOrCreate_Cached "General:fMade" {_PI} 4.5 "made by a probe")'),
    _ini("so.create.float.read", f'SetFunctionValue (GetINIFloatOrDefault_Cached "General:fMade" {_PI} 9)'),
    _ini("so.create.float.lu", f'SetFunctionValue (GetINIFloat "General:fMade" {_PI})'),
    _ini("so.create.float.existing", f'SetFunctionValue (GetINIFloatOrCreate_Cached "General:fNum" {_PI} 4.5)'),
    _ini("so.create.string", f'SetFunctionValue (GetINIStringOrCreate_Cached "General:sMade" {_PI} "new" "")', True),
    _ini("so.create.string.read", f'SetFunctionValue (GetINIStringOrDefault_Cached "General:sMade" {_PI} "dflt")', True),
    # LU writes: an existing file, then a new one.
    _ini("lu.set", f'SetINIFloat "General:fSet" 2.25 {_PI}\nSetFunctionValue (GetINIFloat "General:fSet" {_PI})'),
    _ini("lu.set.existing", f'SetINIFloat "Other:fNum" 6 {_PI}\nSetFunctionValue (GetINIFloat "Other:fNum" {_PI})'),
    _ini("lu.set.cached", f'SetFunctionValue (GetINIFloatOrDefault_Cached "Other:fNum" {_PI} 9)'),
    _ini("lu.set.newfile", f'SetINIFloat "New:fVal" 1.5 {_NEW}\nSetFunctionValue (GetINIFloat "New:fVal" {_NEW})'),
    _ini("lu.set.newfile.cached", f'SetFunctionValue (GetINIFloatOrDefault_Cached "New:fVal" {_NEW} 9)'),
    # MCM.
    _ini("mcm.get", 'SetFunctionValue (GetModINISetting "NVFExtProbe/General/fNum")'),
    _ini("mcm.get.missing", 'SetFunctionValue (GetModINISetting "NVFExtProbe/General/fNone")'),
    _ini("mcm.set", 'SetModINISetting "NVFExtProbe/General/fMcm" 5\nSetFunctionValue (GetModINISetting "NVFExtProbe/General/fMcm")'),
    _ini("mcm.set.lu", 'SetFunctionValue (GetINIFloat "General:fMcm" "NVFExtProbe.ini")'),
]

# Twenty-ninth set (next batch N7): JIP and LU actor and game odds. Forms from fnvtool
# esm inventory [observed]: Water2 161902, Rad1 0335A2, MaleAdult11 173561, Toughness
# 031DE0, Chet 104C79, the dead ACRE near the airport 14DEE2, Doc's house 103DF9.
# Settings changed are put back where a getter can read the old value; the player is
# moved into Doc's house last. Session only, never saved. Left out: Disable/
# EnableNavMeshAlt (navmesh edits) and GetProjectileRefImpactRef (needs a live
# projectile inside an impact handler).
FX.update(water2="161902", rad1="335A2", maleadult11="173561", toughness="31DE0")
_H9 = ("ref rPlayer\nref rDoc\nref rBase\nref rObj\nref rOther\nref rF\nfloat fRes\nstring_var sText\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + _form("rBase", NF["doc"]))


def _n7(pid, body, text=False):
    return (pid, _H9 + body, text, None, "")


def _n7v(pid, expr, text=False):
    return _n7(pid, f"SetFunctionValue ({expr})", text)


def _n7id(pid, expr, pre=""):
    return _n7(pid, pre + f"SetFunctionValue (GetFormIDString ({expr}))", True)


_POS = 'SetFunctionValue (sv_Construct "%.0f %.0f %.0f" (rObj.GetPos X) (rObj.GetPos Y) (rObj.GetPos Z))'

N7_PROBES = [
    # Keywords.
    _n7v("kw.none", 'HasKeyword rBase "NVFKey"'),
    _n7("kw.assign", 'AssignKeyword rBase "NVFKey"\nSetFunctionValue (HasKeyword rBase "NVFKey")'),
    _n7v("kw.ref", 'HasKeyword rDoc "NVFKey"'),
    _n7v("kw.case", 'HasKeyword rBase "nvfkey"'),
    _n7v("kw.other", 'HasKeyword rBase "NVFOther"'),
    _n7(f"kw.list", _o4("list127") + 'AssignKeyword rObj "NVFListKey"\nlet rOther := ListGetNthForm rObj 0\n'
        'SetFunctionValue (HasKeyword rOther "NVFListKey")'),
    _n7(f"kw.list.self", _o4("list127") + 'SetFunctionValue (HasKeyword rObj "NVFListKey")'),
    # Hardcore stages and tracking.
    _n7(f"hc.water2", _o4("water2") + "SetFunctionValue (GetHardcoreStageThreshold rObj)"),
    _n7(f"hc.rad1", _o4("rad1") + "SetFunctionValue (GetHardcoreStageThreshold rObj)"),
    _n7(f"hc.water2.set", _o4("water2") + "let fRes := GetHardcoreStageThreshold rObj\nrPlayer.AuxVarSetFlt \"*nvfHC\" fRes\n"
        "SetHardcoreStageThreshold rObj 321\nSetFunctionValue (GetHardcoreStageThreshold rObj)"),
    _n7(f"hc.water2.restore", _o4("water2") + 'SetHardcoreStageThreshold rObj (rPlayer.AuxVarGetFlt "*nvfHC")\n'
        'rPlayer.AuxVarErase "*nvfHC"\nSetFunctionValue (GetHardcoreStageThreshold rObj)'),
    _n7(f"hc.notstage", _o4("pistol10") + "SetFunctionValue (GetHardcoreStageThreshold rObj)"),
    _n7("hc.tracking.off", "SetFunctionValue (ToggleHardcoreTracking 0)"),
    _n7("hc.tracking.on", "SetFunctionValue (ToggleHardcoreTracking 1)"),
    # Voice type.
    _n7id("voice.ref", "rDoc.GetActorVoiceType"),
    _n7id("voice.base", "GetActorVoiceType rBase"),
    _n7("voice.set", _o4("maleadult11") + "let rOther := rDoc.GetActorVoiceType\nrPlayer.AuxVarSetRef \"*nvfVT\" rOther\n"
        "rDoc.SetActorVoiceType rObj\nSetFunctionValue (GetFormIDString (rDoc.GetActorVoiceType))", True),
    _n7id("voice.set.base", "GetActorVoiceType rBase"),
    _n7("voice.restore", 'let rOther := rPlayer.AuxVarGetRef "*nvfVT"\nrDoc.SetActorVoiceType rOther\nrPlayer.AuxVarErase "*nvfVT"\n'
        "SetFunctionValue (GetFormIDString (rDoc.GetActorVoiceType))", True),
    # Base karma.
    _n7v("karma.base", "GetBaseKarma rBase"),
    _n7v("karma.ref", "rDoc.GetBaseKarma"),
    _n7("karma.save", 'let fRes := GetBaseKarma rBase\nrPlayer.AuxVarSetFlt "*nvfKA" fRes\nSetFunctionValue (rPlayer.AuxVarGetFlt "*nvfKA")'),
    _n7("karma.set", "SetBaseKarma 37.5 rBase\nSetFunctionValue (GetBaseKarma rBase)"),
    _n7("karma.set.int", "SetBaseKarma 37 rBase\nSetFunctionValue (GetBaseKarma rBase)"),
    _n7("karma.set.calling", "rBase.SetBaseKarma 21\nSetFunctionValue (GetBaseKarma rBase)"),
    _n7("karma.set.ref", "rDoc.SetBaseKarma -12\nSetFunctionValue (GetBaseKarma rBase)"),
    _n7("karma.restore", 'SetBaseKarma (rPlayer.AuxVarGetFlt "*nvfKA") rBase\nrPlayer.AuxVarErase "*nvfKA"\nSetFunctionValue (GetBaseKarma rBase)'),
    # Perk entries.
    _n7(f"perk.v1.0", _o4("toughness") + "SetFunctionValue (GetNthPerkEntryValue1 rObj 0)"),
    _n7(f"perk.v1.past", _o4("toughness") + "SetFunctionValue (GetNthPerkEntryValue1 rObj 99)"),
    _n7(f"perk.v1.set", _o4("toughness") + 'let fRes := GetNthPerkEntryValue1 rObj 0\nrPlayer.AuxVarSetFlt "*nvfPK" fRes\n'
        "SetNthPerkEntryValue1 rObj 0 9\nSetFunctionValue (GetNthPerkEntryValue1 rObj 0)"),
    _n7(f"perk.v1.restore", _o4("toughness") + 'SetNthPerkEntryValue1 rObj 0 (rPlayer.AuxVarGetFlt "*nvfPK")\nrPlayer.AuxVarErase "*nvfPK"\n'
        "SetFunctionValue (GetNthPerkEntryValue1 rObj 0)"),
] + [_n7v(f"perkmod.{f}", f"rPlayer.GetPerkModifier {f} 100") for f in (0, 1, 2, 10, 37)] + [
    # Names.
    _n7v("name.before", "GetName rBase", True),
    _n7("name.set", 'LNSetName "NVF Doc" rBase\nSetFunctionValue (GetName rBase)', True),
    _n7v("name.set.ref", "rDoc.GetName", True),
    _n7("name.set.byref", 'LNSetName "NVF Doc Ref" rDoc\nSetFunctionValue (GetName rBase)', True),
    _n7("name.restore", 'LNSetName "Doc Mitchell" rBase\nSetFunctionValue (GetName rBase)', True),
    # Message.
    _n7("n7.messageexalt", 'MessageExAlt 2 "NVF probe message"\nSetFunctionValue 1'),
    # Water, air.
    _n7v("inwater.player", "rPlayer.IsInWater"),
    _n7v("n7.inair.player", "rPlayer.IsInAir"),
    _n7v("inwater.doc", "rDoc.IsInWater"),
    _n7v("inair.doc", "rDoc.IsInAir"),
    # Killer.
    _n7(f"killer.acre", _form("rF", "14DEE2") + "SetFunctionValue (GetFormIDString (rF.GetKiller))", True),
    _n7id("killer.alive", "rDoc.GetKiller"),
    _n7("killer.placed", _form("rObj", "14DEE2") + 'let rObj := rObj.GetBaseObject\nlet rF := rPlayer.PlaceAtMe rObj 1\n'
        'rPlayer.AuxVarSetRef "*nvfKL" rF\nrF.Kill rPlayer\nSetFunctionValue 1'),
    _n7("killer.placed.next", 'let rF := rPlayer.AuxVarGetRef "*nvfKL"\nSetFunctionValue (sv_Construct "%g " (rF.GetDead) + GetFormIDString (rF.GetKiller))', True),
    _n7("killer.placed.nokiller", _form("rObj", "14DEE2") + 'let rObj := rObj.GetBaseObject\nlet rF := rPlayer.PlaceAtMe rObj 1\n'
        'rPlayer.AuxVarSetRef "*nvfKL2" rF\nrF.Kill\nSetFunctionValue 1'),
    _n7("killer.placed.nokiller.next", 'let rF := rPlayer.AuxVarGetRef "*nvfKL2"\nSetFunctionValue (GetFormIDString (rF.GetKiller))', True),
    _n7("killer.cleanup", 'let rF := rPlayer.AuxVarGetRef "*nvfKL"\nrF.Disable\nrF.MarkForDelete\nlet rF := rPlayer.AuxVarGetRef "*nvfKL2"\n'
        'rF.Disable\nrF.MarkForDelete\nrPlayer.AuxVarErase "*nvfKL"\nrPlayer.AuxVarErase "*nvfKL2"\nSetFunctionValue 1'),
    # Speed multiplier.
    _n7v("speed.before", 'rDoc.GetActorValue "SpeedMult"'),
    _n7("speed.set", 'rDoc.SetSpeedMult 150\nSetFunctionValue (rDoc.GetActorValue "SpeedMult")'),
    _n7v("speed.set.next", 'rDoc.GetActorValue "SpeedMult"'),
    _n7("speed.restore", 'rDoc.SetSpeedMult 100\nSetFunctionValue (rDoc.GetActorValue "SpeedMult")'),
    # Linked reference.
    _n7id("linked.before", "rDoc.GetLinkedRef"),
    _n7id("linked.set", "rDoc.GetLinkedRef", "rDoc.SetLinkedReference rPlayer\n"),
    _n7id("linked.reset", "rDoc.GetLinkedRef", "rDoc.SetLinkedReference\n"),
    # UnequipItemAlt.
    _n7("unequipalt.add", _o4("pistol10") + "rPlayer.AddItem rObj 1 1\nSetFunctionValue 1"),
    _n7("unequipalt.equip", _o4("pistol10") + "rPlayer.EquipItem rObj 0 1\nSetFunctionValue 1"),
    _n7v("unequipalt.equipped", "rPlayer.GetEquipped " + "(GetFormFromMod \"FalloutNV.esm\" \"434F\")"),
    _n7("unequipalt", _o4("pistol10") + "rPlayer.UnequipItemAlt rObj 0 1\nSetFunctionValue (rPlayer.GetEquipped rObj)"),
    _n7("unequipalt.next", _o4("pistol10") + "SetFunctionValue (rPlayer.GetEquipped rObj)"),
    _n7("unequipalt.cleanup", _o4("pistol10") + "rPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1"),
    # Reference flags (LU).
] + [_n7v(f"refflag.doc.{f}", f"rDoc.GetReferenceFlag {f}") for f in (1, 64, 1024, 2048, 524288)] + [
    _n7(f"refflag.chet.64", _o4("chetref") + "SetFunctionValue (rObj.GetReferenceFlag 64)"),
    _n7(f"refflag.door.set", _o4("saloondoor") + "rObj.SetReferenceFlag 128 1\nSetFunctionValue (GetFlagsLow rObj)"),
    _n7(f"refflag.door.get", _o4("saloondoor") + "SetFunctionValue (rObj.GetReferenceFlag 128)"),
    _n7(f"refflag.door.clear", _o4("saloondoor") + "rObj.SetReferenceFlag 128 0\nSetFunctionValue (GetFlagsLow rObj)"),
    # CastImmediate: a stimpak on the damaged player.
    _n7("cast.damage", 'rPlayer.DamageActorValue "Health" 40\nSetFunctionValue (rPlayer.GetActorValue "Health")'),
    _n7("cast.stimpak", _o4("stimpak") + 'rPlayer.CastImmediate rObj\nSetFunctionValue (rPlayer.GetActorValue "Health")'),
    _n7v("cast.next", 'rPlayer.GetActorValue "Health"'),
    _n7v("cast.next2", 'rPlayer.GetActorValue "Health"'),
    _n7("cast.heal", 'rPlayer.RestoreActorValue "Health" 1000\nSetFunctionValue (rPlayer.GetActorValue "Health")'),
    # MoveToEditorPosition on Chet.
    _n7("editorpos.before", _o4("chetref") + _POS, True),
    _n7("editorpos.moved", _o4("chetref") + "rObj.MoveTo rPlayer\nSetFunctionValue 1"),
    _n7("editorpos.moved.pos", _o4("chetref") + 'SetFunctionValue (GetFormIDString (rObj.GetParentCell))', True),
    _n7("editorpos.reset", _o4("chetref") + "SetFunctionValue (rObj.MoveToEditorPosition 1)"),
    _n7("editorpos.after", _o4("chetref") + _POS, True),
    _n7("editorpos.after.cell", _o4("chetref") + 'SetFunctionValue (GetFormIDString (rObj.GetParentCell))', True),
    _n7v("editorpos.player", "rPlayer.MoveToEditorPosition 0"),
    # MoveToCell: the player into Doc's house (last).
    _n7("movetocell", _form("rF", CELLS["doc"]) + "SetFunctionValue (rPlayer.MoveToCell rF 0 0 0)"),
    _n7id("movetocell.cell", "rPlayer.GetParentCell"),
    _n7id("movetocell.cell2", "rPlayer.GetParentCell"),
]

# Thirtieth set: the player-state getters the N10 sweep ranks first (GetWaterImmersionPerc,
# IsInKillCam, GetPCUsingScope/IronSights, GetAutoMove, GetActorVelocity, IsIdlePlayingEx)
# and LU's controller functions (next batch N8), standing still at the airport with no
# gamepad connected. Idle NVPHMCHouseIdle 176EE1 from fnvtool esm inventory [observed].
# DisableButton is undone with EnableButton.
FX.update(idlehouse="176EE1")


def _ps(pid, body, text=False):
    return (pid, _H9 + body, text, None, "")


def _psv(pid, expr, text=False):
    return _ps(pid, f"SetFunctionValue ({expr})", text)


PLAYER_STATE_PROBES = (
    [_psv("ps.water.player", "rPlayer.GetWaterImmersionPerc"),
     _psv("ps.water.doc", "rDoc.GetWaterImmersionPerc"),
     _psv("ps.killcam", "IsInKillCam"),
     _psv("ps.scope", "GetPCUsingScope"),
     _psv("ps.ironsights", "GetPCUsingIronSights"),
     _psv("ps.automove", "GetAutoMove"),
     _psv("ps.swimming", "IsPlayerSwimming"),
     _psv("ps.chargen", "IsInCharGen")]
    + [_psv(f"ps.velocity.player.{a or 'all'}", f"rPlayer.GetActorVelocity {a}".rstrip()) for a in ("", "X", "Y", "Z")]
    + [_psv("ps.velocity.doc", "rDoc.GetActorVelocity"),
       _ps("ps.idle.player", _o4("idlehouse") + "SetFunctionValue (rPlayer.IsIdlePlayingEx rObj)"),
       _ps("ps.idle.doc", _o4("idlehouse") + "SetFunctionValue (rDoc.IsIdlePlayingEx rObj)"),
       # Controller (no pad connected).
       _psv("pad.controller", "GetController"),
       _psv("pad.pressed", "GetPressedButtons"),
       _psv("pad.lt", "GetLeftTrigger"),
       _psv("pad.rt", "GetRightTrigger"),
       _psv("pad.lsx", "GetLeftStickX"),
       _psv("pad.lsy", "GetLeftStickY"),
       _psv("pad.rsx", "GetRightStickX"),
       _psv("pad.rsy", "GetRightStickY"),
       _psv("pad.button.a", "IsButtonPressed 4096"),
       _psv("pad.disabled.before", "IsButtonDisabled 4096"),
       _ps("pad.disable", "DisableButton 4096\nSetFunctionValue (IsButtonDisabled 4096)"),
       _psv("pad.disable.next", "IsButtonDisabled 4096"),
       _psv("pad.disable.other", "IsButtonDisabled 8192"),
       _ps("pad.disable.two", "DisableButton 12288\nSetFunctionValue ((IsButtonDisabled 4096) * 10 + (IsButtonDisabled 8192))"),
       _ps("pad.enable.one", "EnableButton 8192\nSetFunctionValue ((IsButtonDisabled 4096) * 10 + (IsButtonDisabled 8192))"),
       _ps("pad.enable", "EnableButton 4096\nSetFunctionValue (IsButtonDisabled 4096)")]
)

# Thirty-first set (next batch N2): MCM's script functions, in a load order of its own
# (the usual eleven plugins plus "The Mod Configuration Menu.esp" last; every other set
# keeps the usual order). In play no menu is open, so StartMenu (the pause menu, where
# MCM's tiles live) is not loaded: these rows are what a mod's script sees outside the
# menu. MCM.ini (Data\Config, made by MCM.dll) is read through GetModINISetting.
MCM_LOAD_ORDER_EXTRA = "The Mod Configuration Menu.esp"


def _mc(pid, body, text=False):
    return (pid, "float fRes\nstring_var sText\nBegin Function { }\n" + body, text, None, "")


MCM_PROBES = [
    _mc("mcm.loaded", 'SetFunctionValue (IsModLoaded "The Mod Configuration Menu.esp")'),
    _mc("mcm.plugin.version", 'SetFunctionValue (GetPluginVersion "MCM Extensions")'),
    _mc("mcm.ini.maxwidth", 'SetFunctionValue (GetModINISetting "MCM/List/MaxWidth")'),
    _mc("mcm.ini.initial", 'SetFunctionValue (GetModINISetting "MCM/Messages/Initial")'),
    _mc("mcm.ini.missing", 'SetFunctionValue (GetModINISetting "MCM/List/NVFNone")'),
    _mc("mcm.startmenu.visible", 'SetFunctionValue (GetUIFloat "StartMenu/visible")'),
    _mc("mcm.tile.visible", 'SetFunctionValue (GetUIFloat "StartMenu/MCM/visible")'),
    _mc("mcm.getfloat.1.1", 'SetFunctionValue (GetMCMFloat 1 1 "_enable")'),
    _mc("mcm.getfloat.0.0", 'SetFunctionValue (GetMCMFloat 0 0 "visible")'),
    _mc("mcm.setfloat", 'SetMCMFloat 1 1 "_nvfprobe" 5\nSetFunctionValue (GetMCMFloat 1 1 "_nvfprobe")'),
    _mc("mcm.setfloat.ui", 'SetFunctionValue (GetUIFloat "StartMenu/MCM/*:1/*:1/_nvfprobe")'),
    _mc("mcm.setstring", 'SetMCMString 1 1 "_title" "NVF probe"\nSetFunctionValue 1'),
    _mc("mcm.menumode", "SetFunctionValue (MenuMode 1013)"),
]

# Thirty-second set (next batch N9): JohnnyGuitar, ShowOff and ITR leftovers. ITR's
# plugin is not installed in this game, so its rows show what a script gets without it.
# Left out: SendStealingAlarm (karma loss and hostile owners). Extra misc stats and aux
# timers live on the player for the session only, never saved.
def _n9(pid, body, text=False):
    return (pid, _H9.replace("string_var sText\n", "string_var sText\narray_var aF\n") + body, text, None, "")


def _n9v(pid, expr, text=False):
    return _n9(pid, f"SetFunctionValue ({expr})", text)


_BASEFLAGS = ("let fRes := 0\nlet sText := \"\"\nwhile fRes < 32\n  if ActorHasBaseFlag fRes rObj\n"
              "    let sText += sv_Construct \"%g \" fRes\n  endif\n  let fRes += 1\nloop\nSetFunctionValue sText")

N9_PROBES = [
    _n9v("jg.pipboy", "GetPipBoyMode"),
    _n9v("jg.lerp", "Lerp 0 10 0.25"),
    _n9v("jg.lerp.over", "Lerp 0 10 1.5"),
    _n9v("jg.lerp.under", "Lerp 0 10 -0.5"),
    _n9v("jg.lerp.desc", "Lerp 10 0 0.25"),
    _n9v("jg.miscstat.none", 'GetExtraMiscStat "NVFStat"'),
    _n9("jg.miscstat.mod", 'ModExtraMiscStat "NVFStat" 3\nSetFunctionValue (GetExtraMiscStat "NVFStat")'),
    _n9("jg.miscstat.mod2", 'ModExtraMiscStat "NVFStat" -1\nSetFunctionValue (GetExtraMiscStat "NVFStat")'),
    _n9v("jg.miscstat.case", 'GetExtraMiscStat "nvfstat"'),
    _n9("jg.camshake", "SetCameraShakeNoHUDShudder 0.5 0.1\nSetFunctionValue 1"),
    _n9(f"jg.armoralt.clear", _o4("leather") + "SetFunctionValue (ClearArmorAltTexture rObj 1 -1)"),
    _n9(f"jg.armoralt.clear.idx", _o4("leather") + "SetFunctionValue (ClearArmorAltTexture rObj 1 0)"),
    _n9(f"jg.weap1st.get", _o4("pistol10") + "SetFunctionValue (GetFormIDString (GetWeapon1stPersonModel rObj 0))", True),
    _n9(f"jg.weap1st.same", _o4("pistol10") + "let rOther := GetWeapon1stPersonModel rObj 0\nSetWeapon1stPersonModel rObj 0 rOther\n"
        "SetFunctionValue (GetFormIDString (GetWeapon1stPersonModel rObj 0))", True),
    # ShowOff: actor base flags, as the list of set flag indices 0-31.
    _n9("so.baseflags.doc", "let rObj := rBase\n" + _BASEFLAGS, True),
    _n9("so.baseflags.player", _form("rObj", "7") + _BASEFLAGS, True),
    _n9("so.baseflags.acre", _form("rObj", "14DEE2") + "let rObj := rObj.GetBaseObject\n" + _BASEFLAGS, True),
    _n9v("so.baseflag.ref", "rDoc.ActorHasBaseFlag 1"),
    # ShowOff aux timers (owner: the player when none is given).
    _n9v("so.timer.left.none", 'AuxTimerTimeLeft "NVFT"'),
    _n9v("so.timer.start", 'AuxTimerStart "NVFT" 5'),
    _n9v("so.timer.left", 'Ceil (AuxTimerTimeLeft "NVFT")'),
    _n9v("so.timer.elapsed", 'Ceil (AuxTimerTimeElapsed "NVFT" 0)'),
    _n9v("so.timer.pause", 'AuxTimerPaused "NVFT" 1'),
    _n9v("so.timer.paused", 'AuxTimerPaused "NVFT" 0'),
    _n9v("so.timer.restart", 'AuxTimerStart "NVFT" 7'),
    _n9v("so.timer.left.restart", 'Ceil (AuxTimerTimeLeft "NVFT")'),
    _n9v("so.timer.stop", 'AuxTimerStop "NVFT" 0'),
    _n9v("so.timer.left.stopped", 'AuxTimerTimeLeft "NVFT"'),
    _n9v("so.timer.stop.again", 'AuxTimerStop "NVFT" 0'),
    _n9v("so.timer.doc.start", 'rDoc.AuxTimerStart "NVFT" 3'),
    _n9v("so.timer.doc.left.player", 'AuxTimerTimeLeft "NVFT"'),
    _n9v("so.timer.doc.left", 'Ceil (rDoc.AuxTimerTimeLeft "NVFT")'),
    _n9v("so.timer.doc.stop", 'rDoc.AuxTimerStop "NVFT" 0'),
    _n9v("so.filecache.clear", 'ClearFileCacheShowOff "NVFExtProbe\\probe.ini" 1'),
    # Files (read-only, relative to Data).
    _n9v("file.esp", 'FileExists "NVFExtProbe.esp"'),
    _n9v("file.bsa", 'FileExists "meshes\\weapons\\1handpistol\\10mmpistol.nif"'),
    _n9v("file.none", 'FileExists "nvf\\none.nif"'),
    _n9v("file.folder", 'FileExists "NVSE" 1'),
    _n9v("file.folder.asfile", 'FileExists "NVSE"'),
    _n9("files.plugins", 'let aF := GetFilesInFolder "NVSE\\Plugins" "*.dll"\n'
        'SetFunctionValue (sv_Construct "%g " (ar_Size aF) + aF[0])', True),
    _n9("files.plugins.all", 'let aF := GetFilesInFolder "NVSE\\Plugins"\nSetFunctionValue (ar_Size aF)'),
    _n9("files.none", 'let aF := GetFilesInFolder "NVFNoFolder"\nSetFunctionValue (ar_Size aF)'),
    # ITR (not installed).
    _n9v("itr.crouch", "rDoc.ForceCrouch 1"),
    _n9v("itr.target", "rDoc.ForceCombatTarget rPlayer"),
]

# Thirty-third set (lu-leveled follow-up): which entries LeveledListAddForm takes, and
# whether an edited list reaches AddItem, a newly placed container and ResetInventory.
# Forms from fnvtool esm inventory [observed]: Ammo556mm 004240, Caps001 00000F,
# VStreetCrBrahmin2 (CREA) 17B587, VarVHDNCRRanger8Veteran (LVLN) 17A2FD,
# VLootAmmoBoxGen30 (CONT) 167815 holding 02FE5D. Lists edited for the session only.
FX.update(ammo556="4240", brahmin="17B587", lvln2="17A2FD", ammobox="167815", boxlist="2FE5D")


def _l2(pid, lst, body, text=False):
    head = _HL.replace("float fRes\n", "float fRes\nstring_var sText\n")
    return (pid, head + _form("rLst", FX[lst]) + body, text, None, "")


_NUM = "SetFunctionValue (GetNumLevItems rLst)"

LEVELED2_PROBES = [
    # AddItem of an unedited list (does it give anything at all?).
    _l2("additem.plain.before", "fortune", "SetFunctionValue (rPlayer.GetNumItems)"),
    _l2("additem.plain", "fortune", "rPlayer.AddItem rLst 1 1\nSetFunctionValue 1"),
    _l2("additem.plain.after", "fortune", "SetFunctionValue (rPlayer.GetNumItems)"),
    _l2("additem.ammo10.before", "lvli", _form("rObj", "4241") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _l2("additem.ammo10.cn0", "lvli", "SetChanceNone rLst 0\nrPlayer.AddItem rLst 1 1\nSetFunctionValue 1"),
    _l2("additem.ammo10.after", "lvli", _form("rObj", "4241") + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    # What an unedited LVLI takes.
    _l2("type.lvli.same", "lvli", _form("rObj", FX["ammo556"]) + "LeveledListAddForm rLst rObj 1 1 1.0\n" + _NUM),
    _l2("type.lvli.same.index", "lvli", _form("rObj", FX["ammo556"]) + "SetFunctionValue (GetLevItemIndexByForm rLst rObj)"),
    _l2("type.lvli.caps.simple", "lvli", _form("rObj", FX["capsf"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("type.lvli.caps.health", "lvli", _form("rObj", FX["capsf"]) + "LeveledListAddForm rLst rObj 1 1 1.0\n" + _NUM),
    _l2("type.lvli.stim", "lvli", _form("rObj", FX["stim"]) + "LeveledListAddForm rLst rObj 1 1 1.0\n" + _NUM),
    _l2("type.lvli.list", "lvli", _form("rObj", FX["fortune"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("player.level", "lvli", "SetFunctionValue (rPlayer.GetLevel)"),
    _l2("type.lvli.atlevel", "lvli", _form("rOther", FX["ammo556"]) + "LeveledListAddForm rLst rOther (rPlayer.GetLevel) 1 1.0\n" + _NUM),
    _l2("type.lvli.abovelevel", "lvli", _form("rOther", FX["ammo556"]) + "LeveledListAddForm rLst rOther ((rPlayer.GetLevel) + 1) 1 1.0\n" + _NUM),
    _l2("type.lvli.level2", "lvli", _form("rObj", FX["ammo556"]) + "LeveledListAddForm rLst rObj 2 1 1.0\n" + _NUM),
    _l2("type.lvli.level7", "lvli", _form("rObj", FX["ammo556"]) + "LeveledListAddForm rLst rObj 7 1 1.0\n" + _NUM),
    _l2("type.lvli.count25", "lvli", _form("rObj", FX["ammo556"]) + "LeveledListAddForm rLst rObj 1 25 1.0\n" + _NUM),
    _l2("type.lvli.order", "lvli", "let fRes := 0\nlet sText := \"\"\nwhile fRes < (GetNumLevItems rLst)\n"
        "  let sText += (GetFormIDString (GetNthLevItem rLst fRes)) + (sv_Construct \":%g:%g \" (GetNthLevItemLevel rLst fRes) (GetNthLevItemCount rLst fRes))\n"
        "  let fRes += 1\nloop\nSetFunctionValue sText", True),
    # LVLC and LVLN.
    _l2("type.lvlc.crea", "lvlc", _form("rObj", FX["brahmin"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("type.lvlc.list", "lvlc", _form("rObj", FX["lvlc2"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("type.lvlc.npc", "lvlc", _form("rObj", FX["docnpc"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("type.lvln.list", "lvln", _form("rObj", FX["lvln2"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    _l2("type.lvln.crea", "lvln", _form("rObj", FX["brahmin"]) + "LeveledListAddForm rLst rObj 1 1 0\n" + _NUM),
    # A container from the edited list: VLootAmmoBoxGen30 holds 02FE5D.
    _l2("box.list.num", "boxlist", _NUM),
    _l2("box.list.rebuild", "boxlist", _form("rObj", FX["stim"])
        + "LeveledListClear rLst\nSetChanceNone rLst 0\nSetLeveledListFlags rLst 0\nLeveledListAddForm rLst rObj 1 3 1.0\n" + _NUM),
    _l2("box.place", "boxlist", _form("rObj", FX["ammobox"]) + 'let rOther := rPlayer.PlaceAtMe rObj 1\nrPlayer.AuxVarSetRef "*nvfBX" rOther\nSetFunctionValue 1'),
    _l2("box.stim", "boxlist", _form("rObj", FX["stim"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nSetFunctionValue (rOther.GetItemCount rObj)'),
    _l2("box.items", "boxlist", 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nSetFunctionValue (rOther.GetNumItems)'),
    _l2("box.reset.rebuild", "boxlist", _form("rObj", FX["stim"]) + "LeveledListClear rLst\nLeveledListAddForm rLst rObj 1 5 1.0\n" + _NUM),
    _l2("box.reset", "boxlist", 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nrOther.ResetInventory\nSetFunctionValue 1'),
    _l2("box.reset.stim", "boxlist", _form("rObj", FX["stim"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nSetFunctionValue (rOther.GetItemCount rObj)'),
    _l2("box.reset.stim.next", "boxlist", _form("rObj", FX["stim"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nSetFunctionValue (rOther.GetItemCount rObj)'),
    _l2("box.additem", "boxlist", _form("rObj", FX["stim"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nrOther.AddItem rLst 1 1\n'
        "SetFunctionValue (rOther.GetItemCount rObj)"),
    _l2("box.additem.next", "boxlist", _form("rObj", FX["stim"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nSetFunctionValue (rOther.GetItemCount rObj)'),
    _l2("player.additem.boxlist.before", "boxlist", _form("rObj", FX["stim"]) + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _l2("player.additem.boxlist", "boxlist", "rPlayer.AddItem rLst 1 1\nSetFunctionValue 1"),
    _l2("player.additem.boxlist.after", "boxlist", _form("rObj", FX["stim"]) + "SetFunctionValue (rPlayer.GetItemCount rObj)"),
    _l2("player.additem.boxlist.cleanup", "boxlist", _form("rObj", FX["stim"]) + "rPlayer.RemoveItem rObj 5 1\nSetFunctionValue (rPlayer.GetItemCount rObj)"),
    _l2("box.cleanup", "boxlist", 'let rOther := rPlayer.AuxVarGetRef "*nvfBX"\nrOther.Disable\nrOther.MarkForDelete\nrPlayer.AuxVarErase "*nvfBX"\nSetFunctionValue 1'),
]

# Thirty-fourth set: JIP's live-projectile functions, read inside a
# SetOnProjectileImpactEventHandler lambda on the 10mm pistol's projectile (no arguments;
# the projectile is the calling reference, GetSelf [documented]). The player fires the
# pistol with FireWeapon (no need to equip it [documented: FireWeapon]); each impact
# appends its values to `*` auxiliary variables on the player. The handler dies with the
# session; the last step erases the variables. The navmesh functions are left out: no
# script function reads whether a navmesh is disabled.
PROJ_VALUES = [
    ("count", "1"),
    ("impact.isref", "IsReference (GetSelf.GetProjectileRefImpactRef)"),
    ("impact.type", "GetType ((GetSelf.GetProjectileRefImpactRef).GetBaseObject)"),
    ("source.player", "(GetSelf.GetProjectileRefSource) == rP"),
    ("weapon.ispistol", "(GetSelf.GetProjectileRefWeapon) == rGun"),
    ("damage", "GetSelf.GetProjectileRefDamage"),
    ("self.type", "GetType (GetSelf.GetBaseObject)"),
]


def _pj(pid, body, text=False):
    return (pid, "ref rP\nref rGun\nref rProj\nfloat fRes\nBegin Function { }\n" + _form("rP", NF["player"])
            + _form("rGun", NF["pistol10"]) + body, text, None, "")


def _proj_steps():
    steps = [_pj("proj.form", "SetFunctionValue (GetFormIDString (GetWeaponProjectile rGun))", True)]
    # A first handler, then the one that reads everything: whether the second replaces
    # the first shows in proj.first.size (one handler per projectile was seen when each
    # value had its own handler).
    steps.append(_pj("proj.reg.first", "let rProj := GetWeaponProjectile rGun\n"
                     'SetOnProjectileImpactEventHandler (Begin Function { }\n  rP.AuxVarSetFlt "*nvfPJ.first" 1 -1\nEnd) 1 rProj\n'
                     "SetFunctionValue 1"))
    body = "".join(f'  rP.AuxVarSetFlt "*nvfPJ.{name}" ({expr}) -1\n' for name, expr in PROJ_VALUES
                   if name in ("count", "impact.isref", "source.player", "self.type"))
    steps.append(_pj("proj.reg", "let rProj := GetWeaponProjectile rGun\n"
                     f"SetOnProjectileImpactEventHandler (Begin Function {{ }}\n{body}End) 1 rProj\nSetFunctionValue 1"))
    # The rest in a handler of its own: a lambda of all seven lines does not compile in
    # the game, four do.
    body2 = "".join(f'  rP.AuxVarSetFlt "*nvfPJ.{name}" ({expr}) -1\n' for name, expr in PROJ_VALUES
                    if name in ("impact.type", "weapon.ispistol", "damage"))
    steps.append(_pj("proj.reg2", "let rProj := GetWeaponProjectile rGun\n"
                     f"SetOnProjectileImpactEventHandler (Begin Function {{ }}\n{body2}End) 1 rProj\nSetFunctionValue 1"))
    # Look down so the shot lands near the player within a few frames.
    steps.append(_pj("proj.aim", "rP.SetAngle X 80\nSetFunctionValue (rP.GetAngle X)"))
    steps.append(_pj("proj.fire", "rP.FireWeapon rGun\nSetFunctionValue 1"))
    steps += [_pj(f"proj.settle{i}", "SetFunctionValue 1") for i in range(10)]
    for name, _ in [("first", "")] + PROJ_VALUES:
        steps.append(_pj(f"proj.{name}.size", f'SetFunctionValue (rP.AuxVarSize "*nvfPJ.{name}")'))
        steps.append(_pj(f"proj.{name}.0", f'SetFunctionValue (rP.AuxVarGetFlt "*nvfPJ.{name}" 0)'))
    steps.append(_pj("proj.cleanup", "".join(f'rP.AuxVarErase "*nvfPJ.{n}"\n' for n, _ in [("first", "")] + PROJ_VALUES)
                     + "SetFunctionValue 1"))
    return steps


PROJECTILE_PROBES = _proj_steps()

# Thirty-fifth set (next batch N4): JIP's sound, weather and image-space traits. Trait
# ID ranges from the function pages [documented: GetSoundTraitNumeric 0-12,
# GetWeatherTraitNumeric 0-20, GetWeatherRGBColor types 0-9 / times 0-5 / layers 0-3,
# GetImageSpaceTrait 0-32, GetImageSpaceModTrait 0-67]. Forms from fnvtool esm inventory
# [observed]: EMTMedicalBeep01 (SOUN) 174325, HazmatImageSpace (IMGS) 0542F2,
# FadeToBlack9sISFX (IMAD) 173904. The weather rows name the weather the save has current
# at the airport (wth.current: 00171285) directly, so they test the trait functions and
# not which weather the save happens to have (a headless run cannot know that). A string_var as SetSoundSourceFile's path does not compile ($sText), so
# the sound's path is put back as a literal. Each setter's row is followed by a row putting the old value back (session
# only, never saved).
FX.update(beep="174325", hazmat="542F2", fadeblack="173904")


def _sk(pid, body, text=False):
    return (pid, "ref rPlayer\nref rObj\nref rW8\nfloat fRes\nstring_var sText\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + body, text, None, "")


def _skv(pid, pre, expr, text=False):
    return _sk(pid, pre + f"SetFunctionValue ({expr})", text)


_SND = _form("rObj", FX["beep"])
_IMGS = _form("rObj", FX["hazmat"])
_IMAD = _form("rObj", FX["fadeblack"])
_WTH = _form("rW8", "171285")

SKY_SOUND_PROBES = (
    # Sound.
    [_skv(f"snd.trait.{i}", _SND, f"GetSoundTraitNumeric rObj {i}") for i in range(13)]
    + [_skv("snd.trait.past", _SND, "GetSoundTraitNumeric rObj 13"),
       _sk("snd.trait.set", _SND + 'let fRes := GetSoundTraitNumeric rObj 3\nrPlayer.AuxVarSetFlt "*nvfSN" fRes\n'
           "SetFunctionValue (SetSoundTraitNumeric rObj 3 7)"),
       _skv("snd.trait.set.read", _SND, "GetSoundTraitNumeric rObj 3"),
       _skv("snd.trait.restore", _SND + 'SetSoundTraitNumeric rObj 3 (rPlayer.AuxVarGetFlt "*nvfSN")\nrPlayer.AuxVarErase "*nvfSN"\n',
            "GetSoundTraitNumeric rObj 3"),
       _skv("snd.file", _SND, "GetSoundSourceFile rObj", True),
       _sk("snd.file.set", _SND + 'let sText := GetSoundSourceFile rObj\nrPlayer.AuxVarSetStr "*nvfSF" sText\n'
           'SetSoundSourceFile rObj "fx\\nvf\\probe.wav"\nSetFunctionValue (GetSoundSourceFile rObj)', True),
       _sk("snd.file.restore", _SND + 'SetSoundSourceFile rObj "fx\\emt\\medicalbeep\\emtmedicalbeep01.wav"\nrPlayer.AuxVarErase "*nvfSF"\n'
           "SetFunctionValue (GetSoundSourceFile rObj)", True),
       _skv("snd.playing.before", _SND, "IsSoundPlaying rObj"),
       _sk("snd.play", _SND + "PlaySound rObj\nSetFunctionValue (IsSoundPlaying rObj)"),
       _skv("snd.playing.next", _SND, "IsSoundPlaying rObj")]
    # Weather.
    + [_skv("wth.current", "let rW8 := GetCurrentWeather\n", "GetFormIDString rW8", True),
       _skv("wth.percent", "", "Ceil ((GetCurrentWeatherPercent) * 100)")]
    + [_skv(f"wth.trait.{i}", _WTH, f"GetWeatherTraitNumeric rW8 {i}") for i in range(21)]
    + [_sk("wth.trait.set", _WTH + 'let fRes := GetWeatherTraitNumeric rW8 4\nrPlayer.AuxVarSetFlt "*nvfWT" fRes\n'
           "SetFunctionValue (SetWeatherTraitNumeric rW8 4 0.5)"),
       _skv("wth.trait.set.read", _WTH, "GetWeatherTraitNumeric rW8 4"),
       _skv("wth.trait.restore", _WTH + 'SetWeatherTraitNumeric rW8 4 (rPlayer.AuxVarGetFlt "*nvfWT")\nrPlayer.AuxVarErase "*nvfWT"\n',
            "GetWeatherTraitNumeric rW8 4")]
    + [_skv(f"wth.rgb.type{k}.day", _WTH, f"sv_Construct \"%.0f\" (GetWeatherRGBColor rW8 {k} 1)", True) for k in range(10)]
    + [_skv(f"wth.rgb.ambient.time{t}", _WTH, f"sv_Construct \"%.0f\" (GetWeatherRGBColor rW8 3 {t})", True) for t in range(6)]
    + [_skv(f"wth.rgb.clouds.layer{c}", _WTH, f"sv_Construct \"%.0f\" (GetWeatherRGBColor rW8 2 1 {c})", True) for c in range(4)]
    + [_sk("wth.rgb.set", _WTH + 'let fRes := GetWeatherRGBColor rW8 3 1\nrPlayer.AuxVarSetFlt "*nvfWC" fRes\n'
           "SetFunctionValue (SetWeatherRGBColor rW8 3 1 16711935)"),
       _skv("wth.rgb.set.read", _WTH, 'sv_Construct "%.0f" (GetWeatherRGBColor rW8 3 1)', True)]
    # Where out-of-range parts go: each value set, then read back (the extender core
    # asked after 16711935 read 16199937).
    + [_skv(f"wth.rgb.set.{v}", _WTH + f"SetWeatherRGBColor rW8 3 1 {v}\n", 'sv_Construct "%.0f" (GetWeatherRGBColor rW8 3 1)', True)
       for v in (255255255, 0, 1, 255, 256, 1000, 255256000, 255000256, 256000000, 300000000, 12345678, 999999999)]
    + [_skv("wth.rgb.restore", _WTH + 'SetWeatherRGBColor rW8 3 1 (rPlayer.AuxVarGetFlt "*nvfWC")\nrPlayer.AuxVarErase "*nvfWC"\n',
            'sv_Construct "%.0f" (GetWeatherRGBColor rW8 3 1)', True)]
    + [_skv(f"wth.imod.time{t}", _WTH, f"GetFormIDString (GetWeatherImageSpaceMod rW8 {t})", True) for t in range(4)]
    # Image space and image-space modifier.
    + [_skv(f"imgs.trait.{i}", _IMGS, f"GetImageSpaceTrait rObj {i}") for i in range(33)]
    + [_sk("imgs.trait.set", _IMGS + 'let fRes := GetImageSpaceTrait rObj 15\nrPlayer.AuxVarSetFlt "*nvfIS" fRes\n'
           "SetImageSpaceTrait rObj 15 3.25\nSetFunctionValue (GetImageSpaceTrait rObj 15)"),
       _skv("imgs.trait.restore", _IMGS + 'SetImageSpaceTrait rObj 15 (rPlayer.AuxVarGetFlt "*nvfIS")\nrPlayer.AuxVarErase "*nvfIS"\n',
            "GetImageSpaceTrait rObj 15")]
    + [_skv(f"imad.trait.{i}", _IMAD, f"GetImageSpaceModTrait rObj {i}") for i in range(68)]
    + [_sk("imad.trait.set", _IMAD + 'let fRes := GetImageSpaceModTrait rObj 0\nrPlayer.AuxVarSetFlt "*nvfIM" fRes\n'
           "SetImageSpaceModTrait rObj 0 4.5\nSetFunctionValue (GetImageSpaceModTrait rObj 0)"),
       _skv("imad.trait.restore", _IMAD + 'SetImageSpaceModTrait rObj 0 (rPlayer.AuxVarGetFlt "*nvfIM")\nrPlayer.AuxVarErase "*nvfIM"\n',
            "GetImageSpaceModTrait rObj 0")]
)

# Thirty-sixth set (next batch N5): kNVSE's animation functions on a loaded actor: a
# copy of Doc Mitchell's NPC placed beside the player (his own reference's cell is not
# loaded). KF paths from fnvtool bsa list of Fallout - Meshes.bsa [observed]. The copy is
# removed at the end; the weapon override is undone.
_KF_IDLE = "characters\\_male\\locomotion\\mtidle.kf"
_KF_SMILE = "characters\\_male\\idleanims\\cg03dadsmilestanding.kf"
_KF_H2H = "characters\\_male\\h2hidle.kf"
_KF_JUMP = "characters\\_male\\1hpjumplandright.kf"
_HK = ("ref rPlayer\nref rA\nref rObj\nfloat fRes\nstring_var sText\narray_var aA\nBegin Function { }\n"
       + _form("rPlayer", NF["player"]))
_GETA = 'let rA := rPlayer.AuxVarGetRef "*nvfKA"\n'
_JOIN = ("let fRes := 0\nlet sText := sv_Construct \"%g:\" (ar_Size aA)\nwhile fRes < (ar_Size aA)\n"
         "  let sText += \" \" + aA[fRes]\n  let fRes += 1\nloop\nSetFunctionValue sText")
# The active list after ActivateAnim came back in another order on a second run, and
# ar_Sort does not compile at run time: that row keeps the size and whether each of the
# three sequences is in it.
_HAS3 = ("SetFunctionValue (sv_Construct \"%g %g %g %g\" (ar_Size aA) "
         "((ar_Find \"Characters\\_Male\\h2hidle.kf\" aA) >= 0) "
         "((ar_Find \"Characters\\_Male\\Locomotion\\mtidle.kf\" aA) >= 0) "
         "((ar_Find \"characters\\_male\\idleanims\\cg03dadsmilestanding.kf\" aA) >= 0))")


def _kn(pid, body, text=False):
    return (pid, _HK + body, text, None, "")


def _knv(pid, expr, text=False):
    return _kn(pid, _GETA + f"SetFunctionValue ({expr})", text)


KNVSE_PROBES = [
    _kn("kn.place", _form("rObj", NF["doc"]) + 'let rA := rPlayer.PlaceAtMe rObj 1\nrPlayer.AuxVarSetRef "*nvfKA" rA\nSetFunctionValue 1'),
    _kn("kn.settle", "SetFunctionValue 1"),
    _kn("kn.settle2", "SetFunctionValue 1"),
] + [_kn(f"kn.settle{i}", "SetFunctionValue 1") for i in range(3, 6)] + [
    # What plays right after placing depends on blend timing (runs gave the idle, empty
    # slots, or a __TempBlendSequence__, and GetPlayingAnimSequencePaths gave ar_Null in
    # some runs): only whether anything is active is kept.
    _kn("kn.active", _GETA + "let aA := rA.GetActiveAnims 0\nSetFunctionValue ((ar_Size aA) > 0)"),
    _knv("kn.animplaying.idle", "rA.IsAnimPlayingAlt Idle"),
    _knv("kn.seq.mtidle", f'rA.IsAnimSequencePlaying "{_KF_IDLE}"'),
    _kn("kn.bygroup.idle", _GETA + "let aA := rA.GetAnimationsByAnimGroup Idle\n" + _JOIN, True),
    _knv("kn.globaltime", "Ceil (rA.GetGlobalAnimTime 0)"),
    # SetActorAnimationPath: an override of the Idle group.
    _knv("kn.setpath", f'rA.SetActorAnimationPath 0 1 "{_KF_H2H}"'),
    _kn("kn.setpath.bygroup", _GETA + "let aA := rA.GetAnimationsByAnimGroup Idle\n" + _JOIN, True),
    _knv("kn.setpath.off", f'rA.SetActorAnimationPath 0 0 "{_KF_H2H}"'),
    _kn("kn.setpath.off.bygroup", _GETA + "let aA := rA.GetAnimationsByAnimGroup Idle\n" + _JOIN, True),
    _knv("kn.setpath.missing", 'rA.SetActorAnimationPath 0 1 "characters\\_male\\nvfnone.kf"'),
    # PlayAnimationPath and ActivateAnim / DeactivateAnim.
    _kn("kn.play", _GETA + f'rA.PlayAnimationPath "{_KF_SMILE}" 0\nSetFunctionValue (rA.IsAnimSequencePlaying "{_KF_SMILE}")'),
    _knv("kn.play.next", f'rA.IsAnimSequencePlaying "{_KF_SMILE}"'),
    _kn("kn.play.attrs", _GETA + f'let aA := ar_Keys (rA.GetAnimationAttributes "{_KF_SMILE}")\n' + _JOIN, True),
    _kn("kn.activate", _GETA + f'rA.ActivateAnim "{_KF_H2H}" 0 2 1 1 0\nSetFunctionValue (rA.IsAnimSequencePlaying "{_KF_H2H}")'),
    _knv("kn.activate.next", f'rA.IsAnimSequencePlaying "{_KF_H2H}"'),
    _kn("kn.activate.active", _GETA + "let aA := rA.GetActiveAnims 0\n" + _HAS3, True),
    _kn("kn.deactivate", _GETA + f'rA.DeactivateAnim "{_KF_H2H}" 0\nSetFunctionValue (rA.IsAnimSequencePlaying "{_KF_H2H}")'),
    _knv("kn.deactivate.next", f'rA.IsAnimSequencePlaying "{_KF_H2H}"'),
    # ForcePlayIdle / ForceStopIdle.
    _kn("kn.idle.play", _GETA + _form("rObj", FX["idlehouse"]) + "rA.ForcePlayIdle rObj\nSetFunctionValue (rA.IsIdlePlayingEx rObj)"),
    _kn("kn.idle.next", _GETA + _form("rObj", FX["idlehouse"]) + "SetFunctionValue (rA.IsIdlePlayingEx rObj)"),
    # Whether the idle still plays after ForceStopIdle varied between runs (1 or 0, also
    # three frames later), so ForceStopIdle is only called here, not read back.
    _kn("kn.idle.stop", _GETA + "rA.ForceStopIdle\nSetFunctionValue 1"),
    # A weapon's animation override.
    _kn("kn.weappath", _form("rObj", NF["pistol10"]) + f'SetFunctionValue (SetWeaponAnimationPath rObj 0 1 "{_KF_JUMP}")'),
    _kn("kn.weappath.off", _form("rObj", NF["pistol10"]) + f'SetFunctionValue (SetWeaponAnimationPath rObj 0 0 "{_KF_JUMP}")'),
    # The player (first person in the save).
    _kn("kn.player.paths", "let aA := rPlayer.GetPlayingAnimSequencePaths\n" + _JOIN, True),
    _kn("kn.player.animplaying.idle", "SetFunctionValue (rPlayer.IsAnimPlayingAlt Idle)"),
    _kn("kn.cleanup", _GETA + 'rA.Disable\nrA.MarkForDelete\nrPlayer.AuxVarErase "*nvfKA"\nSetFunctionValue 1'),
]

# Set knvse2 (#1018): kNVSE's second slice on a placed copy of Doc's NPC, as knvse-anim:
# the animation clock, a sequence's current time, weight and text keys, the blends'
# refusals, the attributes of a sequence the actor does not play. AllowAttack is not in
# the installed kNVSE 37 (a runtime compile fails). The placed actor goes at the end.
_KN2_ATTR = 'let aA := rA.GetAnimationAttributes "' + _KF_SMILE + '"\n'
KNVSE2_PROBES = [
    _kn("k2.place", _form("rObj", NF["doc"]) + 'let rA := rPlayer.PlaceAtMe rObj 1\nrPlayer.AuxVarSetRef "*nvfKA" rA\nSetFunctionValue 1'),
] + [_kn(f"k2.settle{i}", "SetFunctionValue 1") for i in range(1, 6)] + [
    _knv("k2.global", "Ceil (rA.GetGlobalAnimTime 0)"),
    _kn("k2.global.set", _GETA + "rA.SetGlobalAnimTime 5 0\nSetFunctionValue (Floor (rA.GetGlobalAnimTime 0))"),
    _knv("k2.global.next", "Floor (rA.GetGlobalAnimTime 0)"),
    _kn("k2.play", _GETA + f'rA.PlayAnimationPath "{_KF_SMILE}" 0\nSetFunctionValue (rA.IsAnimSequencePlaying "{_KF_SMILE}")'),
    _kn("k2.curtime.set", _GETA + f'rA.SetAnimCurrentTime "{_KF_SMILE}" 1.5\nSetFunctionValue (Floor ((rA.GetAnimCurrentTime "{_KF_SMILE}") * 10))'),
    # The next frame's time is 1.5 plus that frame (16 tenths in a first run, the frames
    # being slow): whole seconds only.
    _knv("k2.curtime.next", f'Floor (rA.GetAnimCurrentTime "{_KF_SMILE}")'),
    # Array elements go through a float first (as the nif-blocks set's _V3).
    _kn("k2.weight", _GETA + f'rA.SetAnimWeight "{_KF_SMILE}" 0.25\n' + _KN2_ATTR + 'let fRes := aA["seqWeight"]\nSetFunctionValue (Floor (fRes * 100))'),
    # AddAnimTextKey changes the loaded KF, which every actor shares, and kNVSE 37 has no
    # function that takes a key off again (no RemoveAnimTextKey page; SetAnimTextKeys
    # sets one actor's keys and "does nothing" while it plays the sequence). The change
    # is in the game's memory only, never on disk, and a job is one set per game launch,
    # which quits after k2.cleanup, so nothing reads the key after this set.
    _knv("k2.addkey", f'AddAnimTextKey "{_KF_SMILE}" 0.5 "nvfKey"'),
    _knv("k2.addkey.missing", 'AddAnimTextKey "characters\\_male\\nvfnone.kf" 0.5 "nvfKey"'),
    _kn("k2.addkey.read", _GETA + _KN2_ATTR + 'let aA := aA["textKeyValues"]\nSetFunctionValue ((ar_Find "nvfKey" aA) >= 0)'),
    _kn("k2.settextkeys.playing", _GETA + f'rA.SetAnimTextKeys "{_KF_SMILE}" (ar_List 0, 1) (ar_List "nvfA", "nvfB")\n' + _KN2_ATTR
        + 'let aA := aA["textKeyValues"]\nSetFunctionValue ((ar_Find "nvfA" aA) >= 0)'),
    _kn("k2.attrs.played", _GETA + _KN2_ATTR + "SetFunctionValue (ar_Size aA)"),
    _kn("k2.attrs.notplayed", _GETA + f'let aA := rA.GetAnimationAttributes "{_KF_JUMP}"\nSetFunctionValue (ar_Size aA)'),
    _kn("k2.attrs.noref", f'let aA := GetAnimationAttributes "{_KF_SMILE}"\nSetFunctionValue (ar_Size aA)'),
    _kn("k2.attrs.state", _GETA + _KN2_ATTR + 'SetFunctionValue aA["state"]'),
    _kn("k2.attrs.cycle", _GETA + _KN2_ATTR + 'SetFunctionValue aA["cycleType"]'),
    _kn("k2.attrs.root", _GETA + _KN2_ATTR + 'SetFunctionValue aA["accumRootName"]', True),
    _kn("k2.bytype.idle", _GETA + "let aA := rA.GetPlayingAnimBySequenceType 0\nSetFunctionValue (ar_Size aA)"),
    _kn("k2.bytype.special", _GETA + "let aA := rA.GetPlayingAnimBySequenceType 7\nSetFunctionValue (ar_Size aA)"),
    _knv("k2.crossfade.notplaying", f'rA.CrossFadeAnims "{_KF_H2H}" "{_KF_JUMP}" 0'),
    _knv("k2.blend.notplaying", f'rA.BlendToAnim "{_KF_H2H}" "{_KF_JUMP}" 0'),
    _knv("k2.blendpose", f'rA.BlendToAnimFromPose "{_KF_H2H}" 0 0.2'),
    _knv("k2.additive", f'rA.SetAnimAdditive "{_KF_JUMP}" 0'),
    _knv("k2.additive.missing", 'rA.SetAnimAdditive "characters\\_male\\nvfnone.kf" 0'),
    _kn("k2.cleanup", _GETA + 'rA.Disable\nrA.MarkForDelete\nrPlayer.AuxVarErase "*nvfKA"\nSetFunctionValue 1'),
]

# Thirty-seventh set (next batch N6): JIP's NIF block functions on a loaded actor (a
# copy of Doc's NPC placed beside the player, as knvse-anim), by node name: the Bip01
# skeleton. Changes are made on that reference only (InsertNode / AttachModel mode 3,
# instantaneous) and go with it at the end. ModelHasBlock reads the 10mm pistol's model.
def _nb(pid, body, text=False):
    return (pid, _HK.replace("float fRes\n", "float fRes\nfloat fX\nfloat fY\nfloat fZ\n") + body, text, None, "")


def _nbv(pid, expr, text=False):
    return _nb(pid, _GETA + f"SetFunctionValue ({expr})", text)


# Array elements go through float variables first: sv_Construct with aA[0] as an
# argument fails at run time ("script call failed" with a 3-element array).
_V3 = ('let fX := aA[0]\nlet fY := aA[1]\nlet fZ := aA[2]\n'
       'SetFunctionValue (sv_Construct "%.2f %.2f %.2f" fX fY fZ)')

NIF_PROBES = [
    _nb("nb.place", _form("rObj", NF["doc"]) + 'let rA := rPlayer.PlaceAtMe rObj 1\nrPlayer.AuxVarSetRef "*nvfKA" rA\nSetFunctionValue 1'),
    _nb("nb.settle", "SetFunctionValue 1"),
    _nb("nb.settle2", "SetFunctionValue 1"),
] + [_nb(f"nb.settle{i}", "SetFunctionValue 1") for i in range(3, 6)] + [
    _nb("nb.trans.head.size", _GETA + 'let aA := rA.GetNifBlockTranslation "Bip01 Head" 0\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.trans.alt", _GETA.replace("", "") + 'let fRes := rA.GetNifBlockTranslationAlt "Bip01 Head" fX fY fZ 0\n'
        'SetFunctionValue (sv_Construct "%g %.2f %.2f %.2f" fRes fX fY fZ)', True),
    _nb("nb.rot.alt", _GETA + 'let fRes := rA.GetNifBlockRotationAlt "Bip01 Head" fX fY fZ 0\n'
        'SetFunctionValue (sv_Construct "%g %.2f %.2f %.2f" fRes fX fY fZ)', True),
    _nb("nb.children.bip01", _GETA + 'let aA := rA.GetChildBlocks "Bip01"\n' + _JOIN, True),
    _nb("nb.children.head", _GETA + 'let aA := rA.GetChildBlocks "Bip01 Head"\n' + _JOIN, True),
    _nb("nb.children.missing", _GETA + 'let aA := rA.GetChildBlocks "NVFNone"\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.parents.head", _GETA + 'let aA := rA.GetNifBlockParentNodes "Bip01 Head"\n' + _JOIN, True),
    # The head's pose follows the idle animation (two runs differed in the second
    # decimal): only the vector's size is kept.
    _nb("nb.trans.head", _GETA + 'let aA := rA.GetNifBlockTranslation "Bip01 Head" 0\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.rot.head", _GETA + 'let aA := rA.GetNifBlockRotation "Bip01 Head" 0\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.rot.head.local", _GETA + 'let aA := rA.GetNifBlockRotation "Bip01 Head" 2\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.trans.missing", _GETA + 'let aA := rA.GetNifBlockTranslation "NVFNone" 0\nSetFunctionValue (ar_Size aA)'),
    _nbv("nb.scale.head", 'rA.GetNifBlockScale "Bip01 Head"'),
    _nbv("nb.scale.missing", 'rA.GetNifBlockScale "NVFNone"'),
    _nb("nb.scale.set", _GETA + 'rA.SetNifBlockScale "Bip01 Head" 1.5\nSetFunctionValue (rA.GetNifBlockScale "Bip01 Head")'),
    _nbv("nb.scale.next", 'rA.GetNifBlockScale "Bip01 Head"'),
    _nb("nb.trans.set", _GETA + 'rA.SetNifBlockTranslation "Bip01 Head" 1 2 3\nlet aA := rA.GetNifBlockTranslation "Bip01 Head" 0\n' + _V3, True),
    _nb("nb.rot.set", _GETA + 'rA.SetNifBlockRotation "Bip01 Head" 10 20 30 0\nlet aA := rA.GetNifBlockRotation "Bip01 Head" 0\n' + _V3, True),
] + [_nbv(f"nb.flag.head.{f}", f'rA.GetNifBlockFlag "Bip01 Head" {f}') for f in range(4)] + [
    _nb("nb.flag.set", _GETA + 'rA.SetNifBlockFlag "Bip01 Head" 0 1\nSetFunctionValue (rA.GetNifBlockFlag "Bip01 Head" 0)'),
    _nb("nb.flag.clear", _GETA + 'rA.SetNifBlockFlag "Bip01 Head" 0 0\nSetFunctionValue (rA.GetNifBlockFlag "Bip01 Head" 0)'),
    _nb("nb.rename", _GETA + 'SetFunctionValue (rA.RenameNifBlock "Bip01 Head" "NVFHead")'),
    _nbv("nb.rename.new", 'rA.GetNifBlockScale "NVFHead"'),
    _nbv("nb.rename.old", 'rA.GetNifBlockScale "Bip01 Head"'),
    _nb("nb.rename.back", _GETA + 'SetFunctionValue (rA.RenameNifBlock "NVFHead" "Bip01 Head")'),
    _nb("nb.insert", _GETA + 'SetFunctionValue (InsertNode rA 3 "Bip01 Head|NVFNode")'),
    _nb("nb.insert.children", _GETA + 'let aA := rA.GetChildBlocks "Bip01 Head"\n' + _JOIN, True),
    _nb("nb.insert.parent", _GETA + 'SetFunctionValue (InsertNode rA 3 "Bip01 Head|^NVFParent")'),
    _nb("nb.insert.parent.parents", _GETA + 'let aA := rA.GetNifBlockParentNodes "Bip01 Head"\n' + _JOIN, True),
    _nb("nb.insert.case", _GETA + 'SetFunctionValue (InsertNode rA 3 "bip01 head|NVFNode2")'),
    _nb("nb.attach", _GETA + 'SetFunctionValue (AttachModel rA 3 "NVFNode|Weapons\\1HandPistol\\10mmPistol.NIF")'),
    _nb("nb.attach.children", _GETA + 'let aA := rA.GetChildBlocks "NVFNode"\n' + _JOIN, True),
    _nb("nb.remove", _GETA + 'SetFunctionValue (rA.RemoveNifBlock "NVFNode")'),
    _nbv("nb.remove.scale", 'rA.GetNifBlockScale "NVFNode"'),
    _nb("nb.hasblock.pistol", _form("rObj", NF["pistol10"]) + 'SetFunctionValue (ModelHasBlock rObj "ProjectileNode")'),
    _nb("nb.hasblock.missing", _form("rObj", NF["pistol10"]) + 'SetFunctionValue (ModelHasBlock rObj "NVFNone")'),
    _nb("nb.hasblock.case", _form("rObj", NF["pistol10"]) + 'SetFunctionValue (ModelHasBlock rObj "projectilenode")'),
    _nb("nb.player.children", 'let aA := rPlayer.GetChildBlocks "Bip01" 2\n' + _JOIN, True),
    # A world object: the 10mm pistol placed as an item beside the player.
    _nb("nb.obj.place", _form("rObj", NF["pistol10"]) + 'let rA := rPlayer.PlaceAtMe rObj 1\nrPlayer.AuxVarSetRef "*nvfKO" rA\nSetFunctionValue 1'),
] + [_nb(f"nb.obj.settle{i}", "SetFunctionValue 1") for i in range(5)] + [
    _nb("nb.obj.trans.size", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nlet aA := rA.GetNifBlockTranslation "ProjectileNode" 0\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.obj.rot.size", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nlet aA := rA.GetNifBlockRotation "ProjectileNode" 0\nSetFunctionValue (ar_Size aA)'),
    _nb("nb.obj.trans", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nlet aA := rA.GetNifBlockTranslation "ProjectileNode" 0\n' + _V3, True),
    _nb("nb.obj.rot", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nlet aA := rA.GetNifBlockRotation "ProjectileNode" 0\n' + _V3, True),
    _nb("nb.obj.trans.set", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nrA.SetNifBlockTranslation "ProjectileNode" 1 2 3\n'
        'let aA := rA.GetNifBlockTranslation "ProjectileNode" 0\n' + _V3, True),
    _nb("nb.obj.rot.set", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nrA.SetNifBlockRotation "ProjectileNode" 10 20 30 0\n'
        'let aA := rA.GetNifBlockRotation "ProjectileNode" 0\n' + _V3, True),
    _nb("nb.obj.scale", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nSetFunctionValue (rA.GetNifBlockScale "ProjectileNode")'),
    _nb("nb.obj.children", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nlet aA := rA.GetChildBlocks "10MMPistol"\n' + _JOIN, True),
    _nb("nb.obj.cleanup", 'let rA := rPlayer.AuxVarGetRef "*nvfKO"\nrA.Disable\nrA.MarkForDelete\nrPlayer.AuxVarErase "*nvfKO"\nSetFunctionValue 1'),
    _nb("nb.cleanup", _GETA + 'rA.Disable\nrA.MarkForDelete\nrPlayer.AuxVarErase "*nvfKA"\nSetFunctionValue 1'),
]

# Thirty-eighth set (next batch N11): which xNVSE events fire in the game, and with which
# two arguments. One SetEventHandler lambda per event appends its firing count and both
# arguments (as references) to `*` auxiliary variables on the player; then the events
# are caused cheaply: the player's inventory (OnAdd, OnDrop, OnActorEquip/OnEquip,
# OnActorUnequip/OnUnequip), a placed creature (OnLoad, OnDisable/OnEnable,
# OnStartCombat, OnHit/OnHitWith, OnDeath/OnMurder), a placed NPC (OnPackageStart/
# Change), a stimpak cast on the player (OnMagicEffectHit). Left out: events that open
# a menu or dialogue (OnActivate, OnOpen/OnClose, OnSell). Handlers die with the session.
_EVENTS = ["OnAdd", "OnDrop", "OnActorEquip", "OnEquip", "OnActorUnequip", "OnUnequip", "OnLoad", "OnDisable",
           "OnEnable", "OnStartCombat", "OnHit", "OnHitWith", "OnDeath", "OnMurder", "OnPackageStart",
           "OnPackageChange", "OnMagicEffectHit", "OnCombatEnd"]
_HE = ("ref rP\nref rA\nref rB\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rP", NF["player"]))


def _ev(pid, body, text=False):
    return (pid, _HE + body, text, None, "")


def _ev_reg(name):
    k = name.lower()
    return _ev(f"evn.reg.{k}", f'SetEventHandler "{name}" (Begin Function {{ref rE1, ref rE2}}\n'
               f'  rP.AuxVarSetFlt "*nvfE.{k}" 1 -1\n  rP.AuxVarSetRef "*nvfE.{k}.1" rE1 -1\n'
               f'  rP.AuxVarSetRef "*nvfE.{k}.2" rE2 -1\nEnd)\nSetFunctionValue 1')


# An argument as "player", "creature" / "npc" (the placed ones), "ref:<base form>" for any
# other reference (made-at-run-time
# references get a different ID each run) or the form's ID.
_ARG = ('let rB := rP.AuxVarGetRef "*nvfEC"\nlet rObj := rP.AuxVarGetRef "*nvfEN"\n'
        'if rA == rP\n  SetFunctionValue "player"\nelseif rA == rB\n  SetFunctionValue "creature"\n'
        'elseif rA == rObj\n  SetFunctionValue "npc"\nelseif IsReference rA\n'
        '  SetFunctionValue ("ref:" + GetFormIDString (rA.GetBaseObject))\nelse\n'
        '  SetFunctionValue (GetFormIDString rA)\nendif')
_CRE = 'let rA := rP.AuxVarGetRef "*nvfEC"\n'
_NPC = 'let rB := rP.AuxVarGetRef "*nvfEN"\n'

NVSE_EVENT_PROBES = (
    [_ev_reg(n) for n in _EVENTS]
    + [
        # Inventory.
        _ev("evn.do.add", _form("rObj", NF["stimpak"]) + "rP.AddItem rObj 1 1\nSetFunctionValue 1"),
        _ev("evn.do.drop", _form("rObj", NF["stimpak"]) + 'rP.Drop rObj 1\nSetFunctionValue 1'),
        _ev("evn.do.addgun", _form("rObj", NF["pistol10"]) + "rP.AddItem rObj 1 1\nSetFunctionValue 1"),
        _ev("evn.do.equip", _form("rObj", NF["pistol10"]) + "rP.EquipItem rObj 0 1\nSetFunctionValue 1"),
        _ev("evn.do.unequip", _form("rObj", NF["pistol10"]) + "rP.UnequipItem rObj 0 1\nSetFunctionValue 1"),
        _ev("evn.do.removegun", _form("rObj", NF["pistol10"]) + "rP.RemoveItem rObj 1 1\nSetFunctionValue 1"),
        # A placed creature (the airport ACRE's base) and a placed NPC (Doc's).
        _ev("evn.do.placecreature", _form("rObj", "14DEE2") + 'let rObj := rObj.GetBaseObject\nlet rA := rP.PlaceAtMe rObj 1\n'
            'rP.AuxVarSetRef "*nvfEC" rA\nSetFunctionValue 1'),
        _ev("evn.do.placenpc", _form("rObj", NF["doc"]) + 'let rB := rP.PlaceAtMe rObj 1\nrP.AuxVarSetRef "*nvfEN" rB\nSetFunctionValue 1'),
    ]
    + [_ev(f"evn.settle{i}", "SetFunctionValue 1") for i in range(4)]
    + [
        _ev("evn.do.disable", _CRE + "rA.Disable\nSetFunctionValue 1"),
        _ev("evn.do.enable", _CRE + "rA.Enable\nSetFunctionValue 1"),
        _ev("evn.do.package", _NPC + "rB.EvaluatePackage\nSetFunctionValue 1"),
        _ev("evn.do.combat", _CRE + "rA.StartCombat rP\nSetFunctionValue 1"),
        _ev("evn.do.cast", _form("rObj", NF["stimpak"]) + "rP.CastImmediate rObj\nSetFunctionValue 1"),
        _ev("evn.do.boost", "rP.ModAV Health 1000\nSetFunctionValue 1"),
        _ev("evn.do.hit", _CRE + _form("rObj", "7078B") + "rA.PlaceAtMe rObj 1\nSetFunctionValue 1"),
    ]
    + [_ev(f"evn.settle.hit{i}", "SetFunctionValue 1") for i in range(3)]
    + [
        _ev("evn.do.kill", _CRE + "rA.Kill rP\nSetFunctionValue 1"),
        _ev("evn.do.stopcombat", _NPC + "rB.StopCombat\nSetFunctionValue 1"),
    ]
    + [_ev(f"evn.settle.kill{i}", "SetFunctionValue 1") for i in range(3)]
    + [x for n in _EVENTS for x in (
        # Whether it fired: how many times depends on frame timing (two runs gave
        # OnHit 4 and 1, OnLoad 4 and 3).
        _ev(f"evn.{n.lower()}.fired", f'SetFunctionValue ((rP.AuxVarSize "*nvfE.{n.lower()}") > 0)'),
        _ev(f"evn.{n.lower()}.arg1", f'let rA := rP.AuxVarGetRef "*nvfE.{n.lower()}.1" 0\n' + _ARG, True),
        _ev(f"evn.{n.lower()}.arg2", f'let rA := rP.AuxVarGetRef "*nvfE.{n.lower()}.2" 0\n' + _ARG, True),
    )]
    + [
        _ev("evn.cleanup", _CRE + _NPC + "rA.Disable\nrA.MarkForDelete\nrB.Disable\nrB.MarkForDelete\n"
            + "rP.ModAV Health -1000\nrP.RestoreAV Health 2000\nSetFunctionValue 1"),
    ]
)

# Thirty-ninth set (N11, JIP and JohnnyGuitar handlers): the SetOn...EventHandler events
# a script can cause without a menu or input: FireWeapon (player), UseAidItem (a stimpak
# used through EquipItem), CrippledLimb and PlayGroup (a placed copy of Doc's NPC),
# Ragdoll (a placed creature killed), NoteAdded (AddNote vYesManBunkerNote 177875,
# fnvtool esm inventory [observed]) and JohnnyGuitar's ActorValueChange (ModAV on the
# player). Each lambda takes the arguments the function page lists [documented] and
# appends what it got to `*` auxiliary variables. Handlers die with the session.
FX.update(note="177875")
_HJ = ("ref rP\nref rA\nref rB\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rP", NF["player"]))


def _je(pid, body, text=False):
    return (pid, _HJ + body, text, None, "")


def _jread(pid, expr, text=False):
    return _je(pid, f"SetFunctionValue ({expr})", text)


_JC = 'let rA := rP.AuxVarGetRef "*nvfJC"\n'
_JN = 'let rB := rP.AuxVarGetRef "*nvfJN"\n'

JIP_EVENT2_PROBES = [
    # Actors to act on.
    _je("je.place.creature", _form("rObj", "14DEE2") + 'let rObj := rObj.GetBaseObject\nlet rA := rP.PlaceAtMe rObj 1\n'
        'rP.AuxVarSetRef "*nvfJC" rA\nSetFunctionValue 1'),
    _je("je.place.npc", _form("rObj", NF["doc"]) + 'let rB := rP.PlaceAtMe rObj 1\nrP.AuxVarSetRef "*nvfJN" rB\nSetFunctionValue 1'),
] + [_je(f"je.settle{i}", "SetFunctionValue 1") for i in range(5)] + [
    # Registrations.
    _je("je.reg.fire", 'SetOnFireWeaponEventHandler (Begin Function {ref rWp}\n  rP.AuxVarSetFlt "*nvfJ.fire" (GetSelf == rP) -1\n'
        '  rP.AuxVarSetRef "*nvfJ.fire.w" rWp -1\nEnd) 1 rP\nSetFunctionValue 1'),
    _je("je.reg.aid", _form("rObj", NF["stimpak"]) + 'SetOnUseAidItemEventHandler (Begin Function {ref rIt}\n'
        '  rP.AuxVarSetFlt "*nvfJ.aid" (GetSelf == rP) -1\n  rP.AuxVarSetRef "*nvfJ.aid.i" rIt -1\nEnd) 1 rObj\nSetFunctionValue 1'),
    _je("je.reg.cripple", _JN + 'SetOnCrippledLimbEventHandler (Begin Function {int iLimb}\n'
        '  rP.AuxVarSetFlt "*nvfJ.cripple" iLimb -1\nEnd) 1 rB\nSetFunctionValue 1'),
    _je("je.reg.playgroup", _JN + 'SetOnPlayGroupEventHandler (Begin Function {int iGroup}\n'
        '  rP.AuxVarSetFlt "*nvfJ.group" iGroup -1\nEnd) 1 rB Idle\nSetFunctionValue 1'),
    _je("je.reg.ragdoll", _JC + 'SetOnRagdollEventHandler (Begin Function { }\n'
        '  rP.AuxVarSetFlt "*nvfJ.ragdoll" (GetSelf == (rP.AuxVarGetRef "*nvfJC")) -1\nEnd) 1\nSetFunctionValue 1'),
    _je("je.reg.note", 'SetOnNoteAddedEventHandler (Begin Function {ref rNote}\n  rP.AuxVarSetRef "*nvfJ.note" rNote -1\nEnd) 1\n'
        'SetFunctionValue 1'),
    # Three arguments [documented]; a handler taking four crashed the game when Health
    # changed [observed: jip_events2 crash, 2026-10-26].
    _je("je.reg.av", 'SetOnActorValueChangeEventHandler 1 (Begin Function {int iAV, float fOld, float fNew}\n'
        '  rP.AuxVarSetFlt "*nvfJ.av" iAV -1\n  rP.AuxVarSetFlt "*nvfJ.av.d" (fNew - fOld) -1\nEnd) 0\nSetFunctionValue 1'),
    # Causes.
    _je("je.do.fire", _form("rObj", NF["pistol10"]) + "rP.FireWeapon rObj\nSetFunctionValue 1"),
    _je("je.do.aid", _form("rObj", NF["stimpak"]) + "rP.EquipItem rObj 0 1\nSetFunctionValue 1"),
    _je("je.do.cripple", _JN + 'rB.DamageAV LeftAttackCondition 200\nSetFunctionValue 1'),
    _je("je.do.playgroup", _JN + "rB.PlayGroup Idle 1\nSetFunctionValue 1"),
    _je("je.do.note", _form("rObj", FX["note"]) + "rP.AddNote rObj\nSetFunctionValue 1"),
    _je("je.do.av", "rP.ModAV Health 5\nSetFunctionValue 1"),
    _je("je.do.kill", _JC + "rA.Kill rP\nSetFunctionValue 1"),
] + [_je(f"je.settle.after{i}", "SetFunctionValue 1") for i in range(4)] + [
    # Reads: whether each fired, and its first values.
    _jread("je.fire.fired", '(rP.AuxVarSize "*nvfJ.fire") > 0'),
    _jread("je.fire.self", 'rP.AuxVarGetFlt "*nvfJ.fire" 0'),
    _jread("je.fire.weapon", 'GetFormIDString (rP.AuxVarGetRef "*nvfJ.fire.w" 0)', True),
    _jread("je.aid.fired", '(rP.AuxVarSize "*nvfJ.aid") > 0'),
    _jread("je.aid.self", 'rP.AuxVarGetFlt "*nvfJ.aid" 0'),
    _jread("je.aid.item", 'GetFormIDString (rP.AuxVarGetRef "*nvfJ.aid.i" 0)', True),
    _jread("je.cripple.fired", '(rP.AuxVarSize "*nvfJ.cripple") > 0'),
    _jread("je.cripple.limb", 'rP.AuxVarGetFlt "*nvfJ.cripple" 0'),
    _jread("je.group.fired", '(rP.AuxVarSize "*nvfJ.group") > 0'),
    _jread("je.group.index", 'rP.AuxVarGetFlt "*nvfJ.group" 0'),
    _jread("je.ragdoll.fired", '(rP.AuxVarSize "*nvfJ.ragdoll") > 0'),
    _jread("je.ragdoll.self", 'rP.AuxVarGetFlt "*nvfJ.ragdoll" 0'),
    _jread("je.note.fired", '(rP.AuxVarSize "*nvfJ.note") > 0'),
    _jread("je.note.form", 'GetFormIDString (rP.AuxVarGetRef "*nvfJ.note" 0)', True),
    _jread("je.av.fired", '(rP.AuxVarSize "*nvfJ.av") > 0'),
    _jread("je.av.code", 'rP.AuxVarGetFlt "*nvfJ.av" 0'),
    _jread("je.av.delta", 'rP.AuxVarGetFlt "*nvfJ.av.d" 0'),
    _je("je.cleanup", _JC + _JN + "rA.Disable\nrA.MarkForDelete\nrB.Disable\nrB.MarkForDelete\nrP.ModAV Health -5\n"
        'rP.AuxVarErase "*nvfJC"\nrP.AuxVarErase "*nvfJN"\nSetFunctionValue 1'),
]

# Fortieth set (next batch N13): xNVSE and JIP leftovers. GetFalloutDirectory gives a
# path of the owner's machine, so the row keeps only whether it ends with the game
# folder's name and a backslash. Read-only.
_HL3 = ("ref rPlayer\nref rDoc\nref rObj\nfloat fRes\nstring_var sText\narray_var aA\nBegin Function { }\n"
        + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]))


def _lo(pid, body, text=False):
    return (pid, _HL3 + body, text, None, "")


N13_PROBES = [
    _lo("lo.paired", _form("rObj", NF["stimpak"]) + "SetFunctionValue (rPlayer.GetPaired rObj rDoc)"),
    _lo("lo.testexpr.ok", "SetFunctionValue (TestExpr (1 + 1))"),
    _lo("lo.testexpr.badindex", "let aA := ar_List 1 2\nSetFunctionValue (TestExpr aA[5])"),
    _lo("lo.testexpr.goodindex", "let aA := ar_List 1 2\nSetFunctionValue (TestExpr aA[1])"),
    _lo("lo.testexpr.nullref", "SetFunctionValue (TestExpr (rObj.GetName))"),
    _lo("lo.testexpr.uninitarray", "SetFunctionValue (TestExpr (ar_Size aA))"),
    _lo("lo.testexpr.stringkey", 'let aA := ar_Map "a"::1\nSetFunctionValue (TestExpr aA["b"])'),
    _lo("lo.falloutdir.end", 'let sText := GetFalloutDirectory\n'
        'SetFunctionValue (sv_Find "Fallout New Vegas\\" sText) == ((sv_Length sText) - 18)'),
    _lo("lo.falloutdir.len.positive", "let sText := GetFalloutDirectory\nSetFunctionValue ((sv_Length sText) > 0)"),
    _lo("lo.calcdamage.base", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetCalculatedWeaponDamage rObj)"),
    _lo("lo.calcdamage.hunting", _form("rObj", NF["hunting"]) + "SetFunctionValue (GetCalculatedWeaponDamage rObj)"),
    _lo("lo.questlinks.size", "let aA := GetCurrentQuestObjectiveTeleportLinks\nSetFunctionValue (ar_Size aA)"),
    _lo("lo.printvar", "let fRes := 2.5\nPrintVar fRes\nSetFunctionValue 1"),
    _lo("lo.printvar.many", 'let fRes := 2.5\nlet sText := "x"\nPrintVar fRes sText\nSetFunctionValue 1'),
    _lo("lo.debugprintvar", "let fRes := 2.5\nDebugPrintVar fRes\nSetFunctionValue 1"),
    _lo("lo.solditem", "SetFunctionValue (GetFormIDString (GetSoldItemInvRef))", True),
]

# Forty-first and forty-second sets (next batch N12): what extender state survives a save
# and load. save-set runs on nvfairport, sets everything and reads it back; the run then
# ends with the console line "save nvfprobe" (capture_run --quit-console). save-read runs
# on nvfprobe and reads it all again. The owner allowed this one save, removed after the
# run with its co-save files (owner-review, 2026-10-26). Aux variable kinds by name:
# plain (public), "_" (private to the mod), "*" (temporary) [documented: JIP's auxiliary
# variable page names the prefixes; which survive is what is probed]. Forms as in
# lu-leveled and sky-sound; ExtProbeLoops06Q (NVFExtProbe.esp B40) holds `int iR`.
_HS = ("ref rP\nref rDoc\nref rBase\nref rObj\nref rQ\nfloat fRes\nBegin Function { }\n" + _form("rP", NF["player"])
       + _form("rDoc", NF["docref"]) + _form("rBase", NF["doc"]) + 'let rQ := GetFormFromMod "NVFExtProbe.esp" "B40"\n')


def _sv(pid, body, text=False):
    return (pid, _HS + body, text, None, "")


# What both runs read, in the same order.
def _save_reads(tag):
    rows = []
    for pre in ("", "_", "*"):
        k = {"": "pub", "_": "priv", "*": "temp"}[pre]
        rows += [
            _sv(f"{tag}.aux.flt.{k}", f'SetFunctionValue (rP.AuxVarGetFlt "{pre}nvfSF")'),
            _sv(f"{tag}.aux.ref.{k}", f'SetFunctionValue (GetFormIDString (rP.AuxVarGetRef "{pre}nvfSR"))', True),
            _sv(f"{tag}.aux.str.{k}", f'SetFunctionValue (rP.AuxVarGetStr "{pre}nvfSS")', True),
            _sv(f"{tag}.refmap.{k}", f'SetFunctionValue (rDoc.RefMapGetFlt "{pre}nvfSM")'),
        ]
    rows += [
        _sv(f"{tag}.questvar", 'SetFunctionValue (GetVariable "iR" rQ)'),
        _sv(f"{tag}.controls.alt", "SetFunctionValue (GetPlayerControlsDisabledAlt 1)"),
        _sv(f"{tag}.controls.vanilla", "SetFunctionValue (GetPlayerControlsDisabled)"),
        _sv(f"{tag}.loop.keep", 'let fRes := rP.AuxVarGetFlt "nvfSL1"\nSetFunctionValue (fRes > 0)'),
        _sv(f"{tag}.loop.auto", 'let fRes := rP.AuxVarGetFlt "nvfSL8"\nSetFunctionValue (fRes > 0)'),
        _sv(f"{tag}.loop.keep.stamp", 'rP.AuxVarSetFlt "nvfSL1s" (rP.AuxVarGetFlt "nvfSL1")\nSetFunctionValue 1'),
        _sv(f"{tag}.loop.auto.stamp", 'rP.AuxVarSetFlt "nvfSL8s" (rP.AuxVarGetFlt "nvfSL8")\nSetFunctionValue 1'),
        _sv(f"{tag}.loop.wait1", "SetFunctionValue 1"),
        _sv(f"{tag}.loop.wait2", "SetFunctionValue 1"),
        _sv(f"{tag}.loop.keep.running", 'SetFunctionValue ((rP.AuxVarGetFlt "nvfSL1") > (rP.AuxVarGetFlt "nvfSL1s"))'),
        _sv(f"{tag}.loop.auto.running", 'SetFunctionValue ((rP.AuxVarGetFlt "nvfSL8") > (rP.AuxVarGetFlt "nvfSL8s"))'),
        _sv(f"{tag}.sound.trait3", _form("rObj", FX["beep"]) + "SetFunctionValue (GetSoundTraitNumeric rObj 3)"),
        _sv(f"{tag}.lvli.lu.num", _form("rObj", FX["lvli"]) + "SetFunctionValue (GetNumLevItems rObj)"),
        _sv(f"{tag}.lvln.showoff.num", _form("rObj", FX["lvln"]) + "SetFunctionValue (GetNumLevItems rObj)"),
        _sv(f"{tag}.lvlc.lu.num", _form("rObj", FX["lvlc"]) + "SetFunctionValue (GetNumLevItems rObj)"),
        _sv(f"{tag}.name", "SetFunctionValue (GetName rBase)", True),
        _sv(f"{tag}.karma", "SetFunctionValue (GetBaseKarma rBase)"),
    ]
    return rows


SAVE_SET_PROBES = (
    [_sv(f"set.aux.{pre or 'pub'}", f'rP.AuxVarSetFlt "{pre}nvfSF" 2.5\nrP.AuxVarSetRef "{pre}nvfSR" rDoc\n'
         f'rP.AuxVarSetStr "{pre}nvfSS" "kept?"\nrDoc.RefMapSetFlt "{pre}nvfSM" 4\nSetFunctionValue 1') for pre in ("", "_", "*")]
    + [_sv("set.questvar", 'SetVariable "iR" 7 rQ\nSetFunctionValue (GetVariable "iR" rQ)'),
       _sv("set.controls.alt", "DisablePlayerControlsAlt 0 0 0 1 0 0 0\nSetFunctionValue (GetPlayerControlsDisabledAlt 1)"),
       _sv("set.controls.vanilla", "DisablePlayerControls 0 0 0 0 0 0 1\nSetFunctionValue (GetPlayerControlsDisabled)"),
       _sv("set.loop.keep", 'SetGameMainLoopCallback (Begin Function { }\n  rP.AuxVarSetFlt "nvfSL1" ((rP.AuxVarGetFlt "nvfSL1") + 1)\nEnd) 1 1 1\n'
           "SetFunctionValue 1"),
       _sv("set.loop.auto", 'SetGameMainLoopCallback (Begin Function { }\n  rP.AuxVarSetFlt "nvfSL8" ((rP.AuxVarGetFlt "nvfSL8") + 1)\nEnd) 1 1 9\n'
           "SetFunctionValue 1"),
       _sv("set.sound", _form("rObj", FX["beep"]) + "SetSoundTraitNumeric rObj 3 7\nSetFunctionValue (GetSoundTraitNumeric rObj 3)"),
       _sv("set.lvli.lu", _form("rObj", FX["lvli"]) + _form("rQ", FX["ammo556"]) + "LeveledListAddForm rObj rQ 1 1 1.0\nSetFunctionValue (GetNumLevItems rObj)"),
       _sv("set.lvln.showoff", _form("rObj", FX["lvln"]) + "let rQ := GetNthLevItem rObj 0\nRemoveFormFromLeveledList rObj rQ\nSetFunctionValue (GetNumLevItems rObj)"),
       _sv("set.lvlc.lu", _form("rObj", FX["lvlc"]) + "let rQ := GetNthLevItem rObj 0\nLeveledListRemoveForm rObj rQ\nSetFunctionValue (GetNumLevItems rObj)"),
       _sv("set.name", 'LNSetName "NVF Saved Doc" rBase\nSetFunctionValue (GetName rBase)', True),
       _sv("set.karma", "SetBaseKarma 77 rBase\nSetFunctionValue (GetBaseKarma rBase)")]
    + [_sv(f"set.wait{i}", "SetFunctionValue 1") for i in range(3)]
    + _save_reads("before")
)

SAVE_READ_PROBES = (
    [_sv(f"read.wait{i}", "SetFunctionValue 1") for i in range(3)]
    + _save_reads("after")
    + [_sv("read.cleanup", "EnablePlayerControlsAlt\nEnablePlayerControls\nSetFunctionValue 1")]
)

# Forty-third set: SetWeatherRGBColor with out-of-range parts, each set from the same
# start (100100100) so the earlier value cannot show through, then read back; and
# SetUIFloat on traits HUDMainMenu has (read first), read back the same frame and later.
# The weather colour and the HUD trait are put back.
_RGB_CASES = [100100100, 100256100, 100300100, 100511100, 100512100, 100999100, 100100256, 100100300,
              100100999, 100300300, 100999999, 256100100, 999100100, 100255255, 100255256]
_HUD_TRAITS = ["visible", "alpha", "x", "y", "width", "height", "depth"]


def _ru(pid, body, text=False):
    return (pid, "ref rPlayer\nref rW8\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"])
            + _form("rW8", "171285") + body, text, None, "")


_RGBREAD = 'SetFunctionValue (sv_Construct "%.0f" (GetWeatherRGBColor rW8 3 1))'

RGB_UI_PROBES = (
    [_ru("rgb.save", 'rPlayer.AuxVarSetFlt "*nvfRC" (GetWeatherRGBColor rW8 3 1)\n' + _RGBREAD, True)]
    + [_ru(f"rgb.{v}", f"SetWeatherRGBColor rW8 3 1 100100100\nSetWeatherRGBColor rW8 3 1 {v}\n" + _RGBREAD, True) for v in _RGB_CASES]
    + [_ru("rgb.restore", 'SetWeatherRGBColor rW8 3 1 (rPlayer.AuxVarGetFlt "*nvfRC")\nrPlayer.AuxVarErase "*nvfRC"\n' + _RGBREAD, True)]
    + [_ru(f"ui.hud.{t}", f'SetFunctionValue (GetUIFloat "HUDMainMenu/{t}")') for t in _HUD_TRAITS]
    + [_ru("ui.alpha.save", 'rPlayer.AuxVarSetFlt "*nvfUA" (GetUIFloat "HUDMainMenu/alpha")\nSetFunctionValue 1'),
       _ru("ui.alpha.set", 'SetUIFloat "HUDMainMenu/alpha" 128\nSetFunctionValue (GetUIFloat "HUDMainMenu/alpha")'),
       _ru("ui.alpha.next", 'SetFunctionValue (GetUIFloat "HUDMainMenu/alpha")')]
    + [_ru(f"ui.alpha.wait{i}", "SetFunctionValue 1") for i in range(3)]
    + [_ru("ui.alpha.later", 'SetFunctionValue (GetUIFloat "HUDMainMenu/alpha")'),
       _ru("ui.alpha.restore", 'SetUIFloat "HUDMainMenu/alpha" (rPlayer.AuxVarGetFlt "*nvfUA")\nrPlayer.AuxVarErase "*nvfUA"\n'
           'SetFunctionValue (GetUIFloat "HUDMainMenu/alpha")'),
       _ru("ui.depth.set", 'let fRes := GetUIFloat "HUDMainMenu/depth"\nSetUIFloat "HUDMainMenu/depth" (fRes + 1)\n'
           'let fRes := (GetUIFloat "HUDMainMenu/depth") - fRes\nSetUIFloat "HUDMainMenu/depth" ((GetUIFloat "HUDMainMenu/depth") - 1)\n'
           "SetFunctionValue fRes"),
       _ru("ui.missingtrait.set", 'SetUIFloat "HUDMainMenu/nvfnone" 5\nSetFunctionValue (GetUIFloat "HUDMainMenu/nvfnone")')]
)

# Forty-fourth set: the functions of docs/handoff/modlist-scan-2026-10-26.md's 40 most
# used that no set had called: SetUIString, SetUIStringAlt, SetUIFloatAlt (on the HUD's
# hardcore Dehydration text, a path the GetUIString page names [documented], and on
# HUDMainMenu's alpha), GetLoadedTypeArray, GetCrosshairRef, StopSound (UIVATSLoop
# 15511B, fnvtool esm inventory [observed]). UI values are put back.
FX.update(vatsloop="15511B")
_DEHY = "HUDMainMenu/HardcoreMode/Dehydration/string"


def _g1(pid, body, text=False):
    return (pid, "ref rPlayer\nref rObj\nfloat fRes\nstring_var sText\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + body, text, None, "")


GAPS1_PROBES = [
    # UI strings.
    _g1("ui.dehy.get", f'SetFunctionValue (GetUIString "{_DEHY}")', True),
    _g1("ui.dehy.get.backslash", 'SetFunctionValue (GetUIString "HUDMainMenu\\HardcoreMode\\Dehydration\\string")', True),
    _g1("ui.dehy.save", f'let sText := GetUIString "{_DEHY}"\nrPlayer.AuxVarSetStr "*nvfUS" sText\nSetFunctionValue 1'),
    _g1("ui.setuistring", f'SetUIString "{_DEHY}" "NVF one"\nSetFunctionValue (GetUIString "{_DEHY}")', True),
    _g1("ui.setuistringalt", f'SetFunctionValue (SetUIStringAlt "{_DEHY}" "NVF %g" 2)'),
    _g1("ui.setuistringalt.read", f'SetFunctionValue (GetUIString "{_DEHY}")', True),
    _g1("ui.setuistringalt.missing", 'SetFunctionValue (SetUIStringAlt "HUDMainMenu/NVFNone/string" "x")'),
    # "%z" with a string_var put the variable's name ("sText") in the trait in the game,
    # so a row reads that, and the text read first (H20) is put back as a literal.
    _g1("ui.stringalt.z", f'let sText := rPlayer.AuxVarGetStr "*nvfUS"\nSetUIStringAlt "{_DEHY}" "%z" sText\n'
        f'SetFunctionValue (GetUIString "{_DEHY}")', True),
    _g1("ui.dehy.restore", f'SetUIStringAlt "{_DEHY}" "H20"\nrPlayer.AuxVarErase "*nvfUS"\n'
        f'SetFunctionValue (GetUIString "{_DEHY}")', True),
    # UI float, Alt form.
    _g1("ui.floatalt.set", 'SetFunctionValue (SetUIFloatAlt "HUDMainMenu/alpha" 128)'),
    _g1("ui.floatalt.read", 'SetFunctionValue (GetUIFloat "HUDMainMenu/alpha")'),
    _g1("ui.floatalt.same", 'SetFunctionValue (SetUIFloatAlt "HUDMainMenu/alpha" 128)'),
    _g1("ui.floatalt.missing", 'SetFunctionValue (SetUIFloatAlt "HUDMainMenu/nvfnone" 1)'),
    _g1("ui.floatalt.restore", 'SetUIFloatAlt "HUDMainMenu/alpha" 255\nSetFunctionValue (GetUIFloat "HUDMainMenu/alpha")'),
    # GetLoadedTypeArray: form type 40 weapons, 47 ingestibles; all plugins and one.
    _g1("glta.weap", "let aA := GetLoadedTypeArray 40\nSetFunctionValue (ar_Size aA)"),
    _g1("glta.weap.esm", "let aA := GetLoadedTypeArray 40 0\nSetFunctionValue (ar_Size aA)"),
    _g1("glta.weap.first", "let aA := GetLoadedTypeArray 40 0\nlet rObj := aA[0]\nSetFunctionValue (GetFormIDString rObj)", True),
    _g1("glta.alch", "let aA := GetLoadedTypeArray 47\nSetFunctionValue (ar_Size aA)"),
    _g1("glta.probe", 'let aA := GetLoadedTypeArray 40 (GetModIndex "NVFExtProbe.esp")\nSetFunctionValue (ar_Size aA)'),
    _g1("glta.alias", "let aA := GLTA 47 0\nSetFunctionValue (ar_Size aA)"),
    # Crosshair.
    _g1("crosshair", "SetFunctionValue (GetFormIDString (GetCrosshairRef))", True),
    # StopSound on a looping sound.
    _g1("sound.loop.play", _form("rObj", FX["vatsloop"]) + "PlaySound rObj\nSetFunctionValue (IsSoundPlaying rObj)"),
    _g1("sound.loop.next", _form("rObj", FX["vatsloop"]) + "SetFunctionValue (IsSoundPlaying rObj)"),
    _g1("sound.loop.stop", _form("rObj", FX["vatsloop"]) + "StopSound rObj\nSetFunctionValue (IsSoundPlaying rObj)"),
    _g1("sound.loop.stopped", _form("rObj", FX["vatsloop"]) + "SetFunctionValue (IsSoundPlaying rObj)"),
]

# Forty-fifth set: the most used functions of docs/handoff/modlist-function-counts.json
# that were neither built nor probed, taking the ones a script can read or set safely
# (menus, pauses and dialogue left out), and whether a stimpak used by the player raises
# OnMagicEffectHit on the player (the extender core asked). Forms from fnvtool esm
# inventory [observed]: Ammo10mmJHPHandLoad 140AA8, AmmoEffectMagnumDTReduction 17BA46,
# VStreetCrBrahmin2 (CREA) 17B587, Toughness 031DE0, GameHour 000038,
# VLootAmmoBoxGen30 167815 holding 02FE5D. Changes are put back or session only.
FX.update(jhp="140AA8", amef="17BA46")


def _g2(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g2v(pid, expr, text=False):
    return _g2(pid, f"SetFunctionValue ({expr})", text)


GAPS2_PROBES = [
    # Effects of an ingestible.
    _g2("eff.stim.num", _form("rObj", NF["stimpak"]) + "SetFunctionValue (GetNumEffects rObj)"),
    _g2("eff.stim.0", _form("rObj", NF["stimpak"]) + "SetFunctionValue (GetFormIDString (GetNthEffectBase rObj 0))", True),
    _g2("eff.stim.past", _form("rObj", NF["stimpak"]) + "SetFunctionValue (GetFormIDString (GetNthEffectBase rObj 9))", True),
    # Ammo effects.
    _g2("ammo.jhp.num", _form("rObj", FX["jhp"]) + "SetFunctionValue (GetNumAmmoEffects rObj)"),
    _g2("ammo.jhp.0", _form("rObj", FX["jhp"]) + "SetFunctionValue (GetFormIDString (GetNthAmmoEffect rObj 0))", True),
    _g2("ammo.10mm.num", _form("rObj", NF["ammo10"]) + "SetFunctionValue (GetNumAmmoEffects rObj)"),
] + [_g2(f"amef.trait.{i}", _form("rObj", FX["amef"]) + f"SetFunctionValue (GetAmmoEffectTraitNumeric rObj {i})") for i in range(4)] + [
    # Globals, perks, creatures.
    _g2("global.gamehour.int", _form("rObj", FX["gamehour"]) + "SetFunctionValue (Floor (GetGlobalVariable rObj))"),
] + [_g2(f"perkflag.toughness.{i}", _form("rObj", FX["toughness"]) + f"SetFunctionValue (GetPerkFlag rObj {i})") for i in range(5)] + [
    _g2("creaturetype.brahmin", _form("rObj", FX["brahmin"]) + "SetFunctionValue (GetCreatureType rObj)"),
    _g2v("creaturetype.doc", "rDoc.GetCreatureType"),
    _g2("pickupsound.stim", _form("rObj", NF["stimpak"]) + "SetFunctionValue (GetFormIDString (GetPickupSound rObj))", True),
    _g2("pickupsound.pistol", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetFormIDString (GetPickupSound rObj))", True),
    _g2v("optionalpatch.none", 'GetOptionalPatch "NVFNoSuchPatch"'),
    # Inventory.
    _g2v("invweight.player", "Floor (rPlayer.GetInventoryWeight)"),
    _g2v("invweight.player.playable", "Floor (rPlayer.GetInventoryWeight 1)"),
    _g2("allitems.player", "let aA := rPlayer.GetAllItems\nSetFunctionValue (ar_Size aA)"),
    _g2("allitems.player.weapons", "let aA := rPlayer.GetAllItems 40\nSetFunctionValue (ar_Size aA)"),
    _g2("equippeditemref.none", "SetFunctionValue (GetFormIDString (rPlayer.GetEquippedItemRef 5))", True),
    _g2("dps.pistol", _form("rObj", NF["pistol10"]) + "SetFunctionValue (Floor ((rPlayer.GetCalculatedWeaponDPS rObj) * 100))"),
    _g2("basecontainer.remove", _form("rObj", FX["ammobox"]) + _form("rOther", FX["boxlist"])
        + "BaseRemoveItem rOther 1 rObj\nSetFunctionValue 1"),
    # Actor values, teammates, distance, combat.
    _g2v("avmod.player.health", 'rPlayer.GetActorValueModifier Health'),
    _g2("teammates", "let aA := GetTeammates\nSetFunctionValue (ar_Size aA)"),
    _g2v("distance2d.doc", "Floor (rPlayer.GetDistance2D rDoc)"),
    _g2v("crosshairwater", "GetFormIDString (GetCrosshairWater)", True),
    # Names and toggles, put back.
    _g2("fullnamealt", 'let fRes := 1\nrDoc.SetFullNameAlt "NVF Alt Doc"\nSetFunctionValue (rDoc.GetName)', True),
    _g2("fullnamealt.base", _form("rObj", NF["doc"]) + 'SetFunctionValue (GetName rObj)', True),
    _g2("fullnamealt.restore", 'rDoc.SetFullNameAlt "Doc Mitchell"\nSetFunctionValue (rDoc.GetName)', True),
    _g2v("pipboylight.state", "TogglePipBoyLight"),
    _g2("pipboylight.on", "TogglePipBoyLight 1\nSetFunctionValue (TogglePipBoyLight)"),
    _g2("pipboylight.off", "TogglePipBoyLight 0\nSetFunctionValue (TogglePipBoyLight)"),
    _g2v("pipboylight.off.next", "TogglePipBoyLight"),
    _g2("combatdisabled", "SetFunctionValue (rDoc.SetCombatDisabled 1)"),
    _g2("combatdisabled.again", "SetFunctionValue (rDoc.SetCombatDisabled 0)"),
    _g2v("hasscriptblock.doc.gamemode", "rDoc.HasScriptBlock 0"),
    _g2v("isanimplayingex.player", "rPlayer.IsAnimPlayingEx 0 0 0"),
    # Does a stimpak the player uses raise OnMagicEffectHit on the player?
    _g2("mgef.reg", 'SetEventHandler "OnMagicEffectHit" (Begin Function {ref rE1, ref rE2}\n'
        '  rPlayer.AuxVarSetFlt "*nvfMG" (rE1 == rPlayer) -1\n  rPlayer.AuxVarSetRef "*nvfMG.e" rE2 -1\nEnd)\nSetFunctionValue 1'),
    _g2("mgef.damage", 'rPlayer.DamageAV Health 30\nSetFunctionValue 1'),
    _g2("mgef.use", _form("rObj", NF["stimpak"]) + "rPlayer.EquipItem rObj 0 1\nSetFunctionValue 1"),
] + [_g2(f"mgef.wait{i}", "SetFunctionValue 1") for i in range(3)] + [
    _g2v("mgef.fired", '(rPlayer.AuxVarSize "*nvfMG") > 0'),
    _g2v("mgef.onplayer", 'rPlayer.AuxVarGetFlt "*nvfMG" 0'),
    _g2v("mgef.effect", 'GetFormIDString (rPlayer.AuxVarGetRef "*nvfMG.e" 0)', True),
    _g2("mgef.cleanup", 'rPlayer.RestoreAV Health 100\nrPlayer.AuxVarErase "*nvfMG"\nrPlayer.AuxVarErase "*nvfMG.e"\nSetFunctionValue 1'),
]

# Forty-sixth set: the next unbuilt, unprobed functions of
# docs/handoff/modlist-function-counts.json that a script can read or set safely. Left
# out: menus and the cursor's movement, pauses, FakeScriptEvent (runs another script's
# block), output-variable forms (WorldToScreen), 3D-only reads. The base effect is the
# stimpak's first (0000014E, gaps2); TimeScale is FalloutNV.esm 00003A. Changes are put
# back or session only.
def _g3(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nfloat fRes\nstring_var sText\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g3v(pid, expr, text=False):
    return _g3(pid, f"SetFunctionValue ({expr})", text)


_MGEF = _form("rObj", "14E")
GAPS3_PROBES = [
    _g3v("menu.active", "GetActiveMenuMode"),
    _g3v("time.multiplier", "GetGlobalTimeMultiplier"),
] + [_g3(f"actors.level{i}", f"let aA := GetActorsByProcessingLevel {i}\nSetFunctionValue (ar_Size aA)") for i in range(3)] + [
    # Level 3 holds about 2,000 actors and its count moved by one between two captures.
    _g3("actors.level3.any", "let aA := GetActorsByProcessingLevel 3\nSetFunctionValue ((ar_Size aA) > 1000)"),
    _g3("folders.nvse", 'let aA := GetFoldersInFolder "NVSE"\nSetFunctionValue (ar_Size aA)'),
    _g3("folders.nvse.first", 'let aA := GetFoldersInFolder "NVSE"\nlet sText := aA[0]\nSetFunctionValue sText', True),
    _g3("folders.none", 'let aA := GetFoldersInFolder "NVFNoFolder"\nSetFunctionValue (ar_Size aA)'),
    _g3v("ground.player", "rPlayer.GetGroundMaterial"),
    _g3v("ragdolled.player", "rPlayer.GetIsRagdolled"),
    _g3(f"perk.entries", _form("rObj", FX["toughness"]) + "SetFunctionValue (GetPerkEntryCount rObj)"),
    _g3(f"perk.entry0.type", _form("rObj", FX["toughness"]) + "SetFunctionValue (GetNthPerkEntryType rObj 0)"),
    _g3("mgef.archetype", _MGEF + "SetFunctionValue (GetBaseEffectArchetype rObj)"),
    _g3("mgef.av", _MGEF + "SetFunctionValue (GetBaseEffectAV rObj)"),
] + [_g3(f"mgef.flag.{i}", _MGEF + f"SetFunctionValue (GetBaseEffectFlag rObj {i})") for i in range(4)] + [
] + [_g3(f"effectflag.stim.{i}", _form("rObj", NF["stimpak"]) + f"SetFunctionValue (GetEffectFlag rObj {i})") for i in range(3)] + [
] + [_g3(f"cellflag.doc.{i}", _form("rObj", CELLS["doc"]) + f"SetFunctionValue (GetCellFlag rObj {i})") for i in range(4)] + [
    _g3("keywordforms", _form("rObj", NF["doc"]) + 'AssignKeyword rObj "NVFGapKey"\nlet aA := GetKeywordForms "NVFGapKey"\nSetFunctionValue (ar_Size aA)'),
    _g3("keywordforms.none", 'let aA := GetKeywordForms "NVFNoKey"\nSetFunctionValue (ar_Size aA)'),
    _g3("global.set", _form("rObj", "3A") + 'let fRes := GetGlobalVariable rObj\nrPlayer.AuxVarSetFlt "*nvfGV" fRes\n'
        "SetFunctionValue (SetGlobalVariable rObj 12)"),
    _g3("global.set.read", _form("rObj", "3A") + "SetFunctionValue (GetGlobalVariable rObj)"),
    _g3("global.restore", _form("rObj", "3A") + 'SetGlobalVariable rObj (rPlayer.AuxVarGetFlt "*nvfGV")\nrPlayer.AuxVarErase "*nvfGV"\n'
        "SetFunctionValue (GetGlobalVariable rObj)"),
    _g3v("combatdisabled.before", "rDoc.GetCombatDisabled"),
    _g3("combatdisabled.set", "rDoc.SetCombatDisabled 1\nSetFunctionValue (rDoc.GetCombatDisabled)"),
    _g3("combatdisabled.clear", "rDoc.SetCombatDisabled 0\nSetFunctionValue (rDoc.GetCombatDisabled)"),
    _g3("effectshader.none", _form("rObj", "14E") + "SetFunctionValue (rPlayer.HasEffectShader rObj)"),
    _g3("destruction.pistol", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetDestructionDataNumStages rObj)"),
    _g3("cursor.x", "SetFunctionValue (GetCursorPos X)"),
    _g3("biped.slot2", "SetFunctionValue (ToggleBipedSlotVisibility 2)"),
    _g3("reloadmodels", "rPlayer.ReloadEquippedModels\nSetFunctionValue 1"),
    # SetWeaponOut on the player with the pistol equipped.
    _g3("weaponout.add", _form("rObj", NF["pistol10"]) + "rPlayer.AddItem rObj 1 1\nrPlayer.EquipItem rObj 0 1\nSetFunctionValue 1"),
    _g3("weaponout.draw", "rPlayer.SetWeaponOut 1\nSetFunctionValue (rPlayer.IsWeaponOut)"),
] + [_g3(f"weaponout.wait{i}", "SetFunctionValue 1") for i in range(5)] + [
    _g3v("weaponout.drawn", "rPlayer.IsWeaponOut"),
    _g3("weaponout.holster", "rPlayer.SetWeaponOut 0\nSetFunctionValue 1"),
] + [_g3(f"weaponout.wait.h{i}", "SetFunctionValue 1") for i in range(5)] + [
    _g3v("weaponout.holstered", "rPlayer.IsWeaponOut"),
    _g3("weaponout.cleanup", _form("rObj", NF["pistol10"]) + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1"),
    # DropAlt.
    _g3("dropalt", _form("rObj", NF["stimpak"]) + "let fRes := rPlayer.GetItemCount rObj\nrPlayer.DropAlt rObj 1\nSetFunctionValue fRes"),
    _g3v("dropalt.after", "rPlayer.GetItemCount (GetFormFromMod \"FalloutNV.esm\" \"15169\")"),
]

# Forty-seventh set: the next unbuilt, unprobed functions of
# docs/handoff/modlist-function-counts.json read or set safely. Climate rows name a climate
# (NVtheStripClimate 161242) so they do not depend on the save; gaps4.climate.current is
# the one save row. Forms from fnvtool esm inventory [observed]: Recipe127mm 17BA3E,
# musDocMitchellHouse (MUSC) 171B4A, VStreetCrBrahmin2 (CREA) 17B587. Changes put back.
FX.update(stripclimate="161242", recipe127="17BA3E", musdoc="171B4A")


def _g4(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g4v(pid, expr, text=False):
    return _g4(pid, f"SetFunctionValue ({expr})", text)


_CLM = _form("rObj", FX["stripclimate"])
GAPS4_PROBES = [
    _g4v("laa", "GetIsLAA"),
    _g4v("climate.current", "GetFormIDString (GetCurrentClimate)", True),
    _g4("climate.num", _CLM + "SetFunctionValue (GetClimateNumWeatherTypes rObj)"),
    _g4("climate.weather0", _CLM + "SetFunctionValue (GetFormIDString (GetClimateNthWeatherType rObj 0))", True),
    _g4("climate.global0", _CLM + "SetFunctionValue (GetFormIDString (GetClimateNthWeatherGlobal rObj 0))", True),
    _g4("climate.weather.past", _CLM + "SetFunctionValue (GetFormIDString (GetClimateNthWeatherType rObj 99))", True),
    _g4("isnight.strip", _CLM + "SetFunctionValue (IsNight rObj)"),
    _g4("bodypart.brahmin", _form("rObj", FX["brahmin"]) + "SetFunctionValue (GetFormIDString (GetBodyPartData rObj))", True),
    _g4("formrefs.stimpak", _form("rObj", NF["stimpak"]) + "let aA := GetFormRefs rObj\nSetFunctionValue (ar_Size aA)"),
    _g4("formrefs.docbase", _form("rObj", NF["doc"]) + "let aA := GetFormRefs rObj 1\nSetFunctionValue (ar_Size aA)"),
    _g4v("falltime.player", "rPlayer.GetFallTimeElapsed"),
    _g4v("attacking.player", "rPlayer.IsAttacking"),
    _g4v("combatwith.doc", "rDoc.IsInCombatWith rPlayer"),
    _g4v("crimeorenemy.doc", "rDoc.IsCrimeOrEnemy"),
    _g4v("runspeed.player.int", "Floor (rPlayer.GetRunSpeed)"),
    _g4v("activequest", "GetFormIDString (GetActiveQuest)", True),
    _g4(f"perk.entry0.function", _form("rObj", FX["toughness"]) + "SetFunctionValue (GetNthPerkEntryFunction rObj 0)"),
    _g4("cell.imagespace.doc", _form("rObj", CELLS["doc"]) + "SetFunctionValue (GetFormIDString (GetCellImageSpace rObj))", True),
    _g4("recipe.outputs", _form("rObj", FX["recipe127"]) + "let aA := GetRecipeOutputForms rObj\nSetFunctionValue (ar_Size aA)"),
    _g4("recipe.output0", _form("rObj", FX["recipe127"]) + "let aA := GetRecipeOutputForms rObj\nlet rOther := aA[0]\n"
        "SetFunctionValue (GetFormIDString rOther)", True),
    _g4("music.playing", _form("rObj", FX["musdoc"]) + "SetFunctionValue (IsMusicPlaying rObj)"),
    _g4v("music.playing.any", "IsMusicPlaying"),
    # Weapon flags and sounds, put back.
] + [_g4(f"weaponflag.pistol.{i}", _form("rObj", NF["pistol10"]) + f"SetFunctionValue (WeaponHasFlag {i} rObj)") for i in range(6)] + [
    _g4("weaponflag.set", _form("rObj", NF["pistol10"]) + "let fRes := WeaponHasFlag 5 rObj\nSetWeaponFlag 5 1 rObj\n"
        "let fRes := fRes * 10 + (WeaponHasFlag 5 rObj)\nSetWeaponFlag 5 0 rObj\nSetFunctionValue fRes"),
] + [_g4(f"weaponsound.pistol.{i}", _form("rObj", NF["pistol10"]) + f"SetFunctionValue (GetFormIDString (GetWeaponSound rObj {i}))", True)
     for i in range(4)] + [
    _g4("weaponsound.set", _form("rObj", NF["pistol10"]) + _form("rOther", FX["beep"])
        + 'let rDoc := GetWeaponSound rObj 0\nrPlayer.AuxVarSetRef "*nvfWS" rDoc\nSetWeaponSound rObj 0 rOther\n'
        "SetFunctionValue (GetFormIDString (GetWeaponSound rObj 0))", True),
    _g4("weaponsound.restore", _form("rObj", NF["pistol10"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfWS"\nSetWeaponSound rObj 0 rOther\n'
        'rPlayer.AuxVarErase "*nvfWS"\nSetFunctionValue (GetFormIDString (GetWeaponSound rObj 0))', True),
    # Sound flags.
    _g4("soundflag.set", _form("rObj", FX["beep"]) + "SetFunctionValue (SetSoundFlag rObj 0 1)"),
    _g4("soundflag.clear", _form("rObj", FX["beep"]) + "SetFunctionValue (SetSoundFlag rObj 0 0)"),
    _g4("stopsoundalt.none", _form("rObj", FX["beep"]) + "SetFunctionValue (StopSoundAlt rObj rPlayer)"),
    # Reference names and gravity.
    _g4("refname.set", 'rDoc.SetRefName "NVF Ref Doc"\nSetFunctionValue (rDoc.GetName)', True),
    _g4("refname.base", _form("rObj", NF["doc"]) + "SetFunctionValue (GetName rObj)", True),
    _g4("gravity.set", "rPlayer.SetActorGravityMult 1\nSetFunctionValue 1"),
    _g4v("optionalpatch.set.none", 'SetOptionalPatch "NVFNoSuchPatch" 1'),
]

# Forty-eighth set: what the extender core asked after building gaps2-4: the save's Guns
# skill and hardcore state (DPS and the stimpak's effects depend on them), weapon sound
# IDs 3-11 on weapons that have more sounds (WeapMinigun 00433F, WeapGatlingLaser 00432E,
# WeapFlamer 00432D), perk entry types and functions on more perks, GetActorValueModifier
# after damage by type filter, and GetRunSpeed sneaking and armoured. Forms from
# fnvtool esm inventory [observed]. Damage, sneak and armour are put back.
_G5_PERKS = {"leadbelly": "44CA9", "intensetraining": "44CB1", "educated": "31DD8", "radresistance": "31DA9",
             "smallframe": "135EC6", "commando": "99828"}


def _g5(pid, body, text=False):
    return (pid, "ref rPlayer\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"]) + body, text, None, "")


def _g5v(pid, expr, text=False):
    return _g5(pid, f"SetFunctionValue ({expr})", text)


GAPS5_PROBES = (
    [_g5v("save.guns", 'rPlayer.GetActorValue "Guns"'),
     _g5v("save.guns.base", 'rPlayer.GetBaseActorValue "Guns"'),
     _g5v("save.hardcore", "rPlayer.IsHardcore"),
     _g5v("save.level", "rPlayer.GetLevel")]
    + [_g5(f"weapsound.{w}.{i}", _form("rObj", fid) + f"SetFunctionValue (GetFormIDString (GetWeaponSound rObj {i}))", True)
       for w, fid in (("minigun", "433F"), ("gatling", "432E"), ("flamer", "432D")) for i in range(12)]
    + [_g5(f"perk.{k}.{what}.{i}", _form("rObj", fid) + f"SetFunctionValue ({fn} rObj {i})")
       for k, fid in _G5_PERKS.items() for i in range(2)
       for what, fn in (("type", "GetNthPerkEntryType"), ("function", "GetNthPerkEntryFunction"))]
    + [_g5(f"perk.{k}.count", _form("rObj", fid) + "SetFunctionValue (GetPerkEntryCount rObj)") for k, fid in _G5_PERKS.items()]
    # Actor value modifiers after damage.
    + [_g5v(f"avmod.before.{f}", f"rPlayer.GetActorValueModifier Health {f}") for f in (0, 1, 2, 3)]
    + [_g5("avmod.damage", "rPlayer.DamageAV Health 30\nSetFunctionValue (rPlayer.GetAV Health)")]
    + [_g5v(f"avmod.after.{f}", f"rPlayer.GetActorValueModifier Health {f}") for f in (0, 1, 2, 3)]
    + [_g5("avmod.restore", "rPlayer.RestoreAV Health 30\nSetFunctionValue (rPlayer.GetActorValueModifier Health)")]
    # Run speed: standing, sneaking, armoured.
    + [_g5v("run.base", "Floor (rPlayer.GetRunSpeed)"),
       _g5("run.sneak.on", "rPlayer.SetForceSneak 1\nSetFunctionValue 1")]
    + [_g5(f"run.sneak.wait{i}", "SetFunctionValue 1") for i in range(3)]
    + [_g5v("run.sneak", "Floor (rPlayer.GetRunSpeed)"),
       _g5v("run.sneak.issneaking", "rPlayer.IsSneaking"),
       _g5("run.sneak.off", "rPlayer.SetForceSneak 0\nSetFunctionValue 1")]
    + [_g5(f"run.sneak.offwait{i}", "SetFunctionValue 1") for i in range(3)]
    + [_g5v("run.sneak.after", "Floor (rPlayer.GetRunSpeed)"),
       _g5("run.armour.on", _form("rObj", NF["leather"]) + "rPlayer.AddItem rObj 1 1\nrPlayer.EquipItem rObj 0 1\nSetFunctionValue 1")]
    + [_g5(f"run.armour.wait{i}", "SetFunctionValue 1") for i in range(2)]
    + [_g5v("run.armour", "Floor (rPlayer.GetRunSpeed)"),
       _g5("run.armour.off", _form("rObj", NF["leather"]) + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\nSetFunctionValue 1")]
    + [_g5(f"run.armour.offwait{i}", "SetFunctionValue 1") for i in range(2)]
    + [_g5v("run.armour.after", "Floor (rPlayer.GetRunSpeed)")]
)

# Forty-ninth set: GetCalculatedWeaponDPS on the skill curve (the 10mm pistol at Guns
# 17, 50 and 100; WeapKnifeCombat 004326 at the save's skills and at Melee Weapons 50),
# as the extender core asked, then the next unprobed JIP functions of the counts that
# have a getter or a visible effect. The 10mm pistol's projectile is 02CD5F (projectile
# set); MineFragExplosion 07078B; EnchFourEyesHat 17B78D. Skills, values, flags and the
# player's limb are put back; the perk description and the pistol's effect are session
# only (a string_var cannot be passed back, gaps1).
FX.update(knife="4326", fourenc="17B78D", pistolproj="2CD5F")


def _g6(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\nstring_var sText\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g6v(pid, expr, text=False):
    return _g6(pid, f"SetFunctionValue ({expr})", text)


_DPS = "SetFunctionValue (Floor ((rPlayer.GetCalculatedWeaponDPS rObj) * 100))"
GAPS6_PROBES = [
    # DPS on the skill curve. SetActorValue on a skill read back doubled in a first run
    # (17 read 34), so each point records GetActorValue and GetBaseActorValue too, and
    # the skill is put back with ModActorValue by the difference from the value read first.
    _g6("dps.guns.save", 'let fRes := rPlayer.GetActorValue "Guns"\nrPlayer.AuxVarSetFlt "*nvfGS" fRes\nSetFunctionValue fRes'),
    _g6("dps.pistol.start", _form("rObj", NF["pistol10"]) + _DPS),
    _g6("dps.guns.set50", 'rPlayer.SetActorValue "Guns" 50\n'
        'SetFunctionValue (sv_Construct "%g %g" (rPlayer.GetActorValue "Guns") (rPlayer.GetBaseActorValue "Guns"))', True),
    _g6("dps.pistol.at50", _form("rObj", NF["pistol10"]) + _DPS),
    _g6("dps.guns.set100", 'rPlayer.SetActorValue "Guns" 100\n'
        'SetFunctionValue (sv_Construct "%g %g" (rPlayer.GetActorValue "Guns") (rPlayer.GetBaseActorValue "Guns"))', True),
    _g6("dps.pistol.at100", _form("rObj", NF["pistol10"]) + _DPS),
    _g6("dps.guns.restore", 'rPlayer.ModActorValue "Guns" ((rPlayer.AuxVarGetFlt "*nvfGS") - (rPlayer.GetActorValue "Guns"))\n'
        'rPlayer.AuxVarErase "*nvfGS"\nSetFunctionValue (rPlayer.GetActorValue "Guns")'),
    _g6("dps.pistol.again", _form("rObj", NF["pistol10"]) + _DPS),
    _g6("dps.melee.save", 'let fRes := rPlayer.GetActorValue "MeleeWeapons"\nrPlayer.AuxVarSetFlt "*nvfMS" fRes\nSetFunctionValue fRes'),
    _g6("dps.knife", _form("rObj", FX["knife"]) + _DPS),
    _g6("dps.knife.damage", _form("rObj", FX["knife"]) + "SetFunctionValue (Floor ((GetCalculatedWeaponDamage rObj) * 100))"),
    _g6("dps.melee.mod16", 'rPlayer.ModActorValue "MeleeWeapons" 16\nSetFunctionValue (rPlayer.GetActorValue "MeleeWeapons")'),
    _g6("dps.knife.at.mod16", _form("rObj", FX["knife"]) + _DPS),
    _g6("dps.melee.restore", 'rPlayer.ModActorValue "MeleeWeapons" ((rPlayer.AuxVarGetFlt "*nvfMS") - (rPlayer.GetActorValue "MeleeWeapons"))\n'
        'rPlayer.AuxVarErase "*nvfMS"\nSetFunctionValue (rPlayer.GetActorValue "MeleeWeapons")'),
    # Effect traits.
] + [_g6(f"efftrait.stim.0.{t}", _form("rObj", NF["stimpak"]) + f"SetFunctionValue (GetNthEffectTraitNumeric rObj 0 {t})") for t in range(4)] + [
    _g6("efftrait.set", _form("rObj", NF["stimpak"]) + "let fRes := GetNthEffectTraitNumeric rObj 0 0\nrPlayer.AuxVarSetFlt \"*nvfET\" fRes\n"
        "SetFunctionValue (SetNthEffectTraitNumeric rObj 0 0 77)"),
    _g6("efftrait.set.read", _form("rObj", NF["stimpak"]) + "SetFunctionValue (GetNthEffectTraitNumeric rObj 0 0)"),
    _g6("efftrait.restore", _form("rObj", NF["stimpak"]) + 'SetNthEffectTraitNumeric rObj 0 0 (rPlayer.AuxVarGetFlt "*nvfET")\n'
        'rPlayer.AuxVarErase "*nvfET"\nSetFunctionValue (GetNthEffectTraitNumeric rObj 0 0)'),
    # Description, value, object effect.
    _g6("desc.toughness", _form("rObj", FX["toughness"]) + "SetFunctionValue (sv_Length (GetFormDescription rObj))"),
    _g6("desc.set", _form("rObj", FX["toughness"]) + 'SetFunctionValue (SetFormDescription rObj "NVF desc %g" 3)'),
    _g6("desc.set.read", _form("rObj", FX["toughness"]) + "SetFunctionValue (GetFormDescription rObj)", True),
    # SetValueAlt returned 0 in a first run, not the old value: the value is read first.
    _g6("value.set", _form("rObj", NF["pistol10"]) + 'let fRes := GetValue rObj\nrPlayer.AuxVarSetFlt "*nvfVA" fRes\n'
        "SetFunctionValue (SetValueAlt rObj 999)"),
    _g6("value.set.read", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetValue rObj)"),
    _g6("value.restore", _form("rObj", NF["pistol10"]) + 'SetValueAlt rObj (rPlayer.AuxVarGetFlt "*nvfVA")\nrPlayer.AuxVarErase "*nvfVA"\n'
        "SetFunctionValue (GetValue rObj)"),
    _g6("objeffect.pistol", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetFormIDString (GetObjectEffect rObj))", True),
    _g6("objeffect.set", _form("rObj", NF["pistol10"]) + _form("rOther", FX["fourenc"]) + "SetFunctionValue (SetObjectEffect rObj rOther)"),
    _g6("objeffect.set.read", _form("rObj", NF["pistol10"]) + "SetFunctionValue (GetFormIDString (GetObjectEffect rObj))", True),
    # Projectile flags and explosion.
] + [_g6(f"projflag.pistol.{i}", _form("rObj", FX["pistolproj"]) + f"SetFunctionValue (GetProjectileFlag rObj {i})") for i in range(6)] + [
    _g6("projflag.set", _form("rObj", FX["pistolproj"]) + "let fRes := GetProjectileFlag rObj 3\nSetProjectileFlag rObj 3 1\n"
        "let fRes := fRes * 10 + (GetProjectileFlag rObj 3)\nSetProjectileFlag rObj 3 0\nSetFunctionValue fRes"),
    _g6("projexplosion", _form("rObj", FX["pistolproj"]) + "SetFunctionValue (GetFormIDString (GetProjectileExplosion rObj))", True),
    _g6("projexplosion.set", _form("rObj", FX["pistolproj"]) + _form("rOther", "7078B")
        + 'let rDoc := GetProjectileExplosion rObj\nrPlayer.AuxVarSetRef "*nvfPX" rDoc\n'
        "SetProjectileExplosion rObj rOther\nSetFunctionValue (GetFormIDString (GetProjectileExplosion rObj))", True),
    _g6("projexplosion.restore", _form("rObj", FX["pistolproj"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfPX"\n'
        'SetProjectileExplosion rObj rOther\nrPlayer.AuxVarErase "*nvfPX"\n'
        "SetFunctionValue (GetFormIDString (GetProjectileExplosion rObj))", True),
    # Aux variables as arrays; ref maps of refs.
    _g6("auxarray.get", 'rPlayer.AuxVarSetFlt "*nvfAA" 1 0\nrPlayer.AuxVarSetFlt "*nvfAA" 2 1\n'
        'let aA := rPlayer.AuxiliaryVariableGetAsArray "*nvfAA"\nSetFunctionValue (ar_Size aA)'),
    _g6("auxarray.set", 'let aA := ar_List 5 6 7\nlet fRes := rPlayer.AuxiliaryVariableSetFromArray "*nvfAB" aA\n'
        'SetFunctionValue (sv_Construct "%g %g %g" fRes (rPlayer.AuxVarSize "*nvfAB") (rPlayer.AuxVarGetFlt "*nvfAB" 2))', True),
    _g6("refmap.setref", 'rDoc.RefMapArraySetRef "*nvfRR" rPlayer\nSetFunctionValue (GetFormIDString (rDoc.RefMapArrayGetRef "*nvfRR"))', True),
    _g6("refmap.erase", 'SetFunctionValue (rDoc.RefMapArrayErase "*nvfRR")'),
    _g6("refmap.erased", 'SetFunctionValue (GetFormIDString (rDoc.RefMapArrayGetRef "*nvfRR"))', True),
    # Actor states.
    _g6v("poisoned.player", "rPlayer.GetIsPoisoned"),
    _g6v("detection.pc", "GetPCDetectionState"),
    _g6v("proclevel.player", "rPlayer.GetActorProcessingLevel"),
    _g6v("proclevel.doc", "rDoc.GetActorProcessingLevel"),
    _g6v("persistent.doc", "rDoc.SetPersistent 1"),
    _g6("cripple.player", "rPlayer.CrippleLimb 2\nSetFunctionValue (rPlayer.GetActorValue LeftAttackCondition)"),
    _g6("cripple.restore", "rPlayer.RestoreAV LeftAttackCondition 100\nSetFunctionValue (rPlayer.GetActorValue LeftAttackCondition)"),
    _g6("stick.disable", "SetStickDisabled 0 1\nSetStickDisabled 0 0\nSetFunctionValue 1"),
    _g6v("uicomponent.hud", 'IsComponentLoaded "HUDMainMenu"'),
    _g6v("uicomponent.none", 'IsComponentLoaded "NVFNoComponent"'),
]

# Fiftieth set: the critical part of GetCalculatedWeaponDPS, as the extender core asked
# (the player's crit actor values, each weapon's crit damage and chance, DPS with crit
# damage 0, crit chance 0 and Luck +5, all put back), then the next unprobed JIP functions
# of the counts with a getter or a visible effect. Forms from fnvtool esm inventory
# [observed]: 40mmGrenadeExplosion 07EA31, BloodyMess 094EBA, SilverSting 167475,
# NukaVendingMachine (CONT) 020525, ElectricBox01 020D31, CrDogVicious 01CFA3,
# Gomorrah00 (CELL) 10D629, VCG01DocMitchellSandbox (PACK) 10B756, LegateArmorRightH
# (ARMA) 1649DF. The explosion, the perk, the weapons and the actor values are put back.
# GetAngleEx and GetAnglesBetweenPoints did not compile here: their outputs are script
# variables, which a runtime-compiled function does not take [observed].
# GetDroppedRefs, GetPlayerPerks, GetFollowers, GetTempEffects and GetDetectingActors read
# ar_Size -1 here: no array at all when there is nothing to list [observed]. ModAV Luck +5
# left CritChance and DPS unchanged after two frames; ModAV CritChance +10 doubled the crit
# part of both weapons' DPS [observed].
FX.update(gren40="7EA31", bloodymess="94EBA", silversting="167475", nuka="20525", ebox="20D31",
          vicious="1CFA3", gomorrah="10D629", docsandbox="10B756", legatearma="1649DF")


def _g7(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\nfloat fAx\nfloat fAy\nfloat fAz\narray_var aA\n"
            "Begin Function { }\n" + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g7v(pid, expr, text=False):
    return _g7(pid, f"SetFunctionValue ({expr})", text)


def _g7w(pid, key, expr, text=False):
    return _g7(pid, _form("rObj", key) + f"SetFunctionValue ({expr})", text)


_DPS7 = "SetFunctionValue (Floor ((rPlayer.GetCalculatedWeaponDPS rObj) * 100))"


def _crit(w, fid):
    """Crit rows for one weapon: its crit fields, DPS, then DPS with crit damage 0 and crit chance 0."""
    return [
        _g7w(f"crit.{w}.critdamage", fid, "GetWeaponCritDamage rObj"),
        _g7w(f"crit.{w}.critchance", fid, "Floor ((GetWeaponCritChance rObj) * 1000)"),
        _g7w(f"crit.{w}.damage", fid, "Floor ((GetCalculatedWeaponDamage rObj) * 100)"),
        _g7(f"crit.{w}.dps", _form("rObj", fid) + _DPS7),
        _g7(f"crit.{w}.dmg0", _form("rObj", fid) + 'let fRes := GetWeaponCritDamage rObj\nrPlayer.AuxVarSetFlt "*nvfCD" fRes\n'
            "SetWeaponCritDamage 0 rObj\n" + _DPS7),
        _g7(f"crit.{w}.dmg0.restore", _form("rObj", fid) + 'SetWeaponCritDamage (rPlayer.AuxVarGetFlt "*nvfCD") rObj\n'
            'rPlayer.AuxVarErase "*nvfCD"\nSetFunctionValue (GetWeaponCritDamage rObj)'),
        _g7(f"crit.{w}.chance0", _form("rObj", fid) + 'let fRes := GetWeaponCritChance rObj\nrPlayer.AuxVarSetFlt "*nvfCC" fRes\n'
            "SetWeaponCritChance 0 rObj\n" + _DPS7),
        _g7(f"crit.{w}.chance0.restore", _form("rObj", fid) + 'SetWeaponCritChance (rPlayer.AuxVarGetFlt "*nvfCC") rObj\n'
            'rPlayer.AuxVarErase "*nvfCC"\nSetFunctionValue (Floor ((GetWeaponCritChance rObj) * 1000))'),
        _g7(f"crit.{w}.dps.again", _form("rObj", fid) + _DPS7),
    ]


GAPS7_PROBES = (
    [_g7v(f"crit.av.{av.lower()}", f'Floor ((rPlayer.GetActorValue "{av}") * 100)')
     for av in ("CritChance", "Luck", "Strength", "MeleeDamage", "UnarmedDamage", "Agility")]
    + _crit("pistol", NF["pistol10"]) + _crit("knife", FX["knife"])
    + [_g7("crit.luck.mod5", 'rPlayer.ModActorValue "Luck" 5\n'
           'SetFunctionValue (sv_Construct "%g %g" (rPlayer.GetActorValue "Luck") (Floor ((rPlayer.GetActorValue "CritChance") * 100)))', True),
       _g7("crit.luck.wait0", "SetFunctionValue 1"),
       _g7("crit.luck.wait1", "SetFunctionValue 1"),
       _g7v("crit.luck.critchance", 'Floor ((rPlayer.GetActorValue "CritChance") * 100)'),
       _g7("crit.pistol.luck5", _form("rObj", NF["pistol10"]) + _DPS7),
       _g7("crit.knife.luck5", _form("rObj", FX["knife"]) + _DPS7),
       _g7("crit.cc.mod10", 'rPlayer.ModActorValue "CritChance" 10\n'
           'SetFunctionValue (Floor ((rPlayer.GetActorValue "CritChance") * 100))'),
       _g7("crit.pistol.cc20", _form("rObj", NF["pistol10"]) + _DPS7),
       _g7("crit.knife.cc20", _form("rObj", FX["knife"]) + _DPS7),
       _g7("crit.cc.restore", 'rPlayer.ModActorValue "CritChance" -10\n'
           'SetFunctionValue (Floor ((rPlayer.GetActorValue "CritChance") * 100))'),
       _g7("crit.luck.restore", 'rPlayer.ModActorValue "Luck" -5\n'
           'SetFunctionValue (sv_Construct "%g %g" (rPlayer.GetActorValue "Luck") (Floor ((rPlayer.GetActorValue "CritChance") * 100)))', True)]
    # Dimensions and terrain.
    + [_g7v(f"objdim.doc.{a}", f"Floor ((rDoc.GetObjectDimensions {a}) * 100)") for a in "XYZ"]
    + [_g7w(f"objdim.nuka.{a}", FX["nuka"], f"Floor ((GetObjectDimensions {a} rObj) * 100)") for a in "XYZ"]
    + [_g7v("terrain.player", "Floor (GetTerrainHeight (rPlayer.GetPos X) (rPlayer.GetPos Y))"),
       _g7v("terrain.far", "Floor (GetTerrainHeight 900000 900000)")]
    # Lists of things.
    + [_g7("baseitems.nuka", _form("rObj", FX["nuka"]) + "let aA := GetBaseItems rObj\nSetFunctionValue (ar_Size aA)"),
       _g7("baseitems.doc", "let aA := rDoc.GetBaseItems\nSetFunctionValue (ar_Size aA)"),
       _g7("droppedrefs.doc", "let aA := rDoc.GetDroppedRefs\nSetFunctionValue (ar_Size aA)"),
       _g7("playerperks", "let aA := GetPlayerPerks\nSetFunctionValue (ar_Size aA)"),
       _g7("followers.player", "let aA := rPlayer.GetFollowers\nSetFunctionValue (ar_Size aA)"),
       _g7("tempeffects.player", "let aA := rPlayer.GetTempEffects\nSetFunctionValue (ar_Size aA)"),
       _g7("detecting.player", "let aA := rPlayer.GetDetectingActors\nSetFunctionValue (ar_Size aA)"),
       _g7("pickfromlist.ammo10", _form("rObj", FX["ammolist10"]) + "let rOther := rPlayer.PickFromList rObj\nSetFunctionValue (GetFormIDString rOther)", True),
       _g7("pickfromlist.list127", _form("rObj", NF["list127"]) + "let rOther := rPlayer.PickFromList rObj\nSetFunctionValue (GetFormIDString rOther)", True)]
    # Item and form kinds.
    + [_g7w("ispoison.silversting", FX["silversting"], "GetIsPoison rObj"),
       _g7w("ispoison.stimpak", NF["stimpak"], "GetIsPoison rObj"),
       _g7w("isarmoraddon.legate", FX["legatearma"], "IsArmorAddon rObj"),
       _g7w("isarmoraddon.leather", NF["leather"], "IsArmorAddon rObj"),
       _g7w("isitem.stimpak", NF["stimpak"], "GetIsItem rObj"),
       _g7w("isitem.nuka", FX["nuka"], "GetIsItem rObj"),
       _g7v("isitem.docref", "rDoc.GetIsItem"),
       _g7w("respawns.nuka", FX["nuka"], "GetContainerRespawns rObj"),
       _g7w("destruction.ebox", FX["ebox"], "GetDestructionDataHealth rObj"),
       _g7w("skeleton.vicious", FX["vicious"], "GetSkeletonModel rObj", True),
       _g7w("skeleton.doc", NF["doc"], "GetSkeletonModel rObj", True),
       _g7w("cellowner.gomorrah", FX["gomorrah"], "GetFormIDString (GetOwnerOfCell rObj)", True),
       _g7w("cellowner.doc", CELLS["doc"], "GetFormIDString (GetOwnerOfCell rObj)", True)]
    + [_g7w(f"packageflag.docsandbox.{i}", FX["docsandbox"], f"GetPackageFlag rObj {i}") for i in (0, 1, 2, 9, 10, 13, 14, 21)]
    # Actor and player state.
    + [_g7v("hctracking", "GetHardcoreTracking"),
       _g7v("alwaysrun", "GetAlwaysRun"),
       _g7v("pcincombat", "IsPCInCombat"),
       _g7v("encumbrance.player", "Floor ((rPlayer.GetEncumbranceRate) * 100)"),
       _g7v("immobile.doc", "rDoc.GetIsImmobile"),
       _g7w("immobile.ebox", FX["ebox"], "GetIsImmobile rObj"),
       _g7v("mobile.doc", "rDoc.IsMobile"),
       _g7v("lastextdoor", "GetFormIDString (GetPCLastExteriorDoor)", True),
       _g7v("leveledbase.doc", "GetFormIDString (rDoc.GetLeveledActorBase)", True),
       _g7v("lightamount.player", "Floor (rPlayer.GetActorLightAmount)"),
       _g7("resetfalltime", "rPlayer.ResetFallTime\nSetFunctionValue (rPlayer.GetFallTimeElapsed)")]
    # Explosion flags and forms, put back.
    + [_g7w(f"explflag.gren40.{i}", FX["gren40"], f"GetExplosionFlag rObj {i}") for i in range(1, 7)]
    + [_g7w("explflag.set", FX["gren40"], "SetExplosionFlag rObj 4 (1 - (GetExplosionFlag rObj 4))"),
       _g7w("explflag.set.read", FX["gren40"], "GetExplosionFlag rObj 4"),
       _g7w("explflag.restore", FX["gren40"], "SetExplosionFlag rObj 4 (1 - (GetExplosionFlag rObj 4))")]
    + [_g7w(f"expltrait.gren40.{i}", FX["gren40"], f"GetFormIDString (GetExplosionTraitForm rObj {i})", True) for i in range(5)]
    + [_g7("expltrait.set", _form("rObj", FX["gren40"]) + _form("rOther", NF["stimpak"])
           + 'let rDoc := GetExplosionTraitForm rObj 4\nrPlayer.AuxVarSetRef "*nvfXT" rDoc\nSetExplosionTraitForm rObj 4 rOther\n'
           "SetFunctionValue (GetFormIDString (GetExplosionTraitForm rObj 4))", True),
       _g7("expltrait.restore", _form("rObj", FX["gren40"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfXT"\n'
           "if rOther\nSetExplosionTraitForm rObj 4 rOther\nelse\nSetExplosionTraitForm rObj 4\nendif\n"
           'rPlayer.AuxVarErase "*nvfXT"\n'
           "SetFunctionValue (GetFormIDString (GetExplosionTraitForm rObj 4))", True)]
    # Perk level and flags, put back.
    + [_g7w("perklevel.bloodymess", FX["bloodymess"], "GetPerkLevel rObj")]
    + [_g7w(f"perkflag.bloodymess.{i}", FX["bloodymess"], f"GetPerkFlag rObj {i}") for i in range(3)]
    + [_g7("perklevel.set", _form("rObj", FX["bloodymess"]) + 'let fRes := GetPerkLevel rObj\nrPlayer.AuxVarSetFlt "*nvfPL" fRes\n'
           "SetPerkLevel rObj 8\nSetFunctionValue (GetPerkLevel rObj)"),
       _g7("perklevel.restore", _form("rObj", FX["bloodymess"]) + 'SetPerkLevel rObj (rPlayer.AuxVarGetFlt "*nvfPL")\n'
           'rPlayer.AuxVarErase "*nvfPL"\nSetFunctionValue (GetPerkLevel rObj)'),
       _g7w("perkflag.set", FX["bloodymess"], "SetPerkFlag rObj 2 1"),
       _g7w("perkflag.set.read", FX["bloodymess"], "GetPerkFlag rObj 2"),
       _g7w("perkflag.restore", FX["bloodymess"], "SetPerkFlag rObj 2 0"),
       _g7v("pcmovedcell", "GetPCMovedCell")]
)

# Fifty-first set: the next JIP setters of the counts that have a getter to read them back
# (worldspace climate and flags, climate textures, armour audio templates and model paths,
# destruction health, container respawns, package flags, interaction), and a few getters.
# WastelandNV (WRLD) 0DA726 from fnvtool esm inventory [observed]. Everything is put back
# except the leather armour's world model path and the climate's sun glare texture, which
# are session only (a string_var cannot be passed back, gaps1). SetDestructionDataHealth,
# SetContainerRespawns and SetPackageFlag returned 0 here, not the new or old value;
# SetArmorAudioTemplate with no form left the old template in place [observed].
FX.update(wasteland="DA726")


def _g8(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g8v(pid, expr, text=False):
    return _g8(pid, f"SetFunctionValue ({expr})", text)


def _g8w(pid, key, expr, text=False):
    return _g8(pid, _form("rObj", key) + f"SetFunctionValue ({expr})", text)


def _restore_ref(setter, aux):
    """Put back a form saved in aux variable `aux` with `setter rObj <form>`, or none."""
    return (f'let rOther := rPlayer.AuxVarGetRef "{aux}"\nif rOther\n{setter} rOther\nelse\n{setter}\nendif\n'
            f'rPlayer.AuxVarErase "{aux}"\n')


GAPS8_PROBES = (
    # Worldspace climate, set and put back.
    [_g8w("wrldclimate.wasteland", FX["wasteland"], "GetFormIDString (GetWorldspaceClimate rObj)", True),
     _g8("wrldclimate.set", _form("rObj", FX["wasteland"]) + _form("rOther", FX["stripclimate"])
         + 'let rDoc := GetWorldspaceClimate rObj\nrPlayer.AuxVarSetRef "*nvfWC" rDoc\nSetWorldspaceClimate rObj rOther\n'
         "SetFunctionValue (GetFormIDString (GetWorldspaceClimate rObj))", True),
     _g8("wrldclimate.restore", _form("rObj", FX["wasteland"]) + _restore_ref("SetWorldspaceClimate rObj", "*nvfWC")
         + "SetFunctionValue (GetFormIDString (GetWorldspaceClimate rObj))", True),
     _g8v("refreshclimate", "RefreshCurrentClimate"),
     _g8v("climate.current.after", "GetFormIDString (GetCurrentClimate)", True)]
    # Worldspace flags.
    + [_g8w(f"wrldflag.wasteland.{i}", FX["wasteland"], f"GetWorldspaceFlag rObj {i}") for i in range(8)]
    + [_g8w("wrldflag.set", FX["wasteland"], "SetWorldspaceFlag rObj 1 (1 - (GetWorldspaceFlag rObj 1))"),
       _g8w("wrldflag.set.read", FX["wasteland"], "GetWorldspaceFlag rObj 1"),
       _g8w("wrldflag.restore", FX["wasteland"], "SetWorldspaceFlag rObj 1 (1 - (GetWorldspaceFlag rObj 1))"),
       _g8w("wrldflag.restore.read", FX["wasteland"], "GetWorldspaceFlag rObj 1")]
    # Climate textures.
    + [_g8w("climate.sunglare", FX["stripclimate"], "GetClimateSunGlareTexture rObj", True),
       _g8w("climate.nightsky", FX["stripclimate"], "GetClimateNightSkyModel rObj", True),
       _g8("climate.sunglare.set", _form("rObj", FX["stripclimate"])
           + 'SetClimateSunGlareTexture rObj "NVF\\Glare.dds"\nSetFunctionValue (GetClimateSunGlareTexture rObj)', True)]
    # Armour: audio templates and model paths.
    + [_g8w(f"armoraudio.{k}", fid, "GetFormIDString (GetArmorAudioTemplate rObj)", True)
       for k, fid in (("leather", NF["leather"]), ("hazmat", FX["hazmat"]))]
    + [_g8("armoraudio.set", _form("rObj", NF["leather"]) + _form("rOther", FX["hazmat"])
           + 'let rDoc := GetArmorAudioTemplate rObj\nrPlayer.AuxVarSetRef "*nvfAT" rDoc\n'
           "let rOther := GetArmorAudioTemplate rOther\nSetArmorAudioTemplate rObj rOther\n"
           "SetFunctionValue (GetFormIDString (GetArmorAudioTemplate rObj))", True),
       _g8("armoraudio.restore", _form("rObj", NF["leather"]) + _restore_ref("SetArmorAudioTemplate rObj", "*nvfAT")
           + "SetFunctionValue (GetFormIDString (GetArmorAudioTemplate rObj))", True)]
    + [_g8w(f"bipedpath.leather.{i}", NF["leather"], f"GetBipedModelPath {i} rObj", True) for i in range(4)]
    + [_g8("bipedpath.set", _form("rObj", NF["leather"])
           + 'SetBipedModelPathAlt rObj 2 "NVF\\Ground%g.nif" 7\nSetFunctionValue (GetBipedModelPath 2 rObj)', True)]
    # Destruction health, container respawns, package flags.
    + [_g8("destruction.set", _form("rObj", FX["ebox"]) + 'let fRes := GetDestructionDataHealth rObj\nrPlayer.AuxVarSetFlt "*nvfDH" fRes\n'
           "SetFunctionValue (SetDestructionDataHealth rObj 75)"),
       _g8w("destruction.set.read", FX["ebox"], "GetDestructionDataHealth rObj"),
       _g8("destruction.restore", _form("rObj", FX["ebox"]) + 'SetDestructionDataHealth rObj (rPlayer.AuxVarGetFlt "*nvfDH")\n'
           'rPlayer.AuxVarErase "*nvfDH"\nSetFunctionValue (GetDestructionDataHealth rObj)'),
       _g8w("respawns.set", FX["nuka"], "SetContainerRespawns 1 rObj"),
       _g8w("respawns.set.read", FX["nuka"], "GetContainerRespawns rObj"),
       _g8w("respawns.restore", FX["nuka"], "SetContainerRespawns 0 rObj"),
       _g8w("respawns.restore.read", FX["nuka"], "GetContainerRespawns rObj"),
       _g8w("packageflag.set", FX["docsandbox"], "SetPackageFlag rObj 10 1"),
       _g8w("packageflag.set.read", FX["docsandbox"], "GetPackageFlag rObj 10"),
       _g8w("packageflag.restore", FX["docsandbox"], "SetPackageFlag rObj 10 0"),
       _g8w("packageflag.restore.read", FX["docsandbox"], "GetPackageFlag rObj 10")]
    # Interaction on a reference, put back.
    + [_g8v("interaction.doc", "rDoc.GetInteractionDisabled"),
       _g8("interaction.set", "rDoc.SetInteractionDisabled 1\nSetFunctionValue (rDoc.GetInteractionDisabled)"),
       _g8("interaction.restore", "rDoc.SetInteractionDisabled 0\nSetFunctionValue (rDoc.GetInteractionDisabled)"),
       _g8w("interaction.base", NF["doc"], "GetInteractionDisabled rObj")]
    # Getters.
    + [_g8v("mine.doc", "rDoc.GetMineArmed"),
       _g8v("modtype.player.2", "rPlayer.EquippedWeaponHasModType 2"),
       _g8v("ammorounds.player", "rPlayer.GetCurrentAmmoRounds"),
       _g8v("reticlerange.int", "Floor (GetReticleRange)")]
)

# Fifty-second set: melee and unarmed DPS, as the extender core asked after gaps7 (the
# knife's rate and crit part did not fit the record). For WeapKnifeCombat 004326,
# WeapNVMachete 0CE569, WeapSledgehammer 004351, Fists 0001F4 and WeapBrassKnuckles
# 004324 [observed, fnvtool esm inventory]: the record fields the formula may use, then
# GetCalculatedWeaponDamage and GetCalculatedWeaponDPS at the save's skills, with crit
# damage 0, with CritChance +10, and with the weapon's skill +16. Everything is put back.
# GetWeaponSpeed did not compile here [observed]; GetWeaponAnimMult is read instead.
_G9_WEAPONS = {"knife": "4326", "machete": "CE569", "sledge": "4351", "fists": "1F4", "knuckles": "4324"}
_G9_FIELDS = (("critdamage", "GetWeaponCritDamage rObj"), ("critmult", "Floor ((GetWeaponCritChance rObj) * 1000)"),
              ("attackdamage", "GetAttackDamage rObj"),
              ("reach", "Floor ((GetWeaponReach rObj) * 1000)"), ("animmult", "Floor ((GetWeaponAnimMult rObj) * 1000)"),
              ("animattackmult", "Floor ((GetWeaponAnimAttackMult rObj) * 1000)"),
              ("attackanim", "GetWeaponAttackAnimation rObj"), ("skill", "GetWeaponSkill rObj"), ("type", "GetWeaponType rObj"))


def _g9(pid, body, text=False):
    return (pid, "ref rPlayer\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"]) + body, text, None, "")


def _g9v(pid, expr, text=False):
    return _g9(pid, f"SetFunctionValue ({expr})", text)


_DPS9 = "SetFunctionValue (Floor ((rPlayer.GetCalculatedWeaponDPS rObj) * 100))"
_DMG9 = "SetFunctionValue (Floor ((GetCalculatedWeaponDamage rObj) * 100))"


def _g9w(w):
    fid = _G9_WEAPONS[w]
    return (
        [_g9(f"{w}.{k}", _form("rObj", fid) + f"SetFunctionValue ({e})") for k, e in _G9_FIELDS]
        + [_g9(f"{w}.damage", _form("rObj", fid) + _DMG9),
           _g9(f"{w}.dps", _form("rObj", fid) + _DPS9),
           _g9(f"{w}.dmg0", _form("rObj", fid) + 'let fRes := GetWeaponCritDamage rObj\nrPlayer.AuxVarSetFlt "*nvfCD" fRes\n'
               "SetWeaponCritDamage 0 rObj\n" + _DPS9),
           _g9(f"{w}.dmg0.restore", _form("rObj", fid) + 'SetWeaponCritDamage (rPlayer.AuxVarGetFlt "*nvfCD") rObj\n'
               'rPlayer.AuxVarErase "*nvfCD"\nSetFunctionValue (GetWeaponCritDamage rObj)')]
    )


_SKILLS = (("MeleeWeapons", ("knife", "machete", "sledge")), ("Unarmed", ("fists", "knuckles")))
GAPS9_PROBES = (
    [_g9v(f"melee.av.{av.lower()}", f'Floor ((rPlayer.GetActorValue "{av}") * 100)')
     for av in ("MeleeWeapons", "Unarmed", "Strength", "Agility", "MeleeDamage", "UnarmedDamage", "CritChance")]
    + [r for w in _G9_WEAPONS for r in _g9w(w)]
    # CritChance +10.
    + [_g9("cc.mod10", 'rPlayer.ModActorValue "CritChance" 10\nSetFunctionValue (Floor ((rPlayer.GetActorValue "CritChance") * 100))')]
    + [_g9(f"{w}.cc20", _form("rObj", fid) + _DPS9) for w, fid in _G9_WEAPONS.items()]
    + [_g9("cc.restore", 'rPlayer.ModActorValue "CritChance" -10\nSetFunctionValue (Floor ((rPlayer.GetActorValue "CritChance") * 100))')]
    # Each weapon's skill +16: damage and DPS, then crit damage 0 again.
    + [r for av, ws in _SKILLS for r in (
        [_g9(f"skill.{av.lower()}.mod16", f'rPlayer.ModActorValue "{av}" 16\nSetFunctionValue (rPlayer.GetActorValue "{av}")')]
        + [r2 for w in ws for r2 in (
            _g9(f"{w}.s16.damage", _form("rObj", _G9_WEAPONS[w]) + _DMG9),
            _g9(f"{w}.s16.dps", _form("rObj", _G9_WEAPONS[w]) + _DPS9),
            _g9(f"{w}.s16.dmg0", _form("rObj", _G9_WEAPONS[w]) + 'let fRes := GetWeaponCritDamage rObj\nrPlayer.AuxVarSetFlt "*nvfCD" fRes\n'
                "SetWeaponCritDamage 0 rObj\n" + _DPS9),
            _g9(f"{w}.s16.dmg0.restore", _form("rObj", _G9_WEAPONS[w]) + 'SetWeaponCritDamage (rPlayer.AuxVarGetFlt "*nvfCD") rObj\n'
                'rPlayer.AuxVarErase "*nvfCD"\nSetFunctionValue (GetWeaponCritDamage rObj)'))]
        + [_g9(f"skill.{av.lower()}.restore", f'rPlayer.ModActorValue "{av}" -16\nSetFunctionValue (rPlayer.GetActorValue "{av}")')])]
)

# Fifty-third set: the next JIP functions of the counts with something to read back: ref
# map strings, RunScriptSnippet, arrays written to and read from a file (nvfprobe_arr.txt
# in the game folder, removed after the run), impact data sets and models (the 40mm
# grenade explosion's impact data set 093BA4, gaps7), shell casings, worldspace image
# spaces, ray casts, SetPosEx, tilt angles, effect bases, sneaking and first person.
# Everything is put back except the dirt impact model and shell casing paths (session only).
# IsThirdPerson is ppNVSE (not installed); IsPC1stPerson is read instead.


def _g10(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\nfloat fAx\nfloat fAy\nfloat fAz\narray_var aA\n"
            "Begin Function { }\n" + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g10v(pid, expr, text=False):
    return _g10(pid, f"SetFunctionValue ({expr})", text)


def _g10w(pid, key, expr, text=False):
    return _g10(pid, _form("rObj", key) + f"SetFunctionValue ({expr})", text)


def _wait10(pid):
    return _g10(pid, "SetFunctionValue 1")


FX.update(ipds40="93BA4")
_POS = "let fAx := rPlayer.GetPos X\nlet fAy := rPlayer.GetPos Y\nlet fAz := rPlayer.GetPos Z\n"
GAPS10_PROBES = (
    # Ref map strings.
    [_g10("refmap.setstring", 'rDoc.RefMapArraySetString "*nvfRS" "NVF str"\nSetFunctionValue (rDoc.RefMapArrayGetString "*nvfRS")', True),
     _g10("refmap.getstring.none", 'SetFunctionValue (rPlayer.RefMapArrayGetString "*nvfRS")', True),
     _g10("refmap.string.erase", 'rDoc.RefMapArrayErase "*nvfRS"\nSetFunctionValue (rDoc.RefMapArrayGetString "*nvfRS")', True),
     # RunScriptSnippet on the player, put back.
     _g10("snippet.luck", 'RunScriptSnippet 0 "ModAV Luck 1"\nSetFunctionValue (rPlayer.GetActorValue Luck)'),
     _g10("snippet.luck.restore", 'RunScriptSnippet 0 "ModAV Luck -1"\nSetFunctionValue (rPlayer.GetActorValue Luck)'),
     # Arrays to and from a file.
     _g10("arrayfile.write", 'let aA := ar_List 4 5 6\nSetFunctionValue (WriteArrayToFile "nvfprobe_arr.txt" 0 aA)'),
     _g10("arrayfile.read.size", 'let aA := ReadArrayFromFile "nvfprobe_arr.txt"\nSetFunctionValue (ar_Size aA)'),
     _g10("arrayfile.read.1", 'let aA := ReadArrayFromFile "nvfprobe_arr.txt"\nlet fRes := aA[1]\nSetFunctionValue fRes'),
     _g10("arrayfile.append", 'let aA := ar_List 7 8 9\nlet fRes := WriteArrayToFile "nvfprobe_arr.txt" 1 aA\n'
          'let aA := ReadArrayFromFile "nvfprobe_arr.txt"\nSetFunctionValue (ar_Size aA)'),
     _g10("arrayfile.missing", 'let aA := ReadArrayFromFile "nvfprobe_none.txt"\nSetFunctionValue (ar_Size aA)')]
    # Impact data sets and models.
    + [_g10w(f"ipds.gren40.{i}", FX["ipds40"], f"GetFormIDString (GetImpactDataSetForm rObj {i})", True) for i in range(12)]
    + [_g10("ipct.model.0", _form("rObj", FX["ipds40"]) + "let rOther := GetImpactDataSetForm rObj 0\n"
            "SetFunctionValue (GetImpactDataModel rOther)", True),
       _g10("ipds.set", _form("rObj", FX["ipds40"]) + 'let rOther := GetImpactDataSetForm rObj 0\nrPlayer.AuxVarSetRef "*nvfID" rOther\n'
            "let rOther := GetImpactDataSetForm rObj 4\nSetImpactDataSetForm rObj 0 rOther\n"
            "SetFunctionValue (GetFormIDString (GetImpactDataSetForm rObj 0))", True),
       _g10("ipds.restore", _form("rObj", FX["ipds40"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfID"\nSetImpactDataSetForm rObj 0 rOther\n'
            'rPlayer.AuxVarErase "*nvfID"\nSetFunctionValue (GetFormIDString (GetImpactDataSetForm rObj 0))', True),
       _g10("ipct.model.set", _form("rObj", FX["ipds40"]) + "let rOther := GetImpactDataSetForm rObj 0\n"
            'SetImpactDataModel rOther "NVF\\Impact.nif"\nSetFunctionValue (GetImpactDataModel rOther)', True),
       _g10w("shell.pistol", NF["pistol10"], "GetWeaponShellCasingModel rObj", True),
       _g10("shell.set", _form("rObj", NF["pistol10"]) + 'SetWeaponShellCasingModel rObj "NVF\\Shell.nif"\n'
            "SetFunctionValue (GetWeaponShellCasingModel rObj)", True),
       _g10w("wrldimagespace.wasteland", FX["wasteland"], "GetFormIDString (GetWorldspaceImagespace rObj)", True)]
    # Ray cast straight up from above the player, and from far away.
    + [_g10("p2p.up", _POS + "SetFunctionValue (Floor (GetP2PRayCastRange fAx fAy (fAz + 100) fAx fAy (fAz + 20000) 0))"),
       _g10("p2p.down", _POS + "SetFunctionValue (Floor (GetP2PRayCastRange fAx fAy (fAz + 100) fAx fAy (fAz - 1000) 0))")]
    # SetPosEx on Doc, translated and put back.
    + [_g10("posex.doc", 'let fAx := rDoc.GetPos X\nrPlayer.AuxVarSetFlt "*nvfPX" fAx\nrDoc.SetPosEx 10 0 0 1\n'
            'SetFunctionValue (Floor ((rDoc.GetPos X) - fAx + 0.5))'),
       _g10("posex.doc.restore", 'rDoc.SetPosEx -10 0 0 1\nlet fAx := rPlayer.AuxVarGetFlt "*nvfPX"\nrPlayer.AuxVarErase "*nvfPX"\n'
            'SetFunctionValue (Floor ((rDoc.GetPos X) - fAx + 0.5))')]
    # Tilt angle on Doc, put back.
    + [_g10v("tilt.doc.x", "Floor ((rDoc.GetActorTiltAngle X) * 100)"),
       _g10("tilt.doc.set", "rDoc.SetActorTiltAngle X 20\nSetFunctionValue (Floor ((rDoc.GetActorTiltAngle X) * 100))"),
       _g10("tilt.doc.restore", "rDoc.SetActorTiltAngle X 0\nSetFunctionValue (Floor ((rDoc.GetActorTiltAngle X) * 100))")]
    # Effect base of the stimpak's first effect, put back.
    + [_g10w("effbase.stimpak.0", NF["stimpak"], "GetFormIDString (GetNthEffectBase rObj 0)", True),
       _g10("effbase.set", _form("rObj", NF["stimpak"]) + _form("rOther", FX["silversting"])
            + 'let rDoc := GetNthEffectBase rObj 0\nrPlayer.AuxVarSetRef "*nvfEB" rDoc\nlet rOther := GetNthEffectBase rOther 0\n'
            "let rOther := SetNthEffectBase rObj 0 rOther\nSetFunctionValue (GetFormIDString rOther)", True),
       _g10w("effbase.set.read", NF["stimpak"], "GetFormIDString (GetNthEffectBase rObj 0)", True),
       _g10("effbase.restore", _form("rObj", NF["stimpak"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfEB"\nSetNthEffectBase rObj 0 rOther\n'
            'rPlayer.AuxVarErase "*nvfEB"\nSetFunctionValue (GetFormIDString (GetNthEffectBase rObj 0))', True)]
    # Sneaking and first person, put back.
    + [_g10("sneak.toggle.on", "TogglePlayerSneaking 1\nSetFunctionValue 1"), _wait10("sneak.wait0"), _wait10("sneak.wait1"),
       _g10v("sneak.on", "rPlayer.IsSneaking"),
       _g10("sneak.toggle.off", "TogglePlayerSneaking 0\nSetFunctionValue 1"), _wait10("sneak.wait2"), _wait10("sneak.wait3"),
       _g10v("sneak.off", "rPlayer.IsSneaking"),
       _g10v("view.first", "IsPC1stPerson"),
       _g10("view.toggle", 'let fRes := IsPC1stPerson\nrPlayer.AuxVarSetFlt "*nvfTP" fRes\nToggleFirstPerson (1 - fRes)\nSetFunctionValue 1'),
       _wait10("view.wait0"), _wait10("view.wait1"),
       _g10v("view.first.after", "IsPC1stPerson"),
       _g10("view.restore", 'let fRes := rPlayer.AuxVarGetFlt "*nvfTP"\nToggleFirstPerson fRes\nrPlayer.AuxVarErase "*nvfTP"\nSetFunctionValue 1'),
       _wait10("view.wait2"), _wait10("view.wait3"),
       _g10v("view.first.restored", "IsPC1stPerson")]
)

# Fifty-fourth set: the frame SetPosEx translates in, as the extender core asked after
# gaps10 (Doc's heading, then 0 10 0 and 10 0 0 with both deltas; the player turned to
# heading 90 the same way), and the JIP functions that work on an inventory reference,
# read on the 10mm pistol equipped by the player (GetEquippedItemRef 5): current health,
# mod flags, current ammo rounds. Positions, heading, health, flags and the inventory are
# put back (the ammo to the count read first). SetAngle reads back a frame later.


def _g11(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\nfloat fAx\nfloat fAy\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _g11v(pid, expr, text=False):
    return _g11(pid, f"SetFunctionValue ({expr})", text)


def _wait11(pid):
    return _g11(pid, "SetFunctionValue 1")


def _move(pid, who, aux, dx, dy):
    """Translate `who` by (dx, dy, 0) in SetPosEx's translate mode and give the world X and Y deltas."""
    return _g11(pid, f'let fAx := {who}.GetPos X\nlet fAy := {who}.GetPos Y\n{who}.SetPosEx {dx} {dy} 0 1\n'
                     f'let fAx := Floor (({who}.GetPos X) - fAx + 0.5)\nlet fAy := Floor (({who}.GetPos Y) - fAy + 0.5)\n'
                     'SetFunctionValue (sv_Construct "%g %g" fAx fAy)', True)


_REF = "let rObj := rPlayer.GetEquippedItemRef 5\n"
GAPS11_PROBES = (
    # Doc's heading and translations, each put back.
    [_g11v("posex.doc.anglez", "Floor ((rDoc.GetAngle Z) * 100)"),
     _move("posex.doc.y10", "rDoc", "", 0, 10), _move("posex.doc.y10.back", "rDoc", "", 0, -10),
     _move("posex.doc.x10", "rDoc", "", 10, 0), _move("posex.doc.x10.back", "rDoc", "", -10, 0),
     _move("posex.doc.xy", "rDoc", "", 10, 10), _move("posex.doc.xy.back", "rDoc", "", -10, -10),
     # The player at heading 90, then the heading put back.
     _g11("posex.player.anglez", 'let fRes := rPlayer.GetAngle Z\nrPlayer.AuxVarSetFlt "*nvfAZ" fRes\nSetFunctionValue (Floor (fRes * 100))'),
     _g11("posex.player.turn", "rPlayer.SetAngle Z 90\nSetFunctionValue (Floor ((rPlayer.GetAngle Z) * 100))"),
     _wait11("posex.player.wait0"),
     _move("posex.player.y10", "rPlayer", "", 0, 10), _move("posex.player.y10.back", "rPlayer", "", 0, -10),
     _move("posex.player.x10", "rPlayer", "", 10, 0), _move("posex.player.x10.back", "rPlayer", "", -10, 0),
     _g11("posex.player.turn0", "rPlayer.SetAngle Z 0\nSetFunctionValue (Floor ((rPlayer.GetAngle Z) * 100))"),
     _wait11("posex.player.wait1"),
     _move("posex.player.h0.y10", "rPlayer", "", 0, 10), _move("posex.player.h0.y10.back", "rPlayer", "", 0, -10),
     _g11("posex.player.restore", 'rPlayer.SetAngle Z (rPlayer.AuxVarGetFlt "*nvfAZ")\nrPlayer.AuxVarErase "*nvfAZ"\n'
          "SetFunctionValue (Floor ((rPlayer.GetAngle Z) * 100))")]
    # The pistol, added and equipped with ammo.
    + [_g11("ref.add", _form("rObj", NF["pistol10"]) + _form("rOther", NF["ammo10"])
            + 'let fRes := rPlayer.GetItemCount rOther\nrPlayer.AuxVarSetFlt "*nvfAM" fRes\n'
            "rPlayer.AddItem rObj 1 1\nrPlayer.AddItem rOther 24 1\nrPlayer.EquipItem rObj 0 1\n"
            'SetFunctionValue (sv_Construct "%g %g" (rPlayer.GetItemCount rObj) fRes)', True),
       _wait11("ref.wait0"), _wait11("ref.wait1"), _wait11("ref.wait2"),
       _g11v("ref.equipped", "GetFormIDString (rPlayer.GetEquippedObject 5)", True),
       _g11(f"ref.base", _REF + "SetFunctionValue (GetFormIDString (rObj.GetBaseObject))", True)]
    + [_g11(f"ref.health.{m}", _REF + f"SetFunctionValue (Floor ((rObj.GetItemRefCurrentHealth {m}) * 100))") for m in range(3)]
    + [_g11("ref.health.set", _REF + 'let fRes := rObj.GetItemRefCurrentHealth\nrPlayer.AuxVarSetFlt "*nvfIH" fRes\n'
            "SetFunctionValue (rObj.SetItemRefCurrentHealth 50)"),
       _g11("ref.health.set.read", _REF + "SetFunctionValue (Floor ((rObj.GetItemRefCurrentHealth) * 100))"),
       _g11("ref.health.setpct", _REF + "let fRes := rObj.SetItemRefCurrentHealth 0.5 1\n"
            "SetFunctionValue (Floor ((rObj.GetItemRefCurrentHealth) * 100))"),
       _g11("ref.health.restore", _REF + 'rObj.SetItemRefCurrentHealth (rPlayer.AuxVarGetFlt "*nvfIH")\nrPlayer.AuxVarErase "*nvfIH"\n'
            "SetFunctionValue (Floor ((rObj.GetItemRefCurrentHealth) * 100))"),
       _g11("ref.modflags", _REF + "SetFunctionValue (rObj.GetWeaponRefModFlags)"),
       _g11("ref.modflags.set", _REF + "SetFunctionValue (rObj.SetWeaponRefModFlags 1)"),
       _g11("ref.modflags.set.read", _REF + "SetFunctionValue (rObj.GetWeaponRefModFlags)"),
       _g11v("ref.modtype.after", "rPlayer.EquippedWeaponHasModType 1"),
       _g11("ref.modflags.restore", _REF + "let fRes := rObj.SetWeaponRefModFlags 0\nSetFunctionValue (rObj.GetWeaponRefModFlags)"),
       _g11("ref.ammo", _REF + "SetFunctionValue (rObj.GetCurrentAmmoRounds)"),
       _g11v("ref.ammo.player", "rPlayer.GetCurrentAmmoRounds"),
       _g11("ref.ammo.set", _REF + "rObj.SetCurrentAmmoRounds 3\nSetFunctionValue (rObj.GetCurrentAmmoRounds)"),
       _g11v("ref.ammo.set.player", "rPlayer.GetCurrentAmmoRounds"),
       _g11("ammo.actor.set", 'let fRes := rPlayer.GetCurrentAmmoRounds\nrPlayer.AuxVarSetFlt "*nvfCR" fRes\n'
            "rPlayer.SetCurrentAmmoRounds 3\nSetFunctionValue (rPlayer.GetCurrentAmmoRounds)"),
       _g11("ammo.actor.restore", 'rPlayer.SetCurrentAmmoRounds (rPlayer.AuxVarGetFlt "*nvfCR")\nrPlayer.AuxVarErase "*nvfCR"\n'
            "SetFunctionValue (rPlayer.GetCurrentAmmoRounds)"),
       # Put back: unequip and remove the pistol and the ammo left.
       _g11("ref.remove", _form("rObj", NF["pistol10"]) + _form("rOther", NF["ammo10"])
            + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\n"
            'rPlayer.RemoveItem rOther ((rPlayer.GetItemCount rOther) - (rPlayer.AuxVarGetFlt "*nvfAM")) 1\n'
            'rPlayer.AuxVarErase "*nvfAM"\nSetFunctionValue 1'),
       _wait11("ref.wait3"), _wait11("ref.wait4"),
       _g11("ref.after", _form("rObj", NF["pistol10"]) + _form("rOther", NF["ammo10"])
            + 'SetFunctionValue (sv_Construct "%g %g" (rPlayer.GetItemCount rObj) (rPlayer.GetItemCount rOther))', True)]
)

# Fifty-fifth set: after JIP, the most used unprobed functions of the other plugins in
# docs/handoff/modlist-function-counts.json (LU, as built into JIP LN; JohnnyGuitarNVSE;
# ShowOffNVSE) that read something or set something readable without menus. Forms from
# fnvtool esm inventory [observed]: CaucasianOld (RACE) 0042C3, VCG01 (QUST) 104C1C,
# VMS01 (QUST) 080664. Always run, the perk entry value and Health are put back; the
# idle's animation path is session only (a string_var cannot be passed back, gaps1).
# Toughness's entry 0 has no second value: GetNthPerkEntryValue2 read -1 and the set was
# ignored; ApplyEasing "linear" is not an easing name (-1) [observed].
FX.update(vcg01="104C1C", vms01="80664")


def _p1(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\narray_var aA\narray_var aB\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _p1v(pid, expr, text=False):
    return _p1(pid, f"SetFunctionValue ({expr})", text)


def _p1w(pid, key, expr, text=False):
    return _p1(pid, _form("rObj", key) + f"SetFunctionValue ({expr})", text)


PLUGINS1_PROBES = (
    # LU: idles, perk entries, recipes, items, flags.
    [_p1w("lu.idlepath", FX["idlehouse"], "GetIdleAnimPath rObj", True),
     _p1("lu.idlepath.set", _form("rObj", FX["idlehouse"]) + 'SetIdleAnimPath rObj "NVF\\Idle.kf"\nSetFunctionValue (GetIdleAnimPath rObj)', True),
     _p1w("lu.perkvalue2.toughness", FX["toughness"], "Floor ((GetNthPerkEntryValue2 rObj 0) * 100)"),
     _p1("lu.perkvalue2.set", _form("rObj", FX["toughness"]) + 'let fRes := GetNthPerkEntryValue2 rObj 0\nrPlayer.AuxVarSetFlt "*nvfP2" fRes\n'
         "SetNthPerkEntryValue2 rObj 0 7\nSetFunctionValue (Floor ((GetNthPerkEntryValue2 rObj 0) * 100))"),
     _p1("lu.perkvalue2.restore", _form("rObj", FX["toughness"]) + 'SetNthPerkEntryValue2 rObj 0 (rPlayer.AuxVarGetFlt "*nvfP2")\n'
         'rPlayer.AuxVarErase "*nvfP2"\nSetFunctionValue (Floor ((GetNthPerkEntryValue2 rObj 0) * 100))'),
     _p1("lu.recipe.inputs", _form("rObj", FX["recipe127"]) + "let aA := GetRecipeInputForms rObj\nSetFunctionValue (ar_Size aA)"),
     _p1("lu.recipe.input0", _form("rObj", FX["recipe127"]) + "let aA := GetRecipeInputForms rObj\nlet rOther := aA[0]\n"
         "SetFunctionValue (GetFormIDString rOther)", True),
     _p1w("lu.recipe.category", FX["recipe127"], "GetFormIDString (GetRecipeCategory rObj)", True),
     _p1w("lu.recipe.subcategory", FX["recipe127"], "GetFormIDString (GetRecipeSubcategory rObj)", True),
     _p1("lu.basecount.doc", _form("rObj", NF["stimpak"]) + _form("rOther", NF["doc"]) + "SetFunctionValue (BaseGetItemCount rObj rOther)"),
     _p1("lu.basecount.docref", _form("rObj", NF["stimpak"]) + "SetFunctionValue (rDoc.BaseGetItemCount rObj)"),
     _p1v("lu.selfmodindex", "GetSelfModIndex"),
     _p1w("lu.hasperkrank.toughness", FX["toughness"], "rPlayer.HasPerkRank rObj"),
     _p1w("lu.hasperkrank.toughness.team", FX["toughness"], "rPlayer.HasPerkRank rObj 1")]
    + [_p1w(f"lu.ingestflag.stimpak.{i}", NF["stimpak"], f"GetIngestibleFlag rObj {i}") for i in range(4)]
    + [_p1w(f"lu.ingestflag.silversting.{i}", FX["silversting"], f"GetIngestibleFlag rObj {i}") for i in range(4)]
    + [_p1w(f"lu.templateflag.doc.{i}", NF["doc"], f"rDoc.GetActorTemplateFlag {i}") for i in range(3)]
    + [_p1w("lu.raceinlist.list127", NF["list127"], "rDoc.IsRaceInList rObj"),
       _p1v("lu.defaultmessagetime", "Floor ((GetDefaultMessageTime) * 100)"),
       _p1("lu.sortforms", _form("rObj", NF["pistol10"]) + _form("rOther", NF["stimpak"])
           + "let aA := ar_List rObj rOther rObj\nlet aB := ar_List 47\nlet aA := SortFormsByType aA aB\nSetFunctionValue (ar_Size aA)")]
    # JohnnyGuitar.
    + [_p1v(f"jg.avalt.player.{c}", f"Floor ((rPlayer.GetActorValueAlt {c}) * 100)") for c in (5, 16, 32, 41)]
    + [_p1w("jg.questfailed.vcg01", FX["vcg01"], "GetQuestFailed rObj"),
       _p1w("jg.questfailed.vms01", FX["vms01"], "GetQuestFailed rObj")]
    + [_p1w(f"jg.raceflag.caucold.{i}", FX["caucold"], f"GetRaceFlag rObj {i}") for i in range(3)]
    + [_p1v("jg.dllloaded.jg", 'IsDLLLoaded "johnnyguitar"'),
       _p1v("jg.dllloaded.jip", 'IsDLLLoaded "jip_nvse"'),
       _p1v("jg.dllloaded.none", 'IsDLLLoaded "nvfnosuchdll"'),
       _p1v("jg.thresholdedav.health", 'Floor ((rPlayer.GetThresholdedActorValue Health) * 100)'),
       _p1v("jg.thresholdedav.strength", 'Floor ((rPlayer.GetThresholdedActorValue Strength) * 100)'),
       _p1("jg.alwaysrun.set", 'let fRes := GetAlwaysRun\nrPlayer.AuxVarSetFlt "*nvfAR" fRes\n'
           "SetFunctionValue (sv_Construct \"%g %g\" (SetAlwaysRun (1 - fRes) 1) (GetAlwaysRun))", True),
       _p1("jg.alwaysrun.restore", 'let fRes := SetAlwaysRun (rPlayer.AuxVarGetFlt "*nvfAR") 1\nrPlayer.AuxVarErase "*nvfAR"\n'
           "SetFunctionValue (GetAlwaysRun)"),
       _p1v("jg.spread.player", "Floor ((rPlayer.GetCalculatedSpread) * 1000)"),
       _p1w("jg.overrideindex.stimpak", NF["stimpak"], "GetFormOverrideIndex rObj"),
       _p1w("lu.overridden.stimpak", NF["stimpak"], "IsFormOverridden rObj"),
       _p1v("jg.remap", "Floor ((Remap 5 0 10 100 200) * 100)"),
       _p1v("jg.remap.outside", "Floor ((Remap 15 0 10 100 200) * 100)"),
       _p1v("jg.vector3ddistance", "Floor ((GetVector3DDistance 0 0 0 3 4 12) * 100)"),
       _p1v("jg.hostilesnearby", "IsHostilesNearby"),
       _p1v("jg.compasshostile.doc", "rDoc.IsCompassHostile")]
    # ShowOff.
    + [_p1v(f"so.easing.{f}.{m}", f'Floor ((ApplyEasing 0.25 "{f}" {m}) * 10000)')
       for f in ("linear", "quad", "cubic", "sine") for m in (0, 1, 2)]
    + [_p1v("so.weapontype.player", "rPlayer.GetEquippedWeaponType"),
       _p1v("so.aiming.player", "rPlayer.IsAiming"),
       _p1v("so.blockingoraiming.player", "rPlayer.IsBlockingOrAiming"),
       _p1v("so.overencumbered", "GetIsPlayerOverencumbered"),
       _p1v("so.overencumbered.ignoregod", "GetIsPlayerOverencumbered 1"),
       _p1w("so.weaponranged.pistol", NF["pistol10"], "IsWeaponRanged rObj"),
       _p1w("so.weaponranged.knife", FX["knife"], "IsWeaponRanged rObj")]
)

# Fifty-sixth set: what the extender core asked after plugins1 (GetIngestibleFlag on food,
# drinks and a poison; ApplyEasing's other names; IsDLLLoaded on every DLL in the game's
# Data\NVSE\Plugins folder and the extender's own), then more LU, JohnnyGuitar and
# ShowOff functions: map markers, pickup sounds, recipe subcategories, activator sounds,
# form lists. Forms from fnvtool esm inventory [observed]: Cram 08C552, NukaCola 01519E,
# Beer 015197, NVPoisonGreaseScorpion 13D60F, the Goodsprings map marker 0CDF51,
# RepNVGoodsprings 104C22, RepNVPrimm 0F2406. Reputation, sounds, subcategory and form list
# are put back, except the stimpak's pickup and putdown sounds: setting them back to none
# left the new sound in place [observed], session only. The marker's name is session only.
FX.update(cram="8C552", nuka_cola="1519E", beer="15197", greasepoison="13D60F", gsmarker="CDF51", repprimm="F2406")


def _p2(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\narray_var aA\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _p2v(pid, expr, text=False):
    return _p2(pid, f"SetFunctionValue ({expr})", text)


def _p2w(pid, key, expr, text=False):
    return _p2(pid, _form("rObj", key) + f"SetFunctionValue ({expr})", text)


def _restore2(setter, aux):
    return (f'let rOther := rPlayer.AuxVarGetRef "{aux}"\nif rOther\n{setter} rOther\nelse\n{setter}\nendif\n'
            f'rPlayer.AuxVarErase "{aux}"\n')


PLUGINS2_PROBES = (
    [_p2w(f"lu.ingestflag.{k}.{i}", FX[k], f"GetIngestibleFlag rObj {i}")
     for k in ("cram", "nuka_cola", "beer", "greasepoison") for i in range(4)]
    + [_p2v(f"so.easing.{f}.{m}", f'Floor ((ApplyEasing 0.25 "{f}" {m}) * 10000)')
       for f in ("quart", "quint", "expo", "circ", "back", "elastic", "bounce") for m in (0, 1, 2)]
    + [_p2v(f"jg.dllloaded.{n}", f'IsDLLLoaded "{n}"')
       for n in ("ShowOffNVSE", "kNVSE", "MCM", "nvse_1_4", "nvse_steam_loader",
                 "SHOWOFFNVSE", "jip_nvse.dll")]
    # Map markers.
    + [_p2("lu.marker.rep", _form("rDoc", FX["gsmarker"]) + "SetFunctionValue (GetFormIDString (rDoc.GetMapMarkerRep))", True),
       _p2("lu.marker.rep.set", _form("rDoc", FX["gsmarker"]) + _form("rOther", FX["repprimm"])
           + 'let rObj := rDoc.GetMapMarkerRep\nrPlayer.AuxVarSetRef "*nvfMR" rObj\nrDoc.SetMapMarkerRep rOther\n'
           "SetFunctionValue (GetFormIDString (rDoc.GetMapMarkerRep))", True),
       _p2("lu.marker.rep.restore", _form("rDoc", FX["gsmarker"]) + _restore2("rDoc.SetMapMarkerRep", "*nvfMR")
           + "SetFunctionValue (GetFormIDString (rDoc.GetMapMarkerRep))", True),
       _p2("lu.marker.name", _form("rDoc", FX["gsmarker"]) + "SetFunctionValue (rDoc.GetName)", True),
       _p2("lu.marker.name.set", _form("rDoc", FX["gsmarker"]) + 'rDoc.SetMapMarkerName "NVF Springs"\nSetFunctionValue (rDoc.GetName)', True),
       _p2v("jg.custommarker", "GetFormIDString (GetCustomMapMarker)", True)]
    # Pickup and putdown sounds, put back.
    + [_p2w("lu.pickupsound.stimpak", NF["stimpak"], "GetFormIDString (GetPickupSound rObj)", True),
       _p2w("lu.putdownsound.stimpak", NF["stimpak"], "GetFormIDString (GetPutdownSound rObj)", True),
       _p2w("lu.pickupsound.pistol", NF["pistol10"], "GetFormIDString (GetPickupSound rObj)", True),
       _p2("lu.pickupsound.set", _form("rObj", NF["stimpak"]) + _form("rOther", FX["beep"])
           + 'let rDoc := GetPickupSound rObj\nrPlayer.AuxVarSetRef "*nvfPS" rDoc\nSetPickupSound rObj rOther\n'
           "SetFunctionValue (GetFormIDString (GetPickupSound rObj))", True),
       _p2("lu.pickupsound.restore", _form("rObj", NF["stimpak"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfPS"\nSetPickupSound rObj rOther\nrPlayer.AuxVarErase "*nvfPS"\n'
           + "SetFunctionValue (GetFormIDString (GetPickupSound rObj))", True),
       _p2("lu.putdownsound.set", _form("rObj", NF["stimpak"]) + _form("rOther", FX["beep"])
           + 'let rDoc := GetPutdownSound rObj\nrPlayer.AuxVarSetRef "*nvfPD" rDoc\nSetPutdownSound rObj rOther\n'
           "SetFunctionValue (GetFormIDString (GetPutdownSound rObj))", True),
       _p2("lu.putdownsound.restore", _form("rObj", NF["stimpak"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfPD"\nSetPutdownSound rObj rOther\nrPlayer.AuxVarErase "*nvfPD"\n'
           + "SetFunctionValue (GetFormIDString (GetPutdownSound rObj))", True)]
    # Recipe subcategory, put back; activator sound.
    + [_p2("lu.recipe.subcategory.set", _form("rObj", FX["recipe127"])
           + 'let rOther := GetRecipeSubcategory rObj\nrPlayer.AuxVarSetRef "*nvfRC" rOther\n'
           "let rOther := GetRecipeCategory rObj\nSetRecipeSubcategory rObj rOther\n"
           "SetFunctionValue (GetFormIDString (GetRecipeSubcategory rObj))", True),
       _p2("lu.recipe.subcategory.restore", _form("rObj", FX["recipe127"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfRC"\n'
           'SetRecipeSubcategory rObj rOther\nrPlayer.AuxVarErase "*nvfRC"\n'
           "SetFunctionValue (GetFormIDString (GetRecipeSubcategory rObj))", True),
       _p2w("lu.activatorsound.ebox", FX["ebox"], "GetFormIDString (GetActivatorSoundActivate rObj)", True),
       _p2w("lu.activatorwater.ebox", FX["ebox"], "GetFormIDString (GetActivatorWaterType rObj)", True)]
    # ShowOff RemoveFormFromFormList on list127: a stimpak added with AddFormToFormList at
    # the end, then removed; and a form not in the list. ListAddList and ListAddArray read 0
    # and -1 in a first run with an index past the end, and are left out.
    + [_p2w("so.list.count", NF["list127"], "ListGetCount rObj"),
       _p2("so.list.last", _form("rObj", NF["list127"]) + "let rOther := ListGetNthForm rObj ((ListGetCount rObj) - 1)\n"
           "SetFunctionValue (GetFormIDString rOther)", True),
       _p2("so.list.add", _form("rObj", NF["list127"]) + _form("rOther", NF["stimpak"])
           + "let fRes := AddFormToFormList rObj rOther\nSetFunctionValue (fRes * 1000 + (ListGetCount rObj))"),
       _p2("so.removeformfromlist", _form("rObj", NF["list127"]) + _form("rOther", NF["stimpak"])
           + "let fRes := RemoveFormFromFormList rObj rOther\nSetFunctionValue (fRes * 1000 + (ListGetCount rObj))"),
       _p2("so.removeformfromlist.absent", _form("rObj", NF["list127"]) + _form("rOther", NF["stimpak"])
           + "let fRes := RemoveFormFromFormList rObj rOther\nSetFunctionValue (fRes * 1000 + (ListGetCount rObj))"),
       _p2("so.list.last.after", _form("rObj", NF["list127"]) + "let rOther := ListGetNthForm rObj ((ListGetCount rObj) - 1)\n"
           "SetFunctionValue (GetFormIDString rOther)", True)]
)

# Fifty-seventh set: whether GetName on a reference ever reads the reference's own name
# (its FULL) rather than its base's, as the extender core asked after plugins2 (the
# Goodsprings marker 0CDF51 read "<no name>"): the marker's base, then every reference
# within two cells of the player (JIP GetRefs) whose GetName differs from its base's
# GetName. Then the last plugin functions with something to read: JohnnyGuitar's add-perk
# and remove-perk events (Toughness added to the player and removed), LU SetGlobalValue
# (TimeScale, put back from GetGlobalValue: SetGlobalValue returned 0 in a first run, not the
# previous value the wiki gives [observed]), JohnnyGuitar GetAvailableRadios.


def _p3(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nref rFirst\nfloat fRes\nfloat fAx\nfloat fAy\narray_var aA\n"
            "string_var sText\nstring_var sName\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _p3v(pid, expr, text=False):
    return _p3(pid, f"SetFunctionValue ({expr})", text)


def _wait3(pid):
    return _p3(pid, "SetFunctionValue 1")


# Walks the references near the player; fAy counts those named apart from their base,
# rFirst is the first of them.
_NAMEWALK = ("let aA := GetRefs 0 2\nlet fAx := 0\nlet fAy := 0\nwhile fAx < (ar_Size aA)\nlet rObj := aA[fAx]\n"
             "let rOther := rObj.GetBaseObject\nlet sText := rObj.GetName\nlet sName := GetName rOther\n"
             "if eval (sText != sName)\nlet fAy := fAy + 1\nif fAy == 1\nlet rFirst := rObj\nendif\nendif\n"
             "let fAx := fAx + 1\nloop\n")
PLUGINS3_PROBES = (
    [_p3("refname.marker.base", _form("rObj", FX["gsmarker"]) + "let rOther := rObj.GetBaseObject\n"
         "SetFunctionValue (GetFormIDString rOther)", True),
     _p3("refname.marker.basename", _form("rObj", FX["gsmarker"]) + "let rOther := rObj.GetBaseObject\n"
         "SetFunctionValue (GetName rOther)", True),
     _p3("refname.near.count", "let aA := GetRefs 0 2\nSetFunctionValue (ar_Size aA)"),
     _p3("refname.near.differ", _NAMEWALK + "SetFunctionValue fAy"),
     _p3("refname.near.first", _NAMEWALK + "if fAy\nSetFunctionValue (GetFormIDString rFirst)\nelse\nSetFunctionValue \"none\"\nendif", True),
     _p3("refname.near.first.name", _NAMEWALK + "if fAy\nSetFunctionValue (rFirst.GetName)\nelse\nSetFunctionValue \"none\"\nendif", True),
     _p3("refname.near.first.basename", _NAMEWALK + "if fAy\nlet rOther := rFirst.GetBaseObject\nSetFunctionValue (GetName rOther)\n"
         "else\nSetFunctionValue \"none\"\nendif", True)]
    # JohnnyGuitar perk events.
    + [_p3("jg.perkevents.set", _form("rObj", FX["toughness"])
           + 'SetJohnnyOnAddPerkEventHandler 1 (Begin Function {ref rPk, int iA, int iB}\n'
             '  rPlayer.AuxVarSetFlt "*nvfAP" (iA * 10 + iB)\n  rPlayer.AuxVarSetRef "*nvfAK" rPk\nEnd) 0 rObj\n'
           + 'SetJohnnyOnRemovePerkEventHandler 1 (Begin Function {ref rPk}\n  rPlayer.AuxVarSetRef "*nvfRK" rPk\nEnd) 0 rObj\n'
           + "SetFunctionValue 1"),
       _p3("jg.perkevents.add", _form("rObj", FX["toughness"]) + "rPlayer.AddPerk rObj\nSetFunctionValue (rPlayer.HasPerk rObj)"),
       _wait3("jg.perkevents.wait0"), _wait3("jg.perkevents.wait1"),
       _p3v("jg.perkevents.add.ranks", 'rPlayer.AuxVarGetFlt "*nvfAP"'),
       _p3v("jg.perkevents.add.perk", 'GetFormIDString (rPlayer.AuxVarGetRef "*nvfAK")', True),
       _p3("jg.perkevents.remove", _form("rObj", FX["toughness"]) + "rPlayer.RemovePerk rObj\nSetFunctionValue (rPlayer.HasPerk rObj)"),
       _wait3("jg.perkevents.wait2"), _wait3("jg.perkevents.wait3"),
       _p3v("jg.perkevents.remove.perk", 'GetFormIDString (rPlayer.AuxVarGetRef "*nvfRK")', True),
       _p3("jg.perkevents.clear", 'rPlayer.AuxVarErase "*nvfAP"\nrPlayer.AuxVarErase "*nvfAK"\nrPlayer.AuxVarErase "*nvfRK"\n'
           "SetFunctionValue 1")]
    # LU SetGlobalValue, put back.
    + [_p3("lu.setglobal", 'let fRes := GetGlobalValue TimeScale\nrPlayer.AuxVarSetFlt "*nvfTS" fRes\nlet fRes := SetGlobalValue TimeScale 31\n'
           'SetFunctionValue (sv_Construct "%g %g" fRes (GetGlobalValue TimeScale))', True),
       _p3("lu.setglobal.restore", 'let fRes := SetGlobalValue TimeScale (rPlayer.AuxVarGetFlt "*nvfTS")\nrPlayer.AuxVarErase "*nvfTS"\n'
           'SetFunctionValue (sv_Construct "%g %g" fRes (GetGlobalValue TimeScale))', True),
       _p3("jg.availableradios.player", "let aA := rPlayer.GetAvailableRadios\nSetFunctionValue (ar_Size aA)")]
)

# Fifty-eighth set: goldens from the original game for the functions the extender core
# builds into the engine (it asked after plugins3): GetP2PRayCastRange from the player's
# position + 100 Z out to 5000 units at headings 0/90/180/270, with the player's position
# (Doc is in another cell on this save, so no actor push); PushObject on a stimpak placed
# at the player (displacement over frames, then the stimpak is disabled and deleted);
# ToggleFirstPerson and TogglePlayerSneaking read every frame after the toggle; the forms
# GetAvailableRadios gives. Each step runs in its own frame [observed, capture plugin].
# How far the stimpak moves, and whether the switch back to first person shows in the first
# to the third frame, differed between captures; only what repeated is kept [observed].


def _e1(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\nfloat fAx\nfloat fAy\nfloat fAz\narray_var aA\n"
            "Begin Function { }\n" + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _e1v(pid, expr, text=False):
    return _e1(pid, f"SetFunctionValue ({expr})", text)


_EYE = "let fAx := rPlayer.GetPos X\nlet fAy := rPlayer.GetPos Y\nlet fAz := (rPlayer.GetPos Z) + 100\n"


def _ray(pid, dx, dy):
    return _e1(pid, _EYE + f"SetFunctionValue (Floor (GetP2PRayCastRange fAx fAy fAz (fAx + {dx}) (fAy + {dy}) fAz 0))")


def _frames(prefix, n, expr):
    return [_e1v(f"{prefix}.f{i}", expr) for i in range(n)]


_STIM = 'let rObj := rPlayer.AuxVarGetRef "*nvfST"\n'
ENGINE1_PROBES = (
    # Positions and ray casts.
    [_e1v("pos.player", 'sv_Construct "%g %g %g" (Floor (rPlayer.GetPos X)) (Floor (rPlayer.GetPos Y)) (Floor (rPlayer.GetPos Z))', True),
     _e1v("pos.cell.player", "GetFormIDString (rPlayer.GetParentCell)", True),
     _ray("ray.n", 0, 5000), _ray("ray.e", 5000, 0), _ray("ray.s", 0, -5000), _ray("ray.w", -5000, 0),
     _ray("ray.ne", 3536, 3536), _ray("ray.down", 0, 0),
     _e1("ray.down.terrain", "let fAx := rPlayer.GetPos X\nlet fAy := rPlayer.GetPos Y\nlet fAz := rPlayer.GetPos Z\n"
         "SetFunctionValue (Floor (GetP2PRayCastRange fAx fAy (fAz + 100) fAx fAy (fAz - 1000) 0))")]
    # PushObject on a placed stimpak.
    + [_e1("push.place", _form("rOther", NF["stimpak"]) + "let rObj := rPlayer.PlaceAtMe rOther 1\nrPlayer.AuxVarSetRef \"*nvfST\" rObj\n"
           "SetFunctionValue 1")]
    + [_e1(f"push.settle{i}", "SetFunctionValue 1") for i in range(5)]
    + [_e1("push.start", _STIM + 'let fAx := rObj.GetPos X\nlet fAy := rObj.GetPos Y\nrPlayer.AuxVarSetFlt "*nvfSX" fAx\n'
           'rPlayer.AuxVarSetFlt "*nvfSY" fAy\nrObj.PushObject 0 -50 0 20\nSetFunctionValue 1')]
    + [_e1(f"push.wait{i}", "SetFunctionValue 1") for i in range(8)]
    + [_e1("push.moved", _STIM + 'let fAx := (rObj.GetPos X) - (rPlayer.AuxVarGetFlt "*nvfSX")\n'
           'let fAy := (rObj.GetPos Y) - (rPlayer.AuxVarGetFlt "*nvfSY")\nSetFunctionValue ((Sqrt (fAx * fAx + fAy * fAy)) > 1)')]
    + [_e1("push.cleanup", _STIM + 'rObj.Disable\nrObj.MarkForDelete\nrPlayer.AuxVarErase "*nvfST"\nrPlayer.AuxVarErase "*nvfSX"\n'
           'rPlayer.AuxVarErase "*nvfSY"\nSetFunctionValue 1')]
    # Toggle timing, frame by frame.
    + [_e1("timing.view.toggle", "ToggleFirstPerson 0\nSetFunctionValue (IsPC1stPerson)")]
    + _frames("timing.view.after", 5, "IsPC1stPerson")
    + [_e1("timing.view.toggle.back", "ToggleFirstPerson 1\nSetFunctionValue (IsPC1stPerson)")]
    + [_e1v(f"timing.view.back.f{i}", "IsPC1stPerson") for i in range(3, 5)]
    + [_e1("timing.sneak.toggle", "TogglePlayerSneaking 1\nSetFunctionValue (rPlayer.IsSneaking)")]
    + _frames("timing.sneak.after", 5, "rPlayer.IsSneaking")
    + [_e1("timing.sneak.toggle.back", "TogglePlayerSneaking 0\nSetFunctionValue (rPlayer.IsSneaking)")]
    + _frames("timing.sneak.back", 5, "rPlayer.IsSneaking")
    # The radios the player can hear.
    + [_e1("radios.size", "let aA := rPlayer.GetAvailableRadios\nSetFunctionValue (ar_Size aA)")]
    + [_e1(f"radios.{i}", f"let aA := rPlayer.GetAvailableRadios\nlet rObj := aA[{i}]\nSetFunctionValue (GetFormIDString rObj)", True)
       for i in range(5)]
    + [_e1(f"radios.{i}.base", f"let aA := rPlayer.GetAvailableRadios\nlet rObj := aA[{i}]\nlet rOther := rObj.GetBaseObject\n"
           "SetFunctionValue (GetFormIDString rOther)", True) for i in range(5)]
)

# Fifty-ninth set: what the extender core asked after engine1 (the straight-down cast read
# 998 at the airport spot): GetP2PRayCastRange down from Z+500 and Z+100 with flags 0-4,
# north at chest height (Z+60) with flags 0-4; then PlayMusic musDocMitchellHouse (MUSC
# 171B4A) and IsMusicPlaying with the form and with none, over a few frames.


def _e2(pid, body, text=False):
    return (pid, "ref rPlayer\nref rObj\nfloat fRes\nfloat fAx\nfloat fAy\nfloat fAz\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + body, text, None, "")


_P = "let fAx := rPlayer.GetPos X\nlet fAy := rPlayer.GetPos Y\nlet fAz := rPlayer.GetPos Z\n"


def _cast(pid, z0, x1, y1, z1, flag):
    return _e2(pid, _P + f"SetFunctionValue (Floor ((GetP2PRayCastRange fAx fAy (fAz + {z0}) (fAx + {x1}) (fAy + {y1}) (fAz + {z1}) {flag}) * 10))")


ENGINE2_PROBES = (
    [_cast(f"ray.down500.f{f}", 500, 0, 0, -500, f) for f in range(5)]
    + [_cast(f"ray.down100.f{f}", 100, 0, 0, -200, f) for f in range(5)]
    + [_cast(f"ray.down100long.f{f}", 100, 0, 0, -1000, f) for f in range(5)]
    + [_cast(f"ray.north60.f{f}", 60, 0, 5000, 60, f) for f in range(5)]
    + [_e2("music.before.any", "SetFunctionValue (IsMusicPlaying)"),
       _e2("music.before.doc", _form("rObj", FX["musdoc"]) + "SetFunctionValue (IsMusicPlaying rObj)"),
       _e2("music.play", _form("rObj", FX["musdoc"]) + "PlayMusic rObj\nSetFunctionValue (IsMusicPlaying rObj)")]
    + [_e2(f"music.f{i}.doc", _form("rObj", FX["musdoc"]) + "SetFunctionValue (IsMusicPlaying rObj)") for i in range(6)]
    + [_e2("music.after.any", "SetFunctionValue (IsMusicPlaying)")]
)

# Sixtieth set: straight-down GetP2PRayCastRange casts 200 units away from the player (at
# X+200 and at Y+200) from Z+100, Z+500 and Z+1000 to Z-1000, and from Z+100 to Z-200,
# with GetTerrainHeight at the same points, as the extender core asked after engine2 (the
# casts from the player's own X, Y read about 99 below the player from Z+100).


def _e3(pid, dx, dy, z0, z1):
    return (pid, "ref rPlayer\nfloat fAx\nfloat fAy\nfloat fAz\nBegin Function { }\n" + _form("rPlayer", NF["player"])
            + f"let fAx := (rPlayer.GetPos X) + {dx}\nlet fAy := (rPlayer.GetPos Y) + {dy}\nlet fAz := rPlayer.GetPos Z\n"
            + f"SetFunctionValue (Floor ((GetP2PRayCastRange fAx fAy (fAz + {z0}) fAx fAy (fAz + {z1}) 0) * 10))", False, None, "")


def _e3t(pid, dx, dy):
    return (pid, "ref rPlayer\nfloat fAx\nfloat fAy\nBegin Function { }\n" + _form("rPlayer", NF["player"])
            + f"let fAx := (rPlayer.GetPos X) + {dx}\nlet fAy := (rPlayer.GetPos Y) + {dy}\n"
            + "SetFunctionValue (Floor (((GetTerrainHeight fAx fAy) - (rPlayer.GetPos Z)) * 10))", False, None, "")


ENGINE3_PROBES = (
    [_e3(f"ray.{k}.z{z0}", dx, dy, z0, -1000) for k, dx, dy in (("x200", 200, 0), ("y200", 0, 200)) for z0 in (100, 500, 1000)]
    + [_e3("ray.x200.z100.short", 200, 0, 100, -200)]
    + [_e3t(f"terrain.{k}", dx, dy) for k, dx, dy in (("here", 0, 0), ("x200", 200, 0), ("y200", 0, 200))]
)

# Sixty-first set: the order of GetLoadedTypeArray's elements, as the extender core asked
# (gaps1's element 0 of the weapons of FalloutNV.esm was GasTrapDummy 0001F6, not Fists
# 0001F4): elements 0-9 and the last two of type 40 for FalloutNV.esm and for every plugin,
# where some known weapons sit (ar_Find), and the first elements of type 47.


def _gl(pid, body, text=False):
    return (pid, "ref rObj\nfloat fRes\narray_var aA\nBegin Function { }\n" + body, text, None, "")


def _gl_at(pid, call, idx):
    return _gl(pid, f"let aA := {call}\nlet rObj := aA[{idx}]\nSetFunctionValue (GetFormIDString rObj)", True)


def _gl_find(pid, call, fid):
    return _gl(pid, f"let aA := {call}\n" + _form("rObj", fid) + "SetFunctionValue (ar_Find rObj aA)")


_W0 = "GetLoadedTypeArray 40 0"
_WALL = "GetLoadedTypeArray 40"
GLTA_ORDER_PROBES = (
    [_gl_at(f"glta.w0.{i}", _W0, i) for i in range(10)]
    + [_gl_at("glta.w0.last", _W0, "(ar_Size aA) - 1"), _gl_at("glta.w0.last2", _W0, "(ar_Size aA) - 2")]
    + [_gl_find(f"glta.w0.find.{k}", _W0, fid)
       for k, fid in (("fists", "1F4"), ("gastrap", "1F6"), ("pistol10", NF["pistol10"]), ("knife", "4326"),
                      ("machete", "CE569"), ("sledge", "4351"))]
    + [_gl_at(f"glta.wall.{i}", _WALL, i) for i in range(4)]
    + [_gl_at("glta.wall.last", _WALL, "(ar_Size aA) - 1")]
    + [_gl_at(f"glta.a0.{i}", "GetLoadedTypeArray 47 0", i) for i in range(5)]
)

# Sixty-second set: the most called functions of the modlist counts still not built that
# set a record field with a getter to read it back (NVSE, FOSE, LU, ShowOff, JohnnyGuitar):
# weight, weapon skill and required skill, biped and slot masks, attack damage, armour
# rating, biped model list, repair list, ammo, sight FOV, health, first-person model; and
# ActorHasBaseFlag on Doc's and the player's base forms (Doc's reference is in another
# cell on this save, and read 0 for every flag). Each value is read, set, read back and put back from the value read
# first (aux variables).


def _d1(pid, body, text=False):
    return (pid, "ref rPlayer\nref rDoc\nref rObj\nref rOther\nfloat fRes\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _num3(k, fid, get, set_to, setter, scale=1):
    """Rows k.get, k.set (read back) and k.restore for a numeric field: get is an expression
    of rObj; setter is a format with {v} for the value."""
    rd = f"SetFunctionValue (Floor (({get}) * {scale}))"
    aux = '"*nvfD1"'
    return [_d1(f"{k}.get", _form("rObj", fid) + rd),
            _d1(f"{k}.set", _form("rObj", fid) + f"let fRes := {get}\nrPlayer.AuxVarSetFlt {aux} fRes\n"
                + setter.format(v=set_to) + "\n" + rd),
            _d1(f"{k}.restore", _form("rObj", fid) + setter.format(v=f"(rPlayer.AuxVarGetFlt {aux})")
                + f"\nrPlayer.AuxVarErase {aux}\n" + rd)]


def _form3(k, fid, get, new_fid, setter):
    """The same for a form field: setter has {v} for the form variable."""
    rd = f"let rOther := {get}\nSetFunctionValue (GetFormIDString rOther)"
    return [_d1(f"{k}.get", _form("rObj", fid) + rd, True),
            _d1(f"{k}.set", _form("rObj", fid) + f"let rOther := {get}\nrPlayer.AuxVarSetRef \"*nvfD2\" rOther\n"
                + _form("rOther", new_fid) + setter.format(v="rOther") + "\n" + rd, True),
            _d1(f"{k}.restore", _form("rObj", fid) + 'let rOther := rPlayer.AuxVarGetRef "*nvfD2"\n'
                + setter.format(v="rOther") + '\nrPlayer.AuxVarErase "*nvfD2"\n' + rd, True)]


_PI = NF["pistol10"]
_LE = NF["leather"]
DATA1_PROBES = (
    _num3("weight.pistol", _PI, "GetWeight rObj", 9.5, "SetWeight {v} rObj", 100)
    + _num3("weaponskill.pistol", _PI, "GetWeaponSkill rObj", 38, "SetWeaponSkill {v} rObj")
    + _num3("reqskill.pistol", _PI, "GetWeaponRequiredSkill rObj", 50, "SetWeaponRequiredSkill {v} rObj")
    + _num3("bipedmask.leather", _LE, "GetEquipmentBipedMask rObj", 0, "SetEquipmentBipedMask {v} rObj")
    + _num3("slotsmask.leather", _LE, "GetEquipmentSlotsMask rObj", 0, "SetEquipmentSlotsMask {v} rObj")
    + [_d1("slotsmask.set.return", _form("rObj", _LE) + "let fRes := GetEquipmentSlotsMask rObj\n"
           "SetFunctionValue (SetEquipmentSlotsMask fRes rObj)")]
    + _num3("attackdamage.pistol", _PI, "GetAttackDamage rObj", 30, "SetAttackDamage {v} rObj")
    + _num3("armorrating.leather", _LE, "GetArmorArmorRating rObj", 77, "SetArmorArmorRating {v} rObj")
    + _num3("sightfov.pistol", _PI, "GetWeaponSightFOV rObj", 40, "SetWeaponSightFOV {v} rObj", 100)
    + _num3("health.pistol", _PI, "GetHealth rObj", 300, "SetObjectHealth {v} rObj")
    + _form3("bipedlist.leather", _LE, "GetBipedModelList rObj", NF["list127"], "SetBipedModelList rObj {v}")
    + _form3("repairlist.pistol", _PI, "GetRepairList rObj", NF["list127"], "SetRepairList {v} rObj")
    + _form3("ammo.pistol", _PI, "GetWeaponAmmo rObj", FX["ammo556"], "SetWeaponAmmo {v} rObj")
    + [_d1(f"fpmodel.pistol.{i}", _form("rObj", _PI) + f"let rOther := GetWeapon1stPersonModel rObj {i}\n"
           "SetFunctionValue (GetFormIDString rOther)", True) for i in range(4)]
    + [_d1("fpmodel.pistol.set0", _form("rObj", _PI) + _form("rDoc", NF["hunting"])
           + 'let rOther := GetWeapon1stPersonModel rObj 0\nrPlayer.AuxVarSetRef "*nvfD2" rOther\n'
           "let rOther := GetWeapon1stPersonModel rDoc 0\nSetWeapon1stPersonModel rObj 0 rOther\n"
           "let rOther := GetWeapon1stPersonModel rObj 0\nSetFunctionValue (GetFormIDString rOther)", True),
       _d1("fpmodel.pistol.set0.restore", _form("rObj", _PI) + 'let rOther := rPlayer.AuxVarGetRef "*nvfD2"\n'
           'SetWeapon1stPersonModel rObj 0 rOther\nrPlayer.AuxVarErase "*nvfD2"\n'
           "let rOther := GetWeapon1stPersonModel rObj 0\nSetFunctionValue (GetFormIDString rOther)", True)]
    + [_d1(f"basefl.{k}.{i}", _form("rObj", fid) + f"SetFunctionValue (ActorHasBaseFlag {i} rObj)")
       for k, fid in (("docbase", NF["doc"]), ("playerbase", NF["playerbase"])) for i in range(16)]
)

# Sixty-third set: ActorHasBaseFlag on bases where flag 1 (essential) should show
# (GSSunnySmiles 104E84, CraigBoone 092BD2, RoseofSharonCassidy 133FDD, Benny 101C9B) and
# called on the player's reference, next to GetActorBaseFlagsLow of the same bases; then
# more data functions still unbuilt in the counts: Set/GetActorBaseFlagsLow, Set/GetNPCHeight
# (Sunny, put back), GetEquippedData and GetNoUnequip on the equipped 10mm pistol,
# SetActivatorWaterType (ElectricBox01, put back), ar_Cat. Forms from fnvtool esm inventory
# [observed].
FX.update(sunny="104E84", boone="92BD2", cass="133FDD", benny="101C9B", cleanwater="1009CA")


def _d2(pid, body, text=False):
    return (pid, "ref rPlayer\nref rObj\nref rOther\nfloat fRes\narray_var aA\narray_var aB\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + body, text, None, "")


_NPCS = (("sunny", FX["sunny"]), ("boone", FX["boone"]), ("cass", FX["cass"]), ("benny", FX["benny"]),
         ("doc", NF["doc"]), ("player", NF["playerbase"]))
_REF2 = "let rObj := rPlayer.GetEquippedItemRef 5\n"
DATA2_PROBES = (
    [_d2(f"baseflags.{k}", _form("rObj", fid) + "SetFunctionValue (GetActorBaseFlagsLow rObj)") for k, fid in _NPCS]
    + [_d2(f"hasbaseflag.{k}.{i}", _form("rObj", fid) + f"SetFunctionValue (ActorHasBaseFlag {i} rObj)")
       for k, fid in _NPCS[:4] for i in (0, 1, 3, 4)]
    + [_d2(f"hasbaseflag.playerref.{i}", f"SetFunctionValue (rPlayer.ActorHasBaseFlag {i})") for i in range(6)]
    + [_d2("baseflags.playerref", "SetFunctionValue (rPlayer.GetActorBaseFlagsLow)")]
    # Base flags set on Sunny's base, put back.
    + [_d2("baseflags.sunny.set", _form("rObj", FX["sunny"]) + 'let fRes := GetActorBaseFlagsLow rObj\nrPlayer.AuxVarSetFlt "*nvfBF" fRes\n'
           "SetActorBaseFlagsLow (fRes + 8) rObj\nSetFunctionValue (GetActorBaseFlagsLow rObj)"),
       _d2("baseflags.sunny.set.flag3", _form("rObj", FX["sunny"]) + "SetFunctionValue (ActorHasBaseFlag 3 rObj)"),
       _d2("baseflags.sunny.restore", _form("rObj", FX["sunny"]) + 'SetActorBaseFlagsLow (rPlayer.AuxVarGetFlt "*nvfBF") rObj\n'
           'rPlayer.AuxVarErase "*nvfBF"\nSetFunctionValue (GetActorBaseFlagsLow rObj)')]
    # NPC height, put back.
    + [_d2(f"npcheight.{k}", _form("rObj", fid) + "SetFunctionValue (Floor ((GetNPCHeight rObj) * 1000))") for k, fid in _NPCS[:5]]
    + [_d2("npcheight.sunny.set", _form("rObj", FX["sunny"]) + 'let fRes := GetNPCHeight rObj\nrPlayer.AuxVarSetFlt "*nvfNH" fRes\n'
           "let fRes := SetNPCHeight 1.25 rObj\nSetFunctionValue (sv_Construct \"%g %g\" (Floor (fRes * 1000)) (Floor ((GetNPCHeight rObj) * 1000)))", True),
       _d2("npcheight.sunny.restore", _form("rObj", FX["sunny"]) + 'let fRes := SetNPCHeight (rPlayer.AuxVarGetFlt "*nvfNH") rObj\n'
           'rPlayer.AuxVarErase "*nvfNH"\nSetFunctionValue (Floor ((GetNPCHeight rObj) * 1000))')]
    # Equipped data on the 10mm pistol (added, equipped, removed after).
    + [_d2("eqdata.add", _form("rObj", NF["pistol10"]) + "rPlayer.AddItem rObj 1 1\nrPlayer.EquipItem rObj 0 1\nSetFunctionValue 1")]
    + [_d2(f"eqdata.wait{i}", "SetFunctionValue 1") for i in range(3)]
    + [_d2("eqdata.size", "let aA := rPlayer.GetEquippedData 5\nSetFunctionValue (ar_Size aA)"),
       _d2("eqdata.0", "let aA := rPlayer.GetEquippedData 5\nlet rObj := aA[0]\nSetFunctionValue (GetFormIDString rObj)", True),
       _d2("eqdata.1", "let aA := rPlayer.GetEquippedData 5\nlet fRes := aA[1]\nSetFunctionValue (Floor (fRes * 100))"),
       _d2("eqdata.2", "let aA := rPlayer.GetEquippedData 5\nlet fRes := aA[2]\nSetFunctionValue fRes"),
       _d2("eqdata.none", "let aA := rPlayer.GetEquippedData 9\nSetFunctionValue (ar_Size aA)"),
       _d2("nounequip.pistol", _REF2 + "SetFunctionValue (rObj.GetNoUnequip)"),
       _d2("eqdata.remove", _form("rObj", NF["pistol10"]) + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\n"
           "SetFunctionValue (rPlayer.GetItemCount rObj)")]
    # Activator water type, put back; ar_Cat.
    + [_d2("waterset.ebox", _form("rObj", FX["ebox"]) + _form("rOther", FX["cleanwater"])
           + 'let rPlayer := GetActivatorWaterType rObj\nPlayerRef.AuxVarSetRef "*nvfWT" rPlayer\n'
           "SetActivatorWaterType rOther rObj\nlet rOther := GetActivatorWaterType rObj\nSetFunctionValue (GetFormIDString rOther)", True),
       _d2("waterset.ebox.restore", _form("rObj", FX["ebox"]) + 'let rOther := rPlayer.AuxVarGetRef "*nvfWT"\n'
           'SetActivatorWaterType rOther rObj\nrPlayer.AuxVarErase "*nvfWT"\n'
           "let rOther := GetActivatorWaterType rObj\nSetFunctionValue (GetFormIDString rOther)", True),
       _d2("arcat", "let aA := ar_List 1 2\nlet aB := ar_List 3 4 5\nar_Cat aA aB\nSetFunctionValue (ar_Size aA)"),
       _d2("arcat.last", "let aA := ar_List 1 2\nlet aB := ar_List 3 4 5\nar_Cat aA aB\nlet fRes := aA[(ar_Size aA) - 1]\nSetFunctionValue fRes")]
)

# Sixty-fourth set: the rest of the readable unbuilt functions the extender core listed after
# data2: Get/SetMapMarkerName on the Goodsprings marker (session only, the name is a string),
# SortFormsByType's element order, ShowOff ListAddArray/ListAddList at index 0 on list127
# (removed again with ListRemoveNthForm 0), ToggleItemUnique with IsItemUnique (put back),
# RewardXPExact against the XP actor value (5 XP, session only), and JohnnyGuitar's
# actor-value-change event on Luck (ModAV Luck +1 then -1).


def _d3(pid, body, text=False):
    return (pid, "ref rPlayer\nref rObj\nref rOther\nfloat fRes\narray_var aA\narray_var aB\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + body, text, None, "")


def _wait3d(pid):
    return _d3(pid, "SetFunctionValue 1")


_SORT = (_form("rObj", NF["stimpak"]) + _form("rOther", NF["pistol10"])
         + "let aA := ar_List rObj rOther\n" + _form("rObj", NF["leather"]) + _form("rOther", FX["nuka_cola"])
         + "ar_Append aA rObj\nar_Append aA rOther\n" + _form("rObj", FX["knife"]) + "ar_Append aA rObj\n"
         + "let aB := ar_List 40 47\nlet aA := SortFormsByType aA aB\n")
_L = _form("rObj", NF["list127"])
DATA3_PROBES = (
    [_d3("marker.name", _form("rObj", FX["gsmarker"]) + "SetFunctionValue (rObj.GetMapMarkerName)", True),
     _d3("marker.name.set", _form("rObj", FX["gsmarker"]) + 'rObj.SetMapMarkerName "NVF Springs"\nSetFunctionValue (rObj.GetMapMarkerName)', True),
     _d3("sort.size", _SORT + "SetFunctionValue (ar_Size aA)")]
    + [_d3(f"sort.{i}", _SORT + f"let rObj := aA[{i}]\nSetFunctionValue (GetFormIDString rObj)", True) for i in range(5)]
    # Form lists at index 0.
    + [_d3("data3.list.count", _L + "SetFunctionValue (ListGetCount rObj)"),
       _d3("listaddarray.0", _L + _form("rOther", NF["stimpak"]) + "let aA := ar_List rOther\n"
           "let fRes := ListAddArray rObj aA 0\nlet rOther := ListGetNthForm rObj 0\n"
           'SetFunctionValue (sv_Construct "%g %g %i" fRes (ListGetCount rObj) rOther)', True),
       _d3("listaddarray.0.remove", _L + "ListRemoveNthForm rObj 0\nSetFunctionValue (ListGetCount rObj)"),
       _d3("listaddlist.0", _L + _form("rOther", FX["ammolist10"])
           + "let fRes := ListAddList rObj rOther 0\nlet rOther := ListGetNthForm rObj 0\n"
           'SetFunctionValue (sv_Construct "%g %g %i" fRes (ListGetCount rObj) rOther)', True),
       _d3("listaddlist.ammolist.count", _form("rObj", FX["ammolist10"]) + "SetFunctionValue (ListGetCount rObj)"),
       _d3("listaddlist.0.remove", _L + _form("rOther", FX["ammolist10"])
           + "let fRes := (ListGetCount rObj) - 3\nwhile fRes > 0\nListRemoveNthForm rObj 0\nlet fRes := fRes - 1\nloop\n"
           "SetFunctionValue (ListGetCount rObj)"),
       _d3("list.last", _L + "let rOther := ListGetNthForm rObj ((ListGetCount rObj) - 1)\nSetFunctionValue (GetFormIDString rOther)", True)]
    # Unique items, put back.
    + [_d3("unique.stimpak", _form("rObj", NF["stimpak"]) + "SetFunctionValue (IsItemUnique rObj)"),
       _d3("unique.pistol", _form("rObj", NF["pistol10"]) + "SetFunctionValue (IsItemUnique rObj)"),
       _d3("unique.toggle", _form("rObj", NF["stimpak"]) + "ToggleItemUnique rObj 1\nSetFunctionValue (IsItemUnique rObj)"),
       _d3("unique.restore", _form("rObj", NF["stimpak"]) + "ToggleItemUnique rObj 0\nSetFunctionValue (IsItemUnique rObj)")]
    # RewardXPExact.
    + [_d3("xp.before", "SetFunctionValue (rPlayer.GetActorValue XP)"),
       _d3("xp.reward", "RewardXPExact 5\nSetFunctionValue (rPlayer.GetActorValue XP)"),
       _wait3d("xp.wait0"),
       _d3("xp.after", "SetFunctionValue (rPlayer.GetActorValue XP)")]
    # JohnnyGuitar actor-value change on Luck.
    + [_d3("avchange.set", 'SetOnActorValueChangeEventHandler 1 (Begin Function {int iAV, float fOld, float fNew}\n'
           '  if iAV == 11\n    rPlayer.AuxVarSetFlt "*nvfAV" (iAV * 10000 + fOld * 100 + fNew)\n  endif\nEnd) 0\nSetFunctionValue 1'),
       _d3("avchange.mod", 'rPlayer.ModActorValue Luck 1\nSetFunctionValue (rPlayer.GetActorValue Luck)'),
       _wait3d("avchange.wait0"), _wait3d("avchange.wait1"),
       _d3("avchange.got", 'SetFunctionValue (rPlayer.AuxVarGetFlt "*nvfAV")'),
       _d3("avchange.restore", 'rPlayer.ModActorValue Luck -1\nSetFunctionValue (rPlayer.GetActorValue Luck)'),
       _wait3d("avchange.wait2"), _wait3d("avchange.wait3"),
       _d3("avchange.got.after", 'let fRes := rPlayer.AuxVarGetFlt "*nvfAV"\nrPlayer.AuxVarErase "*nvfAV"\nSetFunctionValue fRes')]
)

# Sixty-fifth set (this machine's share of the capture split, 2026-10-27: animation): the
# player in third person, IsAnimPlaying and JIP IsAnimPlayingEx (categories 1 misc, 2
# movement, 3 attack, 4 reload; sub-type 0 any, 1 idle) standing, then kNVSE ForcePlayIdle
# LooseCheeringClappingNoArmsUp (IDLE 171DBB) read frame by frame, ForceStopIdle, the idle
# again and JIP StopIdle, with JIP GetPlayedIdle each frame (IsAnimPlayingEx 1 1 read 1
# while the player only stood, so it cannot show the idle [observed]). The view is put back. Rows whose frame of change varies between
# captures are dropped after a second capture.
FX.update(clapidle="171DBB")


def _a1(pid, body, text=False):
    return (f"anim.{pid}", "ref rPlayer\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"]) + body,
            text, None, "")


def _a1v(pid, expr):
    return _a1(pid, f"SetFunctionValue ({expr})")


_EX = ((1, 0), (1, 1), (2, 0), (3, 0), (4, 0))


def _anim_rows(prefix):
    return ([_a1v(f"{prefix}.isanimplaying", "rPlayer.IsAnimPlaying")]
            + [_a1v(f"{prefix}.ex.{c}.{t}", f"rPlayer.IsAnimPlayingEx {c} {t} 0") for c, t in _EX])


_PLAYED = "let rObj := rPlayer.GetPlayedIdle\nSetFunctionValue (GetFormIDString rObj)"


def _played(pid):
    return _a1(pid, _PLAYED, True)


ANIM1_PROBES = (
    [_a1("view.third", 'let fRes := IsPC1stPerson\nrPlayer.AuxVarSetFlt "*nvfA1" fRes\nToggleFirstPerson 0\nSetFunctionValue fRes')]
    + [_a1(f"view.wait{i}", "SetFunctionValue 1") for i in range(4)]
    + _anim_rows("stand") + [_played("stand.played")]
    + [_a1("idle.play", _form("rObj", FX["clapidle"]) + "rPlayer.ForcePlayIdle rObj\nSetFunctionValue 1")]
    + [_played(f"idle.f{i}.played") for i in range(6)]
    + _anim_rows("idle")
    + [_a1("idle.forcestop", "rPlayer.ForceStopIdle\nSetFunctionValue 1")]
    + [_played(f"forcestop.f{i}.played") for i in range(4)]
    + [_a1("idle.play2", _form("rObj", FX["clapidle"]) + "rPlayer.ForcePlayIdle rObj\nSetFunctionValue 1")]
    + [_a1(f"idle2.wait{i}", "SetFunctionValue 1") for i in range(4)]
    + [_played("idle2.played"),
       _a1("idle.stopidle", "rPlayer.StopIdle\nSetFunctionValue 1")]
    + [_played(f"stopidle.f{i}.played") for i in range(4)]
    + _anim_rows("after")
    + [_a1("view.restore", 'ToggleFirstPerson (rPlayer.AuxVarGetFlt "*nvfA1")\nrPlayer.AuxVarErase "*nvfA1"\nSetFunctionValue 1')]
    + [_a1(f"view.rwait{i}", "SetFunctionValue 1") for i in range(4)]
    + [_a1v("view.restored", "IsPC1stPerson")]
)

# Sixty-sixth set: what the extender core asked after anim1. The player (third person, put
# back): ForcePlayIdle, ForceStopIdle, then GetPlayedIdle 30 frames later. A placed
# NVProspectorFemaleT (NPC_ 17B586) placed at the player, then after 10 frames moved 400
# units north (moved at once, or 1500 away, it read 0 even standing): IsAnimPlayingEx standing, then
# walking to the player as a teammate over 120 frames; a second one 600 north, and the two
# fight for 360 frames (knife against 10mm pistol), each frame keeping in an aux variable
# whether IsAnimPlayingEx 3 (attack) or 4 (reload) was ever 1. PlayGroup AttackRight and
# ReloadA read 0 in a first run, and two prospectors would not fight each other, so both
# attack the player, with god mode on (LU Console "tgm") for the fight and off after. Both
# NPCs are disabled and deleted.
FX.update(prospector="17B586")


def _a2(pid, body, text=False):
    return (f"anim2.{pid}", "ref rPlayer\nref rNpc\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"])
            + 'let rNpc := rPlayer.AuxVarGetRef "*nvfN2"\n' + body, text, None, "")


def _a2v(pid, expr):
    return _a2(pid, f"SetFunctionValue ({expr})")


def _w2(pid, n):
    return [_a2(f"{pid}{i}", "SetFunctionValue 1") for i in range(n)]


def _ex2(pid, who="rNpc"):
    return [_a2v(f"{pid}.ex.{c}.{t}", f"{who}.IsAnimPlayingEx {c} {t} 0") for c, t in ((1, 0), (1, 1), (2, 0), (3, 0), (4, 0))]


ANIM2_PROBES = (
    # The player: a stop, then 30 frames.
    [_a2("view.third", 'let fRes := IsPC1stPerson\nrPlayer.AuxVarSetFlt "*nvfA2" fRes\nToggleFirstPerson 0\nSetFunctionValue fRes')]
    + _w2("view.wait", 4)
    + [_a2("pc.play", _form("rObj", FX["clapidle"]) + "rPlayer.ForcePlayIdle rObj\nSetFunctionValue 1")]
    + _w2("pc.playwait", 3)
    + [_a2("pc.stop", "rPlayer.ForceStopIdle\nSetFunctionValue 1")]
    + _w2("pc.stopwait", 30)
    + [_a2("pc.played.f30", "let rObj := rPlayer.GetPlayedIdle\nSetFunctionValue (GetFormIDString rObj)", True)]
    + _ex2("pc.f30", "rPlayer")
    + [_a2("view.restore", 'ToggleFirstPerson (rPlayer.AuxVarGetFlt "*nvfA2")\nrPlayer.AuxVarErase "*nvfA2"\nSetFunctionValue 1')]
    # Two NPCs, 1500 and 1700 units north of the player.
    + [_a2("npc.place", _form("rObj", FX["prospector"]) + 'let rNpc := rPlayer.PlaceAtMe rObj 1\n'
           'rPlayer.AuxVarSetRef "*nvfN2" rNpc\nlet rNpc := rPlayer.PlaceAtMe rObj 1\nrPlayer.AuxVarSetRef "*nvfN3" rNpc\n'
           "SetFunctionValue 1")]
    + _w2("npc.wait", 10)
    + [_a2("npc.move", 'rNpc.SetPos Y ((rPlayer.GetPos Y) + 400)\nlet rObj := rPlayer.AuxVarGetRef "*nvfN3"\n'
           "rObj.SetPos Y ((rPlayer.GetPos Y) + 600)\nSetFunctionValue 1")]
    + _w2("npc.movewait", 10)
    + _ex2("npc.stand")
    + [_a2v("npc.stand.dist.far", "(rNpc.GetDistance rPlayer) > 300")]
    # Walking as a teammate, 120 frames.
    + [_a2("npc.follow", 'rNpc.SetPlayerTeammate 1\nrNpc.EvaluatePackage\nrPlayer.AuxVarSetFlt "*nvfMV" 0\n'
           'let fRes := rNpc.GetDistance rPlayer\nrPlayer.AuxVarSetFlt "*nvfMD" fRes\nSetFunctionValue 1')]
    + [_a2(f"npc.walk.f{i}", 'let fRes := rNpc.IsAnimPlayingEx 2 0 0\n'
           'if fRes > (rPlayer.AuxVarGetFlt "*nvfMV")\nrPlayer.AuxVarSetFlt "*nvfMV" fRes\nendif\nSetFunctionValue 1') for i in range(120)]
    + [_a2v("npc.walk.ever", 'rPlayer.AuxVarGetFlt "*nvfMV"'),
       _a2v("npc.walk.closer", '(rNpc.GetDistance rPlayer) < ((rPlayer.AuxVarGetFlt "*nvfMD") - 50)'),
       _a2("npc.unfollow", 'rNpc.SetPlayerTeammate 0\nrNpc.EvaluatePackage\nrPlayer.AuxVarErase "*nvfMV"\nrPlayer.AuxVarErase "*nvfMD"\nSetFunctionValue 1')]
    # A fight: the first with a knife, the second with a 10mm pistol.
    + [_a2("fight.arm", _form("rObj", FX["knife"]) + "rNpc.AddItem rObj 1 1\nrNpc.EquipItem rObj 0 1\n"
           'let rNpc := rPlayer.AuxVarGetRef "*nvfN3"\n' + _form("rObj", NF["pistol10"]) + "rNpc.AddItem rObj 1 1\n"
           "rNpc.EquipItem rObj 0 1\n" + _form("rObj", NF["ammo10"]) + "rNpc.AddItem rObj 40 1\nSetFunctionValue 1")]
    + _w2("fight.armwait", 5)
    + [_a2("fight.godmode", 'Console "tgm"\nSetFunctionValue 1')]
    + [_a2("fight.start", 'let rObj := rPlayer.AuxVarGetRef "*nvfN3"\nrNpc.StartCombat rPlayer\nrObj.StartCombat rPlayer\n'
           'rPlayer.AuxVarSetFlt "*nvfK2" 0\n'
           'rPlayer.AuxVarSetFlt "*nvfK3" 0\nrPlayer.AuxVarSetFlt "*nvfP3" 0\nrPlayer.AuxVarSetFlt "*nvfP4" 0\n'
           'let fRes := GameHour\nrPlayer.AuxVarSetFlt "*nvfGH" fRes\nSetFunctionValue 1')]
    + _w2("fight.startwait", 30)
    + [_a2v("fight.incombat.knife", "rNpc.IsInCombat"),
       _a2("fight.target.knife", "let rObj := rNpc.GetCombatTarget\nSetFunctionValue (rObj == rPlayer)")]
    + [_a2(f"fight.f{i}", 'let rObj := rPlayer.AuxVarGetRef "*nvfN3"\n'
           'if (rNpc.IsAnimPlayingEx 3 0 0)\nrPlayer.AuxVarSetFlt "*nvfK3" 1\nendif\n'
           'if (rNpc.IsAnimPlayingEx 2 0 0)\nrPlayer.AuxVarSetFlt "*nvfK2" 1\nendif\n'
           'if (rObj.IsAnimPlayingEx 3 0 0)\nrPlayer.AuxVarSetFlt "*nvfP3" 1\nendif\n'
           'if (rObj.IsAnimPlayingEx 4 0 0)\nrPlayer.AuxVarSetFlt "*nvfP4" 1\nendif\nSetFunctionValue 1') for i in range(360)]
    + [_a2v("fight.gamehour.advanced", '(GameHour - (rPlayer.AuxVarGetFlt "*nvfGH")) > 0'),
       _a2v("fight.knife.move.ever", 'rPlayer.AuxVarGetFlt "*nvfK2"'),
       _a2v("fight.health.knife.lost", "(rNpc.GetActorValue Health) < (rNpc.GetBaseActorValue Health)"),
       _a2("fight.health.pistol.lost", 'let rObj := rPlayer.AuxVarGetRef "*nvfN3"\n'
           "SetFunctionValue ((rObj.GetActorValue Health) < (rObj.GetBaseActorValue Health))"),
       _a2v("fight.knife.attack.ever", 'rPlayer.AuxVarGetFlt "*nvfK3"'),
       _a2v("fight.pistol.attack.ever", 'rPlayer.AuxVarGetFlt "*nvfP3"'),
       _a2v("fight.pistol.reload.ever", 'rPlayer.AuxVarGetFlt "*nvfP4"'),
       _a2("fight.cleanup", 'let rObj := rPlayer.AuxVarGetRef "*nvfN3"\nrNpc.Disable\nrNpc.MarkForDelete\n'
           'rObj.Disable\nrObj.MarkForDelete\nConsole "tgm"\nrPlayer.AuxVarErase "*nvfK2"\nrPlayer.AuxVarErase "*nvfN2"\nrPlayer.AuxVarErase "*nvfN3"\n'
           'rPlayer.AuxVarErase "*nvfK3"\nrPlayer.AuxVarErase "*nvfP3"\nrPlayer.AuxVarErase "*nvfP4"\nrPlayer.AuxVarErase "*nvfGH"\nSetFunctionValue 1')]
)


# reload1 (writes, session only): IsAnimPlayingEx category 4 (reload). The player, in third
# person, is given a 10mm pistol (434F) and 20 rounds; the clip is set to 1 round
# (SetCurrentAmmoRounds), TapControl 7 (the reload/ready control, wiki GetControlCodeByID
# example) draws the weapon, a second tap 60 frames later reloads, and each frame of the 180
# after keeps in an aux variable whether IsAnimPlayingEx 4 0 was ever 1. Then god mode: a
# 10-point DamageActorValue Health with it off and on (LU Console "tgm"). Then a placed
# NVProspectorFemaleT with the pistol and 20 rounds fights the player (god mode on) for 600
# frames, its clip set to 1 round at the start, keeping whether 4 0 was ever 1 and whether its
# clip ever went above 1. The player's weapon, ammo count, view and health are put back; the
# NPC is disabled and deleted.
def _r1(pid, body, text=False):
    return (f"reload1.{pid}", "ref rPlayer\nref rNpc\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"])
            + 'let rNpc := rPlayer.AuxVarGetRef "*nvfR1"\n' + body, text, None, "")


def _r1v(pid, expr):
    return _r1(pid, f"SetFunctionValue ({expr})")


def _r1w(pid, n):
    return [_r1(f"{pid}{i}", "SetFunctionValue 1") for i in range(n)]


def _r1ever(who, var):
    return f'if ({who}.IsAnimPlayingEx 4 0 0)\nrPlayer.AuxVarSetFlt "{var}" 1\nendif\n'


RELOAD1_PROBES = (
    # Save the view, weapon and ammo count.
    [_r1("save", 'let fRes := IsPC1stPerson\nrPlayer.AuxVarSetFlt "*nvfRV" fRes\nToggleFirstPerson 0\n'
         'let rObj := rPlayer.GetEquippedObject 5\nrPlayer.AuxVarSetRef "*nvfRW" rObj\n'
         + _form("rObj", NF["ammo10"]) + 'let fRes := rPlayer.GetItemCount rObj\nrPlayer.AuxVarSetFlt "*nvfRA" fRes\nSetFunctionValue 1')]
    + _r1w("viewwait", 5)
    + [_r1("pc.arm", _form("rObj", NF["pistol10"]) + "rPlayer.AddItem rObj 1 1\nrPlayer.EquipItem rObj 0 1\n"
           + _form("rObj", NF["ammo10"]) + "rPlayer.AddItem rObj 20 1\nSetFunctionValue 1")]
    + _r1w("pc.armwait", 10)
    + [_r1("pc.clip1", "rPlayer.SetCurrentAmmoRounds 1\nSetFunctionValue 1")]
    + _r1w("pc.clipwait", 2)
    + [_r1v("pc.clip1.read", "rPlayer.GetCurrentAmmoRounds"),
       _r1("pc.draw", "TapControl 7\nSetFunctionValue 1")]
    + _r1w("pc.drawwait", 60)
    + [_r1v("pc.drawn", "rPlayer.IsWeaponOut"),
       _r1v("pc.drawn.clip", "rPlayer.GetCurrentAmmoRounds"),
       _r1("pc.reload", 'rPlayer.AuxVarSetFlt "*nvfRP" 0\nTapControl 7\nSetFunctionValue 1')]
    + [_r1(f"pc.f{i}", _r1ever("rPlayer", "*nvfRP") + "SetFunctionValue 1") for i in range(180)]
    + [_r1v("pc.reload.ever", 'rPlayer.AuxVarGetFlt "*nvfRP"'),
       _r1v("pc.reload.clip", "rPlayer.GetCurrentAmmoRounds")]
    # God mode: damage with it off, then on.
    + [_r1("god.off.damage", "let fRes := rPlayer.GetActorValue Health\nrPlayer.DamageActorValue Health 10\n"
           "let fRes := fRes - (rPlayer.GetActorValue Health)\nrPlayer.RestoreActorValue Health 10\nSetFunctionValue fRes"),
       _r1("god.on", 'Console "tgm"\nSetFunctionValue 1')]
    + _r1w("god.onwait", 2)
    + [_r1("god.on.damage", "let fRes := rPlayer.GetActorValue Health\nrPlayer.DamageActorValue Health 10\n"
           "let fRes := fRes - (rPlayer.GetActorValue Health)\nrPlayer.RestoreActorValue Health 10\nSetFunctionValue fRes")]
    # The NPC, placed at the player, moved 500 north after 10 frames.
    + [_r1("npc.place", _form("rObj", FX["prospector"]) + 'let rNpc := rPlayer.PlaceAtMe rObj 1\n'
           'rPlayer.AuxVarSetRef "*nvfR1" rNpc\nSetFunctionValue 1')]
    + _r1w("npc.wait", 10)
    + [_r1("npc.move", "rNpc.SetPos Y ((rPlayer.GetPos Y) + 500)\nSetFunctionValue 1")]
    + _r1w("npc.movewait", 10)
    + [_r1("npc.arm", _form("rObj", NF["pistol10"]) + "rNpc.AddItem rObj 1 1\nrNpc.EquipItem rObj 0 1\n"
           + _form("rObj", NF["ammo10"]) + "rNpc.AddItem rObj 20 1\nSetFunctionValue 1")]
    + _r1w("npc.armwait", 5)
    + [_r1("fight.start", 'rNpc.StartCombat rPlayer\nrPlayer.AuxVarSetFlt "*nvfRN" 0\nrPlayer.AuxVarSetFlt "*nvfRC" 0\nSetFunctionValue 1')]
    + _r1w("fight.startwait", 30)
    + [_r1v("fight.incombat", "rNpc.IsInCombat"),
       _r1("fight.clip1", "rNpc.SetCurrentAmmoRounds 1\nSetFunctionValue 1")]
    + _r1w("fight.clipwait", 2)
    + [_r1v("fight.clip1.read", "rNpc.GetCurrentAmmoRounds")]
    + [_r1(f"fight.f{i}", _r1ever("rNpc", "*nvfRN")
           + 'if (rNpc.GetCurrentAmmoRounds) > 1\nrPlayer.AuxVarSetFlt "*nvfRC" 1\nendif\nSetFunctionValue 1') for i in range(600)]
    + [_r1v("fight.reload.ever", 'rPlayer.AuxVarGetFlt "*nvfRN"'),
       _r1v("fight.clip.refilled", 'rPlayer.AuxVarGetFlt "*nvfRC"'),
       _r1("fight.cleanup", 'rNpc.Disable\nrNpc.MarkForDelete\nConsole "tgm"\nrPlayer.AuxVarErase "*nvfR1"\n'
           'rPlayer.AuxVarErase "*nvfRN"\nrPlayer.AuxVarErase "*nvfRC"\nrPlayer.AuxVarErase "*nvfRP"\nSetFunctionValue 1')]
    + _r1w("god.offwait", 2)
    + [_r1("god.off.again.damage", "let fRes := rPlayer.GetActorValue Health\nrPlayer.DamageActorValue Health 10\n"
           "let fRes := fRes - (rPlayer.GetActorValue Health)\nrPlayer.RestoreActorValue Health 10\nSetFunctionValue fRes")]
    # Put back the weapon, the ammo count and the view.
    + [_r1("restore.weapon", _form("rObj", NF["pistol10"]) + "rPlayer.UnequipItem rObj 0 1\nrPlayer.RemoveItem rObj 1 1\n"
           'let rObj := rPlayer.AuxVarGetRef "*nvfRW"\nif IsFormValid rObj\nrPlayer.EquipItem rObj 0 1\nendif\nSetFunctionValue 1'),
       _r1("restore.ammo", _form("rObj", NF["ammo10"]) + 'let fRes := (rPlayer.GetItemCount rObj) - (rPlayer.AuxVarGetFlt "*nvfRA")\n'
           "if fRes > 0\nrPlayer.RemoveItem rObj fRes 1\nelseif fRes < 0\nrPlayer.AddItem rObj (0 - fRes) 1\nendif\nSetFunctionValue 1"),
       _r1("restore.ammo.check", _form("rObj", NF["ammo10"])
           + 'SetFunctionValue ((rPlayer.GetItemCount rObj) == (rPlayer.AuxVarGetFlt "*nvfRA"))')]
    + [_r1("restore.view", 'ToggleFirstPerson (rPlayer.AuxVarGetFlt "*nvfRV")\nrPlayer.AuxVarErase "*nvfRV"\n'
           'rPlayer.AuxVarErase "*nvfRW"\nrPlayer.AuxVarErase "*nvfRA"\nSetFunctionValue 1')]
)


# sound1 (writes, session only): when IsSoundPlaying turns on and off, frame by frame. gaps1
# read UIVATSLoop (15511B) as 0 in the frame of PlaySound and the next, 1 in the frame of
# StopSound and 0 after, so here each frame after PlaySound is a row: the loop, 12 frames, then
# GetSoundPlayers, StopSound and 6 frames; EMTMedicalBeep01 (174325) by PlaySound, 12 frames
# and then at 30, 60 and 120 (still playing), then StopSound; the beep by PlaySound3D on the player, IsSoundPlaying with the
# player and with Doc's base as the source, GetSoundPlayers, StopSoundAlt (JohnnyGuitar) and
# 6 frames. IsSoundPlayingFromPath with the path GetSoundSourceFile gives and with "Data\Sound\"
# before it. Only rows that repeat on a second capture are kept.
def _s1(pid, body, text=False):
    return (f"sound1.{pid}", "ref rPlayer\nref rObj\nref rOther\nfloat fRes\narray_var aArr\nBegin Function { }\n"
            + _form("rPlayer", NF["player"]) + body, text, None, "")


_S1L = _form("rObj", FX["vatsloop"])
_S1B = _form("rObj", FX["beep"])


def _s1f(pid, pre, n, src=""):
    return [_s1(f"{pid}.f{i}", pre + f"SetFunctionValue (IsSoundPlaying rObj{src})") for i in range(n)]


def _s1w(pid, n):
    return [_s1(f"{pid}{i}", "SetFunctionValue 1") for i in range(n)]


def _s1players(pid, pre):
    return [_s1(f"{pid}.players.size", pre + "let aArr := GetSoundPlayers rObj\nSetFunctionValue (ar_Size aArr)")]


SOUND1_PROBES = (
    # The loop.
    [_s1("loop.before", _S1L + "SetFunctionValue (IsSoundPlaying rObj)"),
     _s1("loop.file", _S1L + "SetFunctionValue (GetSoundSourceFile rObj)", True),
     _s1("loop.play", _S1L + "PlaySound rObj\nSetFunctionValue (IsSoundPlaying rObj)")]
    + _s1f("loop", _S1L, 12)
    + [_s1("loop.f12.player", _S1L + "SetFunctionValue (IsSoundPlaying rObj rPlayer)")]
    + _s1players("loop", _S1L)
    + [_s1("loop.stop", _S1L + "StopSound rObj\nSetFunctionValue (IsSoundPlaying rObj)")]
    + _s1f("loop.stopped", _S1L, 6)
    # The beep, once.
    + [_s1("beep.file", _S1B + "SetFunctionValue (GetSoundSourceFile rObj)", True),
       _s1("beep.before", _S1B + "SetFunctionValue (IsSoundPlaying rObj)"),
       _s1("beep.play", _S1B + "PlaySound rObj\nSetFunctionValue (IsSoundPlaying rObj)")]
    + _s1f("beep", _S1B, 12)
    + _s1w("beep.wait30.", 18) + [_s1("beep.f30", _S1B + "SetFunctionValue (IsSoundPlaying rObj)")]
    + _s1w("beep.wait60.", 29) + [_s1("beep.f60", _S1B + "SetFunctionValue (IsSoundPlaying rObj)")]
    + _s1w("beep.wait120.", 59) + [_s1("beep.f120", _S1B + "SetFunctionValue (IsSoundPlaying rObj)")]
    + _s1players("beep", _S1B)
    + [_s1("beep.stop", _S1B + "StopSound rObj\nSetFunctionValue (IsSoundPlaying rObj)")]
    + _s1f("beep.stopped", _S1B, 6)
    # The beep on the player.
    + [_s1("p3d.play", _S1B + "rPlayer.PlaySound3D rObj\nSetFunctionValue (IsSoundPlaying rObj rPlayer)")]
    + _s1f("p3d", _S1B, 6, " rPlayer")
    + [_s1("p3d.any", _S1B + "SetFunctionValue (IsSoundPlaying rObj)"),
       _s1("p3d.other", _S1B + _form("rOther", NF["doc"]) + "SetFunctionValue (IsSoundPlaying rObj rOther)"),
       _s1("p3d.path", _S1B + 'SetFunctionValue (IsSoundPlayingFromPath "fx\\emt\\medicalbeep\\emtmedicalbeep01.wav")'),
       _s1("p3d.path.data", _S1B + 'SetFunctionValue (IsSoundPlayingFromPath "Data\\Sound\\fx\\emt\\medicalbeep\\emtmedicalbeep01.wav")'),
       _s1("p3d.path.player", _S1B + 'SetFunctionValue (IsSoundPlayingFromPath "Data\\Sound\\fx\\emt\\medicalbeep\\emtmedicalbeep01.wav" rPlayer)')]
    + _s1players("p3d", _S1B)
    + [_s1("p3d.stopalt", _S1B + "SetFunctionValue (StopSoundAlt rObj rPlayer)")]
    + _s1f("p3d.stopped", _S1B, 6, " rPlayer")
    + [_s1("p3d.stopalt.again", _S1B + "SetFunctionValue (StopSoundAlt rObj rPlayer)"),
       _s1("p3d.stop", _S1B + "StopSound rObj\nSetFunctionValue (IsSoundPlaying rObj)")]
    + _s1f("p3d.stop.after", _S1B, 3)
)


# proj1 (writes, session only): a live projectile in flight. The player looks 60 degrees up
# and fires the 10mm pistol (FireWeapon); the projectile is found by walking GetFirstRef 51 3 /
# GetNextRef (the way the SetProjectileRefSpeedMult page gives [documented]) for the one whose
# source is the player, kept in an aux variable, and read frame by frame: speed multiplier,
# life time, distance travelled, source, weapon, damage. Then SetProjectileRefSpeedMult 0.5,
# SetProjectileRefDamage 5 and SetProjectileRefSource to a placed NVProspectorFemaleT, read in
# the same frame and the frames after; 30 frames later a walk counts what is left. Each read checks the projectile is still there (IsFormValid);
# the view angle is put back and the NPC disabled and deleted.
def _p1j(pid, body, text=False):
    return (f"proj1.{pid}", "ref rP\nref rGun\nref rProj\nref rNpc\nfloat fRes\nBegin Function { }\n"
            + _form("rP", NF["player"]) + _form("rGun", NF["pistol10"])
            + 'let rProj := rP.AuxVarGetRef "*nvfPR"\nlet rNpc := rP.AuxVarGetRef "*nvfPN"\n' + body, text, None, "")


def _p1jv(pid, expr):
    """A read of the kept projectile: -99 when it is gone."""
    return _p1j(pid, f"let fRes := -99\nif IsFormValid rProj\nlet fRes := {expr}\nendif\nSetFunctionValue fRes")


_P1WALK = ("let fRes := 0\nlet rProj := GetFirstRef 51 3\nwhile rProj\n"
           "if (rProj.GetProjectileRefSource) == rP\nlet fRes += 1\nrP.AuxVarSetRef \"*nvfPR\" rProj\nendif\n"
           "let rProj := GetNextRef\nloop\n")


def _p1reads(tag):
    return [_p1jv(f"{tag}.exists", "1"),
            _p1jv(f"{tag}.speedmult", "rProj.GetProjectileRefSpeedMult"),
            _p1jv(f"{tag}.lifetime.pos", "(rProj.GetProjectileRefLifeTime) > 0"),
            _p1jv(f"{tag}.distance.pos", "(rProj.GetProjectileRefDistanceTraveled) > 0"),
            _p1jv(f"{tag}.source.player", "(rProj.GetProjectileRefSource) == rP"),
            _p1jv(f"{tag}.source.npc", "(rProj.GetProjectileRefSource) == rNpc"),
            _p1jv(f"{tag}.weapon.pistol", "(rProj.GetProjectileRefWeapon) == rGun"),
            _p1jv(f"{tag}.damage", "rProj.GetProjectileRefDamage"),
            _p1jv(f"{tag}.type", "GetType (rProj.GetBaseObject)")]


PROJ1_PROBES = (
    [_p1j("npc.place", _form("rNpc", FX["prospector"]) + 'let rNpc := rP.PlaceAtMe rNpc 1\nrP.AuxVarSetRef "*nvfPN" rNpc\n'
          'let fRes := rP.GetAngle X\nrP.AuxVarSetFlt "*nvfPA" fRes\nSetFunctionValue 1')]
    + [_p1j(f"npc.wait{i}", "SetFunctionValue 1") for i in range(10)]
    + [_p1j("npc.move", "rNpc.SetPos Y ((rP.GetPos Y) - 600)\nSetFunctionValue 1"),
       _p1j("aim", "rP.SetAngle X -60\nSetFunctionValue 1")]
    + [_p1j(f"aim.wait{i}", "SetFunctionValue 1") for i in range(5)]
    + [_p1j("fire.walk", 'rP.AuxVarErase "*nvfPR"\nrP.FireWeapon rGun\n' + _P1WALK + "SetFunctionValue fRes"),
       _p1j("f1.walk", _P1WALK + "SetFunctionValue fRes")]
    + _p1reads("f1")
    + [_p1jv("set.speedmult", "rProj.SetProjectileRefSpeedMult 0.5"),
       _p1jv("set.speedmult.read", "rProj.GetProjectileRefSpeedMult"),
       _p1jv("set.damage", "rProj.SetProjectileRefDamage 5"),
       _p1jv("set.damage.read", "rProj.GetProjectileRefDamage"),
       _p1j("set.source", "let fRes := -99\nif IsFormValid rProj\nrProj.SetProjectileRefSource rNpc\n"
            "let fRes := (rProj.GetProjectileRefSource) == rNpc\nendif\nSetFunctionValue fRes")]
    + _p1reads("after")
    + [_p1j(f"late.wait{i}", "SetFunctionValue 1") for i in range(30)]
    + [_p1j("late.walk", _P1WALK + "SetFunctionValue fRes"),
       _p1jv("late.exists", "1")]
    + [_p1j("cleanup", 'rP.SetAngle X (rP.AuxVarGetFlt "*nvfPA")\nrNpc.Disable\nrNpc.MarkForDelete\n'
            'rP.AuxVarErase "*nvfPR"\nrP.AuxVarErase "*nvfPN"\nrP.AuxVarErase "*nvfPA"\nSetFunctionValue 1')]
)


# controls1 (writes, session only): GetControl for controls 0-16 on the keyboard (type 0) and
# Attack/Block on the mouse (type 1): these are this install's bindings, not defaults, so they
# can differ on another machine. SetControl 7 to scancode 48 and back in consecutive frames
# (the page says changes reach the .ini on exit [documented]; the capture ends the game by
# killing it, and the binding is put back first either way). HoldControl 0 (Forward) for 30
# frames: IsControlPressed while held, whether the player moved more than 10 units, then
# ReleaseControl: whether the player moved more than 10 units in the 10 frames after, and
# more than 1 unit in one frame 30 frames after; the position is put back. The 10mm pistol
# equipped, SetWeaponOut 1 read at 10, 30, 60 and 90 frames (gaps3 read 0 after 5), then
# HolsterWeapon read at 5 to 120 frames, then SetWeaponOut 0 the same; the pistol is removed.
def _c1(pid, body, text=False):
    return (f"controls1.{pid}", "ref rP\nref rObj\nfloat fRes\nfloat fDx\nBegin Function { }\n" + _form("rP", NF["player"]) + body,
            text, None, "")


def _c1v(pid, expr):
    return _c1(pid, f"SetFunctionValue ({expr})")


def _c1w(pid, n):
    return [_c1(f"{pid}{i}", "SetFunctionValue 1") for i in range(n)]


# The squared distance from the saved position.
_C1MOVED = ('let fDx := (rP.GetPos X) - (rP.AuxVarGetFlt "*nvfCX")\nlet fRes := (rP.GetPos Y) - (rP.AuxVarGetFlt "*nvfCY")\n'
            "let fRes := (fDx * fDx) + (fRes * fRes)\n")
# The same from where the player was at the release.
_C1REL = _C1MOVED.replace("*nvfCX", "*nvfRX").replace("*nvfCY", "*nvfRY")


def _c1checks(tag, ats=(10, 30, 60, 90)):
    out, done = [], 0
    for at in ats:
        out += _c1w(f"{tag}.wait{at}.", at - done - 1) + [_c1v(f"{tag}.f{at}", "rP.IsWeaponOut")]
        done = at
    return out


CONTROLS1_PROBES = (
    [_c1v(f"get.key.{c}", f"GetControl {c} 0") for c in range(17)]
    + [_c1v(f"get.mouse.{c}", f"GetControl {c} 1") for c in (4, 6)]
    + [_c1("set.key7", 'let fRes := GetControl 7 0\nrP.AuxVarSetFlt "*nvfCK" fRes\nSetControl 7 48 0\nSetFunctionValue (GetControl 7 0)'),
       _c1v("set.key7.next", "GetControl 7 0"),
       _c1("set.key7.restore", 'SetControl 7 (rP.AuxVarGetFlt "*nvfCK") 0\nrP.AuxVarErase "*nvfCK"\nSetFunctionValue (GetControl 7 0)')]
    # Forward, held.
    + [_c1("hold.save", 'let fRes := rP.GetPos X\nrP.AuxVarSetFlt "*nvfCX" fRes\nlet fRes := rP.GetPos Y\nrP.AuxVarSetFlt "*nvfCY" fRes\n'
           "HoldControl 0\nSetFunctionValue (IsControlPressed 0)")]
    + _c1w("hold.wait", 2)
    + [_c1v("hold.pressed", "IsControlPressed 0"),
       _c1v("hold.pressed.ignore", "IsControlPressed 0 1")]
    + _c1w("hold.walk", 26)
    + [_c1("hold.moved", _C1MOVED + "SetFunctionValue (fRes > 100)"),
       _c1("release", 'let fRes := rP.GetPos X\nrP.AuxVarSetFlt "*nvfRX" fRes\nlet fRes := rP.GetPos Y\nrP.AuxVarSetFlt "*nvfRY" fRes\n'
           "ReleaseControl 0\nSetFunctionValue (IsControlPressed 0)")]
    + _c1w("release.wait", 10)
    + [_c1v("release.pressed", "IsControlPressed 0"),
       _c1("release.moved", _C1REL + "SetFunctionValue (fRes > 100)")]
    + _c1w("release.settle", 20)
    + [_c1("release.still", 'let fRes := rP.GetPos X\nrP.AuxVarSetFlt "*nvfRX" fRes\nlet fRes := rP.GetPos Y\nrP.AuxVarSetFlt "*nvfRY" fRes\nSetFunctionValue 1'),
       _c1("release.still.moved", _C1REL + "SetFunctionValue (fRes > 1)"),
       _c1("hold.restore", 'rP.SetPos X (rP.AuxVarGetFlt "*nvfCX")\nrP.SetPos Y (rP.AuxVarGetFlt "*nvfCY")\n'
           'rP.AuxVarErase "*nvfCX"\nrP.AuxVarErase "*nvfCY"\nrP.AuxVarErase "*nvfRX"\nrP.AuxVarErase "*nvfRY"\nSetFunctionValue 1')]
    # SetWeaponOut and HolsterWeapon.
    + [_c1("weap.add", _form("rObj", NF["pistol10"]) + "rP.AddItem rObj 1 1\nrP.EquipItem rObj 0 1\nSetFunctionValue 1")]
    + _c1w("weap.addwait", 10)
    + [_c1v("weap.before", "rP.IsWeaponOut"),
       _c1("weap.out", "rP.SetWeaponOut 1\nSetFunctionValue (rP.IsWeaponOut)")]
    + _c1checks("weap.out")
    + [_c1("weap.holster", "rP.HolsterWeapon\nSetFunctionValue (rP.IsWeaponOut)")]
    + _c1checks("weap.holster", (5, 10, 15, 20, 30, 60, 120))
    + [_c1("weap.setout0", "rP.SetWeaponOut 0\nSetFunctionValue (rP.IsWeaponOut)")]
    + _c1checks("weap.setout0", (10, 30, 60, 120))
    + [_c1("weap.cleanup", _form("rObj", NF["pistol10"]) + "rP.UnequipItem rObj 0 1\nrP.RemoveItem rObj 1 1\nSetFunctionValue 1")]
)


# hud1 (writes, session only): HUD reads that need no menu. GetHUDVisibilityOverride and
# SetHUDVisibilityOverride (JohnnyGuitar; flag bits from the wiki page [documented]) read back
# and put back; GetHUDShudderPower; GetReticlePos (JIP) size, and GetReticleTargetLimb with
# nothing aimed at. Corner messages: GetNumQueuedCornerMessages (ShowOff) before, after three
# MessageEx calls, and after ClearMessageQueue (JIP). Compass: GetNumCompassHostiles,
# GetCompassTargets 0/1/2 sizes and GetNearestCompassHostile before and while a placed
# NVProspectorFemaleT fights the player (god mode on, LU Console "tgm", and off after); the
# NPC is disabled and deleted.
def _h1(pid, body, text=False):
    return (f"hud1.{pid}", "ref rP\nref rNpc\nref rObj\nfloat fRes\narray_var aArr\nBegin Function { }\n" + _form("rP", NF["player"])
            + 'let rNpc := rP.AuxVarGetRef "*nvfH1"\n' + body, text, None, "")


def _h1v(pid, expr):
    return _h1(pid, f"SetFunctionValue ({expr})")


def _h1w(pid, n):
    return [_h1(f"{pid}{i}", "SetFunctionValue 1") for i in range(n)]


def _h1compass(tag):
    return ([_h1v(f"{tag}.hostiles.num", "GetNumCompassHostiles"),
             _h1v(f"{tag}.hostiles.num.near", "GetNumCompassHostiles 100"),
             _h1("%s.hostiles.size" % tag, "let aArr := GetCompassHostiles\nSetFunctionValue (ar_Size aArr)")]
            + [_h1(f"{tag}.targets.{w}.size", f"let aArr := GetCompassTargets {w}\nSetFunctionValue (ar_Size aArr)") for w in (0, 1, 2)]
            + [_h1(f"{tag}.nearest.npc", "let rObj := GetNearestCompassHostile\nSetFunctionValue (rObj == rNpc)"),
               _h1(f"{tag}.nearest.valid", "let rObj := GetNearestCompassHostile\nSetFunctionValue (IsFormValid rObj)")])


HUD1_PROBES = (
    [_h1("vis.get", 'let fRes := GetHUDVisibilityOverride\nrP.AuxVarSetFlt "*nvfHV" fRes\nSetFunctionValue fRes'),
     _h1("vis.set", "SetHUDVisibilityOverride 3\nSetFunctionValue (GetHUDVisibilityOverride)"),
     _h1v("vis.set.next", "GetHUDVisibilityOverride"),
     _h1("vis.restore", 'SetHUDVisibilityOverride (rP.AuxVarGetFlt "*nvfHV")\nrP.AuxVarErase "*nvfHV"\nSetFunctionValue (GetHUDVisibilityOverride)'),
     _h1v("shudder", "GetHUDShudderPower"),
     _h1("reticle.size", "let aArr := GetReticlePos\nSetFunctionValue (ar_Size aArr)"),
     _h1v("reticle.limb", "GetReticleTargetLimb"),
     _h1v("corner.before", "GetNumQueuedCornerMessages"),
     _h1("corner.add", 'MessageEx "nvf one"\nMessageEx "nvf two"\nMessageEx "nvf three"\nSetFunctionValue (GetNumQueuedCornerMessages)'),
     _h1v("corner.add.next", "GetNumQueuedCornerMessages"),
     _h1("corner.clear", "ClearMessageQueue\nSetFunctionValue (GetNumQueuedCornerMessages)"),
     _h1v("corner.clear.next", "GetNumQueuedCornerMessages")]
    + _h1compass("calm")
    + [_h1("npc.place", _form("rObj", FX["prospector"]) + 'let rNpc := rP.PlaceAtMe rObj 1\nrP.AuxVarSetRef "*nvfH1" rNpc\nSetFunctionValue 1')]
    + _h1w("npc.wait", 10)
    + [_h1("npc.move", "rNpc.SetPos Y ((rP.GetPos Y) + 500)\nSetFunctionValue 1")]
    + _h1w("npc.movewait", 10)
    + _h1compass("npc.calm")
    + [_h1("fight.start", 'Console "tgm"\nrNpc.StartCombat rP\nSetFunctionValue 1')]
    + _h1w("fight.wait", 60)
    + [_h1v("fight.incombat", "rNpc.IsInCombat")]
    + _h1compass("fight")
    + [_h1("cleanup", 'rNpc.StopCombat\nrNpc.Disable\nrNpc.MarkForDelete\nConsole "tgm"\nrP.AuxVarErase "*nvfH1"\nSetFunctionValue 1')]
    + _h1w("cleanup.wait", 10)
    + _h1compass("after")
)


# showoff1 (writes, session only): the ShowOff functions the mod lists use that no set read
# (docs/handoff/modlist-function-counts.json). The event pages are not in the wiki dump, so
# each event gets two handlers, one taking no arguments and one taking two refs, and the
# rows say which ran and what they got [observed only]. ShowOff:OnAdd: the player is given
# a stimpak (15169); GetAddedItemRefShowOff inside the handler, and its base's type. ShowOff:OnExplosionHit: the
# player's Health is raised by 1000 and a MineFragExplosion (07078B) is placed at the
# player; GetExplosionHitDamage read, SetExplosionHitDamage 5 set, and the Health lost
# compared. SetOnAuxTimerStopHandler on the player's timer "*nvfST": started for 0.1 s and
# left to run out, then started again and stopped with AuxTimerStop 1 (fire the event).
# ForceWeaponJamAnim on the player with the 10mm pistol away, then drawn (SetWeaponOut 1,
# 40 frames), with IsAnimPlayingEx 4 kept over 60 frames after. Handlers are removed, Health,
# the weapon and the stimpak put back.
def _so1(pid, body, text=False):
    return (f"showoff1.{pid}", "ref rP\nref rObj\nfloat fRes\nBegin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


def _so1v(pid, expr):
    return _so1(pid, f"SetFunctionValue ({expr})")


def _so1w(pid, n):
    return [_so1(f"{pid}{i}", "SetFunctionValue 1") for i in range(n)]


def _so1aux(tag, names):
    return [_so1v(f"{tag}.{n}.size", f'rP.AuxVarSize "*nvfS1.{n}"') for n in names] + [
        _so1v(f"{tag}.{n}.0", f'rP.AuxVarGetFlt "*nvfS1.{n}" 0') for n in names]


_SO1_ADD0 = ("add0.count", "add0.isref", "add0.base.type")
_SO1_ADD2 = ("add2.count", "add2.a.type", "add2.b.player")
_SO1_EXP0 = ("exp0.count", "exp0.damage")
_SO1_EXP2 = ("exp2.count", "exp2.a.player", "exp2.b.isref", "exp2.b.type")
_SO1_TIM = ("tim.count", "tim.owner.player")
_SO1_ALL = _SO1_ADD0 + _SO1_ADD2 + _SO1_EXP0 + _SO1_EXP2 + _SO1_TIM


def _so1set(n, expr):
    return f'  rP.AuxVarSetFlt "*nvfS1.{n}" ({expr}) -1\n'


SHOWOFF1_PROBES = (
    # OnAdd.
    [_so1("add.reg", 'SetEventHandler "ShowOff:OnAdd" (Begin Function { }\n  let rObj := GetAddedItemRefShowOff\n'
          + _so1set("add0.count", "1") + _so1set("add0.isref", "IsReference rObj") + _so1set("add0.base.type", "GetType (rObj.GetBaseObject)")
          + 'End)\nSetEventHandler "ShowOff:OnAdd" (Begin Function {ref rA, ref rB}\n'
          + _so1set("add2.count", "1") + _so1set("add2.a.type", "GetType rA") + _so1set("add2.b.player", "rB == rP")
          + "End)\nSetFunctionValue 1"),
     _so1("add.give", _form("rObj", NF["stimpak"]) + "rP.AddItem rObj 1 1\nSetFunctionValue 1")]
    + _so1w("add.wait", 5)
    + _so1aux("add", _SO1_ADD0 + _SO1_ADD2)
    # OnExplosionHit.
    + [_so1("exp.reg", 'SetEventHandler "ShowOff:OnExplosionHit" (Begin Function { }\n'
            + _so1set("exp0.count", "1") + _so1set("exp0.damage", "GetExplosionHitDamage") + "  SetExplosionHitDamage 5\n"
            + 'End)\nSetEventHandler "ShowOff:OnExplosionHit" (Begin Function {ref rA, ref rB}\n'
            + _so1set("exp2.count", "1") + _so1set("exp2.a.player", "rA == rP") + _so1set("exp2.b.isref", "IsReference rB") + _so1set("exp2.b.type", "GetType rB")
            + "End)\nSetFunctionValue 1"),
       _so1("exp.boost", 'rP.ModAV Health 1000\nlet fRes := rP.GetAV Health\nrP.AuxVarSetFlt "*nvfS1H" fRes\nSetFunctionValue 1'),
       _so1("exp.boom", _form("rObj", "7078B") + "rP.PlaceAtMe rObj 1\nSetFunctionValue 1")]
    + _so1w("exp.wait", 10)
    + _so1aux("exp", _SO1_EXP0 + _SO1_EXP2)
    + [_so1v("exp.health.lost", '(rP.AuxVarGetFlt "*nvfS1H") - (rP.GetAV Health)'),
       _so1("exp.restore", 'rP.ModAV Health -1000\nrP.RestoreAV Health 2000\nrP.AuxVarErase "*nvfS1H"\nSetFunctionValue 1')]
    # Aux timer stops.
    + [_so1("tim.reg", 'SetOnAuxTimerStopHandler 1 (Begin Function {string_var sName, ref rOwner}\n'
            + _so1set("tim.count", "1") + _so1set("tim.owner.player", "rOwner == rP") + 'End) "*nvfST" rP\n'
            'rP.AuxTimerStart "*nvfST" 0.1\nSetFunctionValue 1')]
    + _so1w("tim.wait", 30)
    + [_so1v("tim.expired.count", 'rP.AuxVarSize "*nvfS1.tim.count"'),
       _so1v("tim.expired.left", 'rP.AuxTimerTimeLeft "*nvfST"'),
       _so1("tim.restart", 'rP.AuxTimerStart "*nvfST" 30\nSetFunctionValue (rP.AuxTimerStop "*nvfST" 1)')]
    + _so1w("tim.stopwait", 3)
    + [_so1v("tim.stopped.count", 'rP.AuxVarSize "*nvfS1.tim.count"'),
       _so1v("tim.owner.0", 'rP.AuxVarGetFlt "*nvfS1.tim.owner.player" 0')]
    # The jam animation.
    + [_so1("jam.add", _form("rObj", NF["pistol10"]) + "rP.AddItem rObj 1 1\nrP.EquipItem rObj 0 1\nSetFunctionValue 1")]
    + _so1w("jam.addwait", 10)
    + [_so1v("jam.away", "rP.ForceWeaponJamAnim"),
       _so1("jam.draw", "rP.SetWeaponOut 1\nSetFunctionValue 1")]
    + _so1w("jam.drawwait", 40)
    + [_so1v("jam.drawn", "rP.IsWeaponOut"),
       _so1("jam.force", 'rP.AuxVarSetFlt "*nvfS1J" 0\nSetFunctionValue (rP.ForceWeaponJamAnim)')]
    + [_so1(f"jam.f{i}", 'if (rP.IsAnimPlayingEx 4 0 0)\nrP.AuxVarSetFlt "*nvfS1J" 1\nendif\nSetFunctionValue 1') for i in range(60)]
    + [_so1v("jam.reload.ever", 'rP.AuxVarGetFlt "*nvfS1J"'),
       _so1("jam.cleanup", _form("rObj", NF["pistol10"]) + "rP.SetWeaponOut 0\nrP.UnequipItem rObj 0 1\nrP.RemoveItem rObj 1 1\n"
            + _form("rObj", NF["stimpak"]) + "rP.RemoveItem rObj 1 1\n" + 'rP.AuxVarErase "*nvfS1J"\n'
            + "".join(f'rP.AuxVarErase "*nvfS1.{n}"\n' for n in _SO1_ALL) + "SetFunctionValue 1")]
)


# atk1 (writes, session only): GetAttackDamage on a creature base (VStreetCrBrahmin2, CREA
# 17B587; the page says it works on weapons, water and creatures [documented]),
# SetAttackDamage 50 read back and put back; SetWeaponSightFOV 40 and SetWeaponAmmo on the
# 10mm pistol (434F) read back and put back.
def _at1(pid, body, text=False):
    return (f"atk1.{pid}", "ref rP\nref rObj\nref rOther\nfloat fRes\nBegin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


_AT1C = _form("rObj", "17B587")
_AT1W = _form("rObj", NF["pistol10"])

ATK1_PROBES = (
    [_at1("crea.get", _AT1C + "SetFunctionValue (GetAttackDamage rObj)"),
     _at1("crea.set", _AT1C + 'let fRes := GetAttackDamage rObj\nrP.AuxVarSetFlt "*nvfAD" fRes\nSetAttackDamage 50 rObj\n'
          "SetFunctionValue (GetAttackDamage rObj)"),
     _at1("crea.set.next", _AT1C + "SetFunctionValue (GetAttackDamage rObj)"),
     _at1("crea.restore", _AT1C + 'SetAttackDamage (rP.AuxVarGetFlt "*nvfAD") rObj\nrP.AuxVarErase "*nvfAD"\nSetFunctionValue (GetAttackDamage rObj)'),
     _at1("fov.get", _AT1W + "SetFunctionValue (GetWeaponSightFOV rObj)"),
     _at1("fov.set", _AT1W + 'let fRes := GetWeaponSightFOV rObj\nrP.AuxVarSetFlt "*nvfAF" fRes\nSetWeaponSightFOV 40 rObj\n'
          "SetFunctionValue (GetWeaponSightFOV rObj)"),
     _at1("fov.restore", _AT1W + 'SetWeaponSightFOV (rP.AuxVarGetFlt "*nvfAF") rObj\nrP.AuxVarErase "*nvfAF"\nSetFunctionValue (GetWeaponSightFOV rObj)'),
     _at1("ammo.get", _AT1W + "SetFunctionValue (GetFormIDString (GetWeaponAmmo rObj))", True),
     _at1("ammo.set", _AT1W + _form("rOther", NF["ammo10"]) + "let rOther := GetWeaponAmmo rObj\nrP.AuxVarSetRef \"*nvfAA\" rOther\n"
          + _form("rOther", "4240") + "SetWeaponAmmo rOther rObj\nSetFunctionValue (GetFormIDString (GetWeaponAmmo rObj))", True),
     _at1("ammo.restore", _AT1W + 'let rOther := rP.AuxVarGetRef "*nvfAA"\nSetWeaponAmmo rOther rObj\nrP.AuxVarErase "*nvfAA"\n'
          "SetFunctionValue (GetFormIDString (GetWeaponAmmo rObj))", True)]
)


# jip4 (writes, session only; docs/spec-notes/jip-traits-4.md): the record traits and
# game-state getters of JIP batch 4 on the wiki pages' example forms (FalloutNV.esm IDs from
# fnvtool esm inventory [observed]): every trait and flag ID of NCRRanger (CLAS 8D424),
# FollowersCombatStyleRanged (CSTY C80B6), BasementLightFillWarm (LIGH 695BE),
# GrassWasteland01 (GRAS 17753), 1stPersonSniperRifleTexture (TXST 782D2), LandscapeDirtNV
# (TXST 11673A), WastelandClimate (CLMT 17907), EMTMedicalBeep01 (SOUN 174325); the quest
# flags of five quests; cell and water forms; then SetClimateTraitNumeric 6 (does it keep the
# moons?), SetQuestFlag 5 and SetGlobalRef on GameYear (is the number kept?), each put back;
# GetStartingPos/GetStartingAngle on the player, Doc Mitchell and his door. GetEditorPosition
# is not here: no form of the call compiled at run time (six tried, 2026-10-30), and the
# GetDelayElapsed values change every frame (they are in docs/spec-notes/jip-traits-4.md).
J4 = {"class": "8D424", "csty": "C80B6", "light": "695BE", "grass": "17753", "sniper": "782D2",
      "dirt": "11673A", "climate": "17907", "beep": "174325", "vcg01": "104C1C", "vcg02": "10A214",
      "vms01": "80664", "vms20": "10E908", "deadsea": "125E7E", "ncr": "A46E7", "gomorrah": "10D629",
      "gasstation": "109038", "water": "1009CA", "gameyear": "35"}


def _j4(pid, body, text=False):
    return (f"jip4.{pid}", "ref rP\nref rObj\nref rOther\nfloat fRes\nfloat fX\nfloat fY\nfloat fZ\n"
            "float fA\nfloat fB\nfloat fC\nBegin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


def _j4v(pid, key, expr, text=False):
    """fRes (or a form ID's text) of expr with rObj = the J4 form `key`."""
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return _j4(pid, _form("rObj", J4[key]) + rd, bool(text))


_J4_QUESTS = ("vcg01", "vcg02", "vms01", "vms20", "deadsea")
_J4_REFS = (("player", None), ("doc", NF["docref"]), ("door", AF["door"]))
JIP4_PROBES = (
    [_j4v(f"class.trait.{n}", "class", f"GetClassTraitNumeric rObj {n}") for n in range(12)]
    + [_j4v(f"class.flag.{n}", "class", f"GetClassFlag rObj {n}") for n in range(20)]
    + [_j4v(f"csty.trait.{n}", "csty", f"GetCombatStyleTraitNumeric rObj {n}") for n in range(64)]
    + [_j4v(f"csty.flag.{n}", "csty", f"GetCombatStyleFlag rObj {n}") for n in range(9)]
    + [_j4v(f"light.trait.{n}", "light", f"GetLightTraitNumeric rObj {n}") for n in range(7)]
    + [_j4v(f"light.flag.{n}", "light", f"GetLightFlag rObj {n}") for n in range(32)]
    + [_j4v(f"grass.trait.{n}", "grass", f"GetGrassTraitNumeric rObj {n}") for n in range(10)]
    + [_j4v("grass.model", "grass", "GetGrassModel rObj", True)]
    + [_j4v(f"txst.{k}.{n}", k, f"GetTextureSetTexture rObj {n}", True) for k in ("sniper", "dirt") for n in range(6)]
    + [_j4v(f"climate.trait.{n}", "climate", f"GetClimateTraitNumeric rObj {n}") for n in range(7)]
    + [_j4("climate.set6", _form("rObj", J4["climate"]) + 'let fRes := GetClimateTraitNumeric rObj 6\nrP.AuxVarSetFlt "*nvfJ4C" fRes\n'
           "SetClimateTraitNumeric rObj 6 7\nSetFunctionValue (GetClimateTraitNumeric rObj 6)"),
       _j4v("climate.set6.moons", "climate", "GetClimateTraitNumeric rObj 5"),
       _j4("climate.set6.restore", _form("rObj", J4["climate"]) + 'SetClimateTraitNumeric rObj 6 (rP.AuxVarGetFlt "*nvfJ4C")\n'
           'rP.AuxVarErase "*nvfJ4C"\nSetFunctionValue (GetClimateTraitNumeric rObj 6)')]
    + [_j4v(f"quest.{q}.{n}", q, f"GetQuestFlag rObj {n}") for q in _J4_QUESTS for n in range(8)]
    + [_j4("quest.deadsea.set5", _form("rObj", J4["deadsea"]) + "SetQuestFlag rObj 5 1\nSetFunctionValue (GetQuestFlag rObj 5)"),
       _j4v("quest.deadsea.set5.running", "deadsea", "GetQuestFlag rObj 0"),
       _j4("quest.deadsea.set5.restore", _form("rObj", J4["deadsea"]) + "SetQuestFlag rObj 5 0\nSetFunctionValue (GetQuestFlag rObj 5)")]
    + [_j4v("faction.reptype", "ncr", "GetFactionReputationType rObj", "form")]
    + [_j4v(f"cell.{c}.{f}", c, f"GetCell{F} rObj", "form")
       for c in ("gomorrah", "gasstation") for f, F in (("climate", "Climate"), ("ltmp", "LightingTemplate"))]
    + [_j4v(f"cell.{c}.noise", c, "GetCellNoiseTexture rObj", True) for c in ("gomorrah", "gasstation")]
    + [_j4v("water.sound", "water", "GetWaterSound rObj", "form"),
       _j4v("water.noise", "water", "GetWaterNoiseTexture rObj", True)]
    + [_j4v(f"sound.flag.{n}", "beep", f"GetSoundFlag rObj {n}") for n in range(14)]
    + [_j4("difficulty", "SetFunctionValue (GetGameDifficulty)"),
       _j4("sessiontime.positive", "SetFunctionValue ((GetSessionTime) > 0)"),
       _j4("sessiontime.order", "let fRes := GetSessionTime\nSetFunctionValue ((GetSessionTime) >= fRes)"),
       _j4("pcdetection", "SetFunctionValue (GetPCDetectionState)"),
       _j4("poisoned.player", "SetFunctionValue (rP.GetIsPoisoned)")]
    + [_j4v(f"delay.{q}.le5", q, "(GetDelayElapsed rObj) <= 5") for q in ("vcg01", "deadsea")]
    + [_j4v("globalref.before.number", "gameyear", "GetGlobalValue GameYear")]
    + [_j4(f"reftype.{k}", ("" if fid is None else _form("rP", fid)) + "SetFunctionValue (rP.GetRefType)") for k, fid in _J4_REFS]
    + [_j4(f"reftype.{k}.base", ("" if fid is None else _form("rP", fid)) + "SetFunctionValue (GetType (rP.GetBaseObject))") for k, fid in _J4_REFS]
    + [_j4("teleportdoor.door", _form("rObj", AF["door"]) + "SetFunctionValue (GetFormIDString (rObj.GetTeleportDoor))", True),
       _j4("teleportdoor.linked", _form("rObj", AF["door"]) + "SetFunctionValue (GetFormIDString (rObj.GetLinkedDoor))", True)]
    + [_j4v("globalref.read", "gameyear", "GetGlobalRef rObj", "form"),
       _j4("globalref.set", _form("rObj", J4["gameyear"]) + _form("rOther", NF["docref"])
           + "SetGlobalRef rObj rOther\nSetFunctionValue (GetFormIDString (GetGlobalRef rObj))", True),
       _j4("globalref.set.number", "SetFunctionValue (GetGlobalValue GameYear)"),
       # Put back with the value globalref.before.number reads (a value carried in an
       # auxiliary variable came back 0 in the first runs).
       _j4("globalref.restore", "let fRes := SetGlobalValue GameYear 2281\nSetFunctionValue (GetGlobalValue GameYear)")]
    + [_j4(f"start{fn[-3:].lower()}.{k}.{a.lower()}", ("" if fid is None else _form("rP", fid)) + f"SetFunctionValue (Floor (({fn} {a}){m}))")
       for k, fid in _J4_REFS
       for fn, m, a in (("rP.GetStartingPos", "", "X"), ("rP.GetStartingPos", "", "Y"), ("rP.GetStartingPos", "", "Z"),
                        ("rP.GetStartingAngle", " * 1000", "X"), ("rP.GetStartingAngle", " * 1000", "Y"),
                        ("rP.GetStartingAngle", " * 1000", "Z"))]
)


# rules1 (writes, session only; issues #1021 and #1025): rules no page documents.
# FATan/FATan2 to eight decimals over a range (jip-getters read 44.9998 for 45 at six
# digits); GetRunSpeed with the speed multiplier and Agility beside it (gaps4 read 338 for
# the player); the references of Doc Mitchell's house with and without taken ones, as form
# IDs (nvse-refs counted 21 and 22); and SetNthPackage, whose page gives no arguments, in
# three likely forms, each read back with GetNthPackage (a form the game refuses to compile
# leaves no row).
def _r1(pid, body, text=True, prefix="rules1"):
    return (f"{prefix}.{pid}", "ref rP\nref rDoc\nref rObj\nref rPkg\nref rF\nfloat fRes\nint iN\n"
            "string_var sText\narray_var aA\narray_var aE\nBegin Function { }\n"
            + _form("rP", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


_R1_ATAN = (1, 0.5, 2, 0.25, 4, 10, 0.1, -1, 0)
_R1_ATAN2 = ((1, 1), (1, -1), (-1, -1), (-1, 1), (3, 4), (4, 3), (0, 1), (1, 0), (1, 2))
_R1_REFS = ('let aA := GetRefsInCell rF 0 0 {taken}\nlet sText := sv_Construct "%g:" (ar_Size aA)\n'
            'foreach aE <- aA\n  let rObj := aE["value"]\n'
            '  let sText := sText + " " + (GetFormIDString rObj)\nloop\nSetFunctionValue sText')
_R1_PKG = 'let rPkg := rDoc.GetNthPackage 1\nlet rObj := rDoc.GetNthPackage 0\n'
_R1_PKG_READ = 'SetFunctionValue (sv_Construct "%i %i %i" rObj rPkg (rDoc.GetNthPackage 0))'
RULES1_PROBES = (
    [_r1(f"fatan.{i}", f'SetFunctionValue (sv_Construct "%.8f" (fatan {x}))') for i, x in enumerate(_R1_ATAN)]
    + [_r1(f"fatan2.{i}", f'SetFunctionValue (sv_Construct "%.8f" (fatan2 {y} {x}))') for i, (y, x) in enumerate(_R1_ATAN2)]
    + [_r1("runspeed.player", 'SetFunctionValue (sv_Construct "%.4f" (rP.GetRunSpeed))'),
       _r1("runspeed.doc", 'SetFunctionValue (sv_Construct "%.4f" (rDoc.GetRunSpeed))'),
       _r1("speedmult.player", 'SetFunctionValue (sv_Construct "%.4f" (rP.GetAV SpeedMult))'),
       _r1("speedmult.doc", 'SetFunctionValue (sv_Construct "%.4f" (rDoc.GetAV SpeedMult))'),
       _r1("agility.player", 'SetFunctionValue (sv_Construct "%.4f" (rP.GetAV Agility))'),
       _r1("agility.doc", 'SetFunctionValue (sv_Construct "%.4f" (rDoc.GetAV Agility))'),
       _r1("encumbrance.player", 'SetFunctionValue (sv_Construct "%.4f %.4f" (rP.GetAV InventoryWeight) (rP.GetAV CarryWeight))')]
    + [_r1(f"refs.doc.{t}", _form("rF", CELLS["doc"]) + _R1_REFS.format(taken=t)) for t in (0, 1)]
    + [_r1("setnthpackage.ref.pkg.index", _R1_PKG + "rDoc.SetNthPackage rPkg 0\n" + _R1_PKG_READ),
       _r1("setnthpackage.ref.index.pkg", _R1_PKG + "rDoc.SetNthPackage 0 rPkg\n" + _R1_PKG_READ),
       _r1("setnthpackage.pkg.index.base", _R1_PKG + "SetNthPackage rPkg 0 (rDoc.GetBaseObject)\n" + _R1_PKG_READ),
       _r1("setnthpackage.result", _R1_PKG + "let fRes := rDoc.SetNthPackage rPkg 0\n"
           'SetFunctionValue (sv_Construct "%g" fRes)')]
)


# rules2 (writes, session only, each change put back; #1021, #1025): what rules1 left
# open. FATan at sixteenths of 1 and more (an approximation, rules1); GetRunSpeed while
# Speed Mult, Agility, fMoveRunMult and fMoveBaseSpeed change one at a time (rules1 read
# 338.8 = fMoveBaseSpeed 77 x 4.4 for the player and Doc); SetNthPackage in the one form
# that compiled in rules1 (`let fRes := ref.SetNthPackage package index`), read back.
_R2_X = [k / 16 for k in range(1, 17)] + [0.3, 0.7, 0.9, 0.05]
_R2_RUN = 'SetFunctionValue (sv_Construct "%.4f" (rP.GetRunSpeed))'
def _r2(pid, body, text=True):
    return _r1(pid, body, text, "rules2")


_R2_PKGS = ('let rObj := rDoc.GetNthPackage 0\nlet rF := rDoc.GetNthPackage 1\nlet fRes := rDoc.GetPackageCount\n'
            'SetFunctionValue (sv_Construct "%i %i %g" rObj rF fRes)')
RULES2_PROBES = (
    [_r2(f"fatan.x{i}", f'SetFunctionValue (sv_Construct "%.10f" (fatan {x:g}))') for i, x in enumerate(_R2_X)]
    + [_r2("run.base", _R2_RUN),
       _r2("run.speedmult50", 'rP.SetAV SpeedMult 50\n' + _R2_RUN),
       _r2("run.speedmult50.next", _R2_RUN),
       _r2("run.speedmult.restore", 'rP.SetAV SpeedMult 100\n' + _R2_RUN),
       _r2("run.agility10", 'rP.SetAV Agility 10\n' + _R2_RUN),
       _r2("run.agility.restore", 'rP.SetAV Agility 5\n' + _R2_RUN),
       _r2("run.runmult5", 'SetNumericGameSetting "fMoveRunMult" 5\n' + _R2_RUN),
       _r2("run.runmult.restore", 'SetNumericGameSetting "fMoveRunMult" 4\n' + _R2_RUN),
       _r2("run.base100", 'SetNumericGameSetting "fMoveBaseSpeed" 100\n' + _R2_RUN),
       _r2("run.base.restore", 'SetNumericGameSetting "fMoveBaseSpeed" 77\n' + _R2_RUN),
       _r2("run.sneak", 'SetFunctionValue (sv_Construct "%.4f %g" (rP.GetRunSpeed) (rP.IsSneaking))')]
    + [_r2("setnth.before", _R2_PKGS),
       _r2("setnth.set", _R1_PKG + 'let fRes := rDoc.SetNthPackage rPkg 0\n'
           + _R2_PKGS),
       _r2("setnth.restore", 'let rPkg := GetFormFromMod "FalloutNV.esm" "15ED62"\nlet fRes := rDoc.SetNthPackage rPkg 0\n'
           + _R2_PKGS),
       _r2("setnth.past", _R1_PKG + 'let fRes := rDoc.SetNthPackage rPkg 99\n'
           'SetFunctionValue (sv_Construct "%g" (rDoc.GetPackageCount))')]
)


# showoff2 (writes, session only, each change put back; #1010): the ShowOff functions of
# the 2026-10-30 batch (docs/spec-notes/ext-showoff.md §8) on the player, Doc Mitchell, the
# 10mm pistol, the nail board (the page's melee example), the combat knife, leather armor,
# a stimpak, 10mm ammo, the dog VCrTier1DogMed, Doc's house and two encounter zones.
SO2 = {"nailboard": "EB6B1", "knife": "4326", "dog": "14F425", "vault11zone": "E9081", "hooverzone": "131F33"}


def _so2(pid, body, text=False):
    return (f"showoff2.{pid}", "ref rP\nref rDoc\nref rObj\nref rOther\nfloat fRes\nstring_var sText\nBegin Function { }\n"
            + _form("rP", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _so2v(pid, expr, text=False):
    return _so2(pid, f"SetFunctionValue ({expr})", text)


def _so2f(pid, key, expr, text=False):
    fid = SO2.get(key) or NF.get(key) or CELLS.get(key)
    return _so2(pid, _form("rObj", fid) + f"SetFunctionValue ({expr})", text)


SHOWOFF2_PROBES = (
    [_so2v("equippedweapon.player", "GetFormIDString (rP.GetEquippedWeapon)", True),
     _so2v("equippedweapon.doc", "GetFormIDString (rDoc.GetEquippedWeapon)", True),
     _so2v("equippedmelee.player", "rP.IsEquippedWeaponMelee"),
     _so2v("equippedranged.player", "rP.IsEquippedWeaponRanged"),
     _so2v("equippedmelee.doc", "rDoc.IsEquippedWeaponMelee"),
     _so2v("equippedranged.doc", "rDoc.IsEquippedWeaponRanged"),
     _so2f("weaponmelee.nailboard", "nailboard", "IsWeaponMelee rObj"),
     _so2f("weaponmelee.knife", "knife", "IsWeaponMelee rObj"),
     _so2f("weaponmelee.pistol", "pistol10", "IsWeaponMelee rObj"),
     _so2f("canhealth.pistol", "pistol10", "GetItemCanHaveHealth rObj"),
     _so2f("canhealth.leather", "leather", "GetItemCanHaveHealth rObj"),
     _so2f("canhealth.stimpak", "stimpak", "GetItemCanHaveHealth rObj"),
     _so2v("limbs.mask", "rP.GetCrippledLimbsAsBitMask"),
     _so2v("limbs.num", "rP.GetNumCrippledLimbs"),
     _so2v("limbs.any", "rP.IsLimbCrippled"),
     _so2v("limbs.mask.100", "rP.GetCrippledLimbsAsBitMask 100"),
     _so2v("limbs.num.100", "rP.GetNumCrippledLimbs 100"),
     _so2v("limbs.head.100", "rP.IsLimbCrippled 0 100"),
     _so2(f"limbs.leftleg.cripple", 'rP.DamageAV LeftMobilityCondition 200\n'
          'SetFunctionValue (sv_Construct "%g %g %g %g" (rP.GetCrippledLimbsAsBitMask) (rP.GetNumCrippledLimbs) (rP.IsLimbCrippled 4) (rP.GetAVDamage LeftMobilityCondition))', True),
     _so2("limbs.leftleg.restore", 'rP.RestoreAV LeftMobilityCondition 200\nSetFunctionValue (rP.GetCrippledLimbsAsBitMask)'),
     _so2v("avdamage.health", "rP.GetActorValueDamage Health"),
     _so2v("avname.1", "GetActorValueName (1)", True),
     _so2v("avname.25", "GetActorValueName (25)", True),
     _so2v("avname.76", "GetActorValueName (76)", True),
     _so2f("ammoname.0", "ammo10", "GetAmmoName 0 rObj", True),
     _so2f("ammoname.1", "ammo10", "GetAmmoName 1 rObj", True),
     _so2f("ammoname.2", "ammo10", "GetAmmoName 2 rObj", True),
     _so2("ammoname.set", _form("rObj", NF["ammo10"]) + 'let sText := GetAmmoName 1 rObj\nrP.AuxVarSetStr "*nvfSO2" sText\n'
          'SetAmmoName "NVF" 1 rObj\nSetFunctionValue (GetAmmoName 1 rObj)', True),
     _so2("ammoname.restore", _form("rObj", NF["ammo10"]) + 'let sText := rP.AuxVarGetStr "*nvfSO2"\nSetAmmoName sText 1 rObj\n'
          'rP.AuxVarErase "*nvfSO2"\nSetFunctionValue (GetAmmoName 1 rObj)', True),
     _so2f("consume.stimpak", "stimpak", "GetFormIDString (GetIngestibleConsumeSound rObj)", True),
     _so2("consume.set", _form("rObj", NF["stimpak"]) + _form("rOther", "88B16")
          + 'let fRes := SetIngestibleConsumeSound rOther rObj\nSetFunctionValue (sv_Construct "%g %i" fRes (GetIngestibleConsumeSound rObj))', True),
     _so2f("creature.foot", "dog", "GetCreatureFootWeight rObj"),
     _so2f("creature.turn", "dog", "GetCreatureTurningSpeed rObj"),
     _so2v("creature.foot.doc", "GetCreatureFootWeight (rDoc.GetBaseObject)"),
     _so2("creature.set", _form("rObj", SO2["dog"]) + 'let fRes := SetCreatureTurningSpeed 45 rObj\n'
          'SetFunctionValue (sv_Construct "%g %g" fRes (GetCreatureTurningSpeed rObj))', True),
     _so2("creature.set.negative", _form("rObj", SO2["dog"]) + 'SetFunctionValue (SetCreatureFootWeight -1 rObj)'),
     _so2("creature.restore", _form("rObj", SO2["dog"]) + 'SetCreatureTurningSpeed 90 rObj\nSetFunctionValue (GetCreatureTurningSpeed rObj)'),
     _so2f("cellzone.doc", "doc", "GetFormIDString (GetCellEncounterZone rObj)", True),
     _so2f("zonerespawns.vault11", "vault11zone", "GetZoneRespawns rObj"),
     _so2f("zonerespawns.hoover", "hooverzone", "GetZoneRespawns rObj"),
     _so2f("isbaseform.pistol", "pistol10", "IsBaseForm rObj"),
     _so2v("isbaseform.player", "IsBaseForm rP"),
     _so2v("isbaseform.docref", "IsBaseForm rDoc"),
     _so2v("cloned.player", "rP.IsReferenceCloned"),
     _so2f("isactoralt.docbase", "doc", "IsActorAlt rObj"),
     _so2f("isactoralt.pistol", "pistol10", "IsActorAlt rObj"),
     _so2v("isactoralt.docref", "rDoc.IsActorAlt"),
     _so2("modsetting", 'let fRes := ModNumericGameSetting "fMoveRunMult" 1\n'
          'SetFunctionValue (sv_Construct "%g %g" fRes (GetNumericGameSetting "fMoveRunMult"))', True),
     _so2("modsetting.restore", 'ModNumericGameSetting "fMoveRunMult" -1\nSetFunctionValue (GetNumericGameSetting "fMoveRunMult")'),
     _so2v("modsetting.missing", 'ModNumericGameSetting "fNVFNone" 1'),
     _so2v("debugmode", "GetShowOffDebugMode")]
)


# tmpl1 (read-only; #1057): what the base-form getters answer for templated NPCs, against
# their records' own fields [observed: fnvtool esm decode]. Three take their template from
# another NPC (VHDLegionODeckHeavyTroopers, use Traits and Factions; VHDNCRRanger6Veteran,
# use Traits and Factions, its own PKID, SNAM and CNAM; VHDNCRTrooper7aHeavy, use Factions,
# not Traits), three from a leveled list (VSpawnSpecialJackalFem01Primm, VViperGangSpawnNRGunD,
# VSpawnJackalFemale01NR, using Traits, Factions and the Actor Effect List). Doc Mitchell
# untemplated beside them.
TM1 = {"legion": "17B58D", "ranger": "17A2EE", "trooper": "17A2DC", "jackalprimm": "1689BD", "viper": "167DFB",
       "jackal": "167DDD", "doc": "104C0C"}


def _tm1(pid, key, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return (f"tmpl1.{pid}", "ref rObj\narray_var aA\nfloat fRes\nBegin Function { }\n" + _form("rObj", TM1[key]) + rd,
            bool(text), None, "")


TMPL1_PROBES = [p for k in TM1 for p in (
    _tm1(f"{k}.factions", k, "GetBaseNumFactions rObj"),
    _tm1(f"{k}.faction0", k, "GetBaseNthFaction 0 rObj", "form"),
    _tm1(f"{k}.rank0", k, "GetBaseNthRank 0 rObj"),
    _tm1(f"{k}.class", k, "GetClass rObj", "form"),
    _tm1(f"{k}.spells", k, "ar_Size (GetBaseSpellListSpells rObj)"),
    _tm1(f"{k}.race", k, "GetRace rObj", "form"),
)]

# jg1 (writes, session only, each change put back; #1007): JohnnyGuitar batch 1
# (docs/spec-notes/ext-jg1.md) on FalloutNV.esm forms [observed: fnvtool esm inventory]:
# books, factions, the dog, races, the 10mm pistol, music types, a dresser with sounds, a
# lighting template.
JG1 = {"skillbook": "160227", "guidebook": "C5634", "lucky38": "1267AB", "ncr": "A46E7", "dog": "14F425",
       "caucasian": "19", "oldrace": "987DF", "success": "89580", "stinger": "177FA7", "dresser": "167D70",
       "hiddenvalley": "177550", "pistol": "434F", "doc": "104C0C", "shishkebab": "434E", "knife": "4326",
       "nailboard": "EB6B1"}


def _jg1(pid, body, text=False):
    return (f"jg1.{pid}", "ref rP\nref rObj\nref rOther\nref rSave\nfloat fRes\nfloat fSave\nstring_var sText\nstring_var sSave\n"
            "Begin Function { }\n"
            + _form("rP", NF["player"]) + body, text, None, "")


def _jg1v(pid, key, expr, text=False):
    return _jg1(pid, _form("rObj", JG1[key]) + f"SetFunctionValue ({expr})", text)


JG1_PROBES = (
    [_jg1v(f"book.{k}.flags", k, "GetBookFlags rObj") for k in ("skillbook", "guidebook")]
    + [_jg1v(f"book.{k}.skill", k, "GetBookSkill rObj") for k in ("skillbook", "guidebook")]
    # Each setter step puts the record back as it found it, in the same step (#1064 review).
    + [_jg1("book.setskill", _form("rObj", JG1["guidebook"]) + "let fSave := GetBookSkill rObj\nSetBookSkill rObj 33\n"
            "let fRes := GetBookSkill rObj\nSetBookSkill rObj 5\nlet sText := sv_Construct \"%g %g\" fRes (GetBookSkill rObj)\n"
            "SetBookSkill rObj fSave\nSetFunctionValue (sv_Construct \"%z %g\" sText (GetBookSkill rObj))", True),
       _jg1("book.setflags", _form("rObj", JG1["guidebook"]) + "SetBookFlags rObj 2\nlet fRes := GetBookFlags rObj\nSetBookFlags rObj 0\nSetFunctionValue fRes")]
    + [_jg1v(f"faction.{k}.flags", k, "GetFactionFlags rObj") for k in ("lucky38", "ncr")]
    + [_jg1("faction.setflags", _form("rObj", JG1["lucky38"]) + "SetFactionFlags rObj 257\nlet fRes := GetFactionFlags rObj\n"
            "SetFactionFlags rObj 256\nSetFunctionValue fRes")]
    + [_jg1v("creature.combatskill", "dog", "GetCreatureCombatSkill rObj"),
       _jg1v("basescale.dog", "dog", "GetBaseScale rObj"),
       _jg1v("basescale.doc", "doc", "GetBaseScale rObj"),
       _jg1("basescale.player", "SetFunctionValue (rP.GetBaseScale)"),
       _jg1v("basescale.pistol", "pistol", "GetBaseScale rObj")]
    + [_jg1(f"sign.{i}", f"SetFunctionValue (Sign {x})") for i, x in enumerate((5, -2, 0, 0.5))]
    + [_jg1v(f"race.head.{k}.{part}.{sex}", k, f"GetRaceHeadModelPath rObj {part} {sex}", True)
       for k in ("caucasian", "oldrace") for part in (0, 1, 7) for sex in (0, 1)]
    + [_jg1v(f"race.body.caucasian.{part}.{sex}", "caucasian", f"GetRaceBodyModelPath rObj {part} {sex}", True)
       for part in (0, 2) for sex in (0, 1)]
    + [_jg1v(f"weapon.worldmodel.{k}", k, "GetWeaponWorldModelPath rObj", True) for k in ("shishkebab", "knife", "nailboard")]
    + [_jg1v("weapon.worldmodel", "pistol", "GetWeaponWorldModelPath rObj", True),
       _jg1("weapon.worldmodel.set", _form("rObj", JG1["shishkebab"]) + 'let sSave := GetWeaponWorldModelPath rObj\n'
            'SetWeaponWorldModelPath rObj "nvf\\test.nif"\nlet sText := GetWeaponWorldModelPath rObj\n'
            # Back as a literal: a string_var here reaches the game as its name (ext-showoff.md §5).
            'SetWeaponWorldModelPath rObj "Weapons\\1HandMelee\\ShishkebabGO.NIF"\n'
            'SetFunctionValue (sv_Construct "%z|%z" sText (GetWeaponWorldModelPath rObj))', True)]
    + [_jg1v(f"music.{k}.path", k, "GetMusicTypePath rObj", True) for k in ("success", "stinger")]
    + [_jg1v(f"music.{k}.db", k, "GetMusicTypeDB rObj") for k in ("success", "stinger")]
    + [_jg1("music.setdb", _form("rObj", JG1["success"]) + "let fRes := GetMusicTypeDB rObj\nSetMusicTypeDB rObj -20\n"
            'SetFunctionValue (sv_Construct "%g" (GetMusicTypeDB rObj))\nSetMusicTypeDB rObj fRes', True)]
    + [_jg1v(f"container.sound.{w}", "dresser", f"GetFormIDString (GetContainerSound rObj {w})", True) for w in (0, 1, 2, 3)]
    # Slot 0 has a sound to put back (slot 2 is empty, and setting none leaves a slot as it is).
    + [_jg1("container.setsound", _form("rObj", JG1["dresser"]) + "let rSave := GetContainerSound rObj 0\n"
            "let rOther := GetContainerSound rObj 1\nSetContainerSound rObj 0 rOther\nlet rOther := GetContainerSound rObj 0\n"
            "let sText := GetFormIDString rOther\nlet rOther := GetFormFromMod \"FalloutNV.esm\" \"0\"\nSetContainerSound rObj 0 rOther\n"
            "let rOther := GetContainerSound rObj 0\nlet sText := sv_Construct \"%z %z\" sText (GetFormIDString rOther)\n"
            "SetContainerSound rObj 0 rSave\nlet rOther := GetContainerSound rObj 0\n"
            "SetFunctionValue (sv_Construct \"%z %z\" sText (GetFormIDString rOther))", True)]
    + [_jg1v(f"lgtm.{n}", "hiddenvalley", f"GetLightingTemplateTraitNumeric rObj {n}") for n in range(0, 18)]
    + [_jg1("lgtm.set", _form("rObj", JG1["hiddenvalley"]) + "let fRes := GetLightingTemplateTraitNumeric rObj 10\n"
            "SetLightingTemplateTraitNumeric rObj 10 99\nlet sText := sv_Construct \"%g\" (GetLightingTemplateTraitNumeric rObj 10)\n"
            "SetLightingTemplateTraitNumeric rObj 10 fRes\nSetFunctionValue sText", True)]
)


# jg2 (writes, session only, each change put back; #1007): JohnnyGuitar batch 2
# (docs/spec-notes/ext-jg2.md) on FalloutNV.esm forms [observed: fnvtool esm inventory]:
# PrimmNashResidence (an interior with lighting and an acoustic space), Doc Mitchell's
# house and reference, Vault11Zone, NVWastelandNightIS, the baseball bat, leather armor and
# Sunset Sarsaparilla.
JG2 = {"nash": "D70E2", "dochouse": "103DF9", "vault11zone": "E9081", "nightis": "CEE1B", "bat": "421C",
       "leather": "20423", "sarsaparilla": "103B1E"}


def _jg2(pid, body, text=False):
    return (f"jg2.{pid}", "ref rP\nref rDoc\nref rObj\nref rOther\nref rSave\nfloat fRes\nfloat fSave\nstring_var sText\n"
            "string_var sSave\nBegin Function { }\n"
            + _form("rP", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _jg2v(pid, key, expr, text=False):
    return _jg2(pid, _form("rObj", JG2[key]) + f"SetFunctionValue ({expr})", text)


JG2_PROBES = (
    # The acoustic space functions are left out: no form of the call compiled in the game
    # (ext-jg2.md).
    [_jg2v(f"interior.{n}", "nash", f"GetInteriorLightingTraitNumeric rObj {n}") for n in range(0, 17)]
    + [_jg2("interior.set", _form("rObj", JG2["nash"]) + "let fRes := GetInteriorLightingTraitNumeric rObj 10\n"
            "SetInteriorLightingTraitNumeric rObj 10 123\nlet sText := sv_Construct \"%g\" (GetInteriorLightingTraitNumeric rObj 10)\n"
            "SetInteriorLightingTraitNumeric rObj 10 fRes\nSetFunctionValue sText", True),
       # A colour channel given 300 and 25.5: kept, clamped or wrapped (#1102 review).
       _jg2("interior.setcolor", _form("rObj", JG2["nash"]) + "let fSave := GetInteriorLightingTraitNumeric rObj 0\n"
            "SetInteriorLightingTraitNumeric rObj 0 300\nlet fRes := GetInteriorLightingTraitNumeric rObj 0\n"
            "SetInteriorLightingTraitNumeric rObj 0 25.5\nlet sText := sv_Construct \"%g %g\" fRes (GetInteriorLightingTraitNumeric rObj 0)\n"
            "SetInteriorLightingTraitNumeric rObj 0 fSave\n"
            'SetFunctionValue (sv_Construct "%z %g" sText (GetInteriorLightingTraitNumeric rObj 0))', True)]
    + [_jg2("refzone.doc", "SetFunctionValue (GetFormIDString (rDoc.GetRefEncounterZone))", True),
       # The player as the reference (#1102 review), its zone saved and put back.
       _jg2("refzone.player", _form("rObj", JG2["vault11zone"]) + "let rSave := rP.GetRefEncounterZone\n"
            "let fRes := rP.SetRefEncounterZone rObj\nlet rOther := rP.GetRefEncounterZone\nlet sText := GetFormIDString rOther\n"
            "rP.SetRefEncounterZone rSave\nlet rOther := rP.GetRefEncounterZone\n"
            'SetFunctionValue (sv_Construct "%g %z %i %i" fRes sText rOther rSave)', True),
       _jg2("refzone.set", _form("rObj", JG2["vault11zone"]) + "let fRes := rDoc.SetRefEncounterZone rObj\n"
            "let rOther := rDoc.GetRefEncounterZone\nlet sText := GetFormIDString rOther\nrDoc.SetRefEncounterZone\n"
            "let rOther := rDoc.GetRefEncounterZone\n"
            'SetFunctionValue (sv_Construct "%g %z %i" fRes sText rOther)', True),
       # Each setter step saves the value and writes it back in the same step (#1102 review).
       _jg2("cellzone.set", _form("rObj", JG2["dochouse"]) + _form("rOther", JG2["vault11zone"])
            + "let rSave := GetCellEncounterZone rObj\nlet fRes := SetCellEncounterZone rObj rOther\n"
            "let rOther := GetCellEncounterZone rObj\nlet sText := GetFormIDString rOther\nSetCellEncounterZone rObj rSave\n"
            "let rOther := GetCellEncounterZone rObj\n"
            'SetFunctionValue (sv_Construct "%g %z %i %i" fRes sText rOther rSave)', True)]
    + [_jg2v("icon.sarsaparilla", "sarsaparilla", "GetMessageIconPath rObj", True),
       _jg2v("icon.leather.male", "leather", "GetMessageIconPath rObj 0", True),
       _jg2v("icon.leather.female", "leather", "GetMessageIconPath rObj 1", True),
       _jg2v("icon.bat", "bat", "GetMessageIconPath rObj", True),
       # The original goes back as a literal (the record's MICO, fnvtool esm decode): a
       # string_var given here reaches the game as its name (ext-showoff.md §5). The row shows
       # the saved, the set and the restored path, so a restore that differs shows.
       _jg2("icon.set", _form("rObj", JG2["sarsaparilla"]) + "let sSave := GetMessageIconPath rObj\n"
            'SetMessageIconPath "nvf\\icon.dds" rObj\nlet sText := GetMessageIconPath rObj\n'
            'SetMessageIconPath "interface\\icons\\pipboyimages_small\\items_small\\glow_items_cola.dds" rObj\n'
            'SetFunctionValue (sv_Construct "%z|%z|%z" sSave sText (GetMessageIconPath rObj))', True)]
    + [_jg2v("imod.night", "nightis", "GetIMODAnimatable rObj"),
       _jg2("imod.set", _form("rObj", JG2["nightis"]) + "let fSave := GetIMODAnimatable rObj\nSetIMODAnimatable rObj 1\n"
            "let fRes := GetIMODAnimatable rObj\nSetIMODAnimatable rObj fSave\n"
            'SetFunctionValue (sv_Construct "%g %g" fRes (GetIMODAnimatable rObj))', True)]
    + [_jg2v(f"vats.bat.{n}", "bat", f"GetWeaponVATSTraitNumeric rObj {n}") for n in range(0, 7)]
    + [_jg2("vats.set", _form("rObj", JG2["bat"]) + "let fSave := GetWeaponVATSTraitNumeric rObj 3\nSetWeaponVATSTraitNumeric rObj 3 99\n"
            "let fRes := GetWeaponVATSTraitNumeric rObj 3\nSetWeaponVATSTraitNumeric rObj 3 fSave\n"
            'SetFunctionValue (sv_Construct "%g %g" fRes (GetWeaponVATSTraitNumeric rObj 3))', True)]
)


# jip5 (writes, session only, each change put back; #1006): JIP batch 5
# (docs/spec-notes/jip-records-5.md) on FalloutNV.esm forms [observed: fnvtool esm
# inventory]: NVGecko (body-part data), LScrubDirtCanyon01Grass (a land texture with three
# grasses), DecalOrganicBallisticBB and LandscapeDirtNV (texture sets, with and without
# decal data), FreesideWorld and WastelandNV.
J5 = {"gecko": "10CD74", "ltex": "63A8D", "decal": "1A32C", "dirt": "11673A", "freeside": "10BEEA",
      "wasteland": "DA726", "grass": "17753"}


def _j5(pid, body, text=False):
    return (f"jip5.{pid}", "ref rP\nref rObj\nref rOther\nfloat fRes\nfloat fSave\nstring_var sText\nBegin Function { }\n"
            + _form("rP", NF["player"]) + body, text, None, "")


def _j5v(pid, key, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return _j5(pid, _form("rObj", J5[key]) + rd, bool(text))


JIP5_PROBES = (
    [_j5v(f"bpd.has.{n}", "gecko", f"GetBodyPartDataHasPart rObj {n}") for n in range(0, 16)]
    + [_j5v("bpd.ragdoll", "gecko", "GetBodyPartDataRagdoll rObj", "form"),
       _j5v("bpd.skeleton", "gecko", "GetBodyPartDataSkeleton rObj", True)]
    + [_j5v(f"bpd.head.num.{t}", "gecko", f"GetBodyPartTraitNumeric rObj 1 {t}") for t in range(0, 20)]
    + [_j5v(f"bpd.head.form.{t}", "gecko", f"GetBodyPartTraitForm rObj 1 {t}", "form") for t in range(0, 7)]
    + [_j5v(f"bpd.head.flag.{t}", "gecko", f"GetBodyPartFlag rObj 1 {t}") for t in range(0, 8)]
    + [_j5v("bpd.torso.num.2", "gecko", "GetBodyPartTraitNumeric rObj 0 2"),
       _j5("bpd.setnum", _form("rObj", J5["gecko"]) + "let fRes := GetBodyPartTraitNumeric rObj 1 1\nSetBodyPartTraitNumeric rObj 1 1 7\n"
           "let sText := sv_Construct \"%g\" (GetBodyPartTraitNumeric rObj 1 1)\nSetBodyPartTraitNumeric rObj 1 1 fRes\nSetFunctionValue sText", True),
       # The flag saved and written back in the same step (#1145 review).
       _j5("bpd.setflag", _form("rObj", J5["gecko"]) + "let fSave := GetBodyPartFlag rObj 1 2\nSetBodyPartFlag rObj 1 2 1\n"
           "let fRes := GetBodyPartFlag rObj 1 2\nSetBodyPartFlag rObj 1 2 fSave\n"
           'SetFunctionValue (sv_Construct "%g %g %g" fSave fRes (GetBodyPartFlag rObj 1 2))', True),
       # What the setters give back (#1145 review): each result, its value put back after.
       _j5("bpd.setflag.result", _form("rObj", J5["gecko"]) + "let fSave := GetBodyPartFlag rObj 1 2\n"
           "let fRes := SetBodyPartFlag rObj 1 2 1\nSetBodyPartFlag rObj 1 2 fSave\n"
           'SetFunctionValue (sv_Construct "%g %g" fRes (GetBodyPartFlag rObj 1 2))', True),
       _j5("bpd.setnum.result", _form("rObj", J5["gecko"]) + "let fSave := GetBodyPartTraitNumeric rObj 1 1\n"
           "let fRes := SetBodyPartTraitNumeric rObj 1 1 7\nSetBodyPartTraitNumeric rObj 1 1 fSave\n"
           'SetFunctionValue (sv_Construct "%g %g" fRes (GetBodyPartTraitNumeric rObj 1 1))', True)]
    + [_j5v("ltex.num", "ltex", "GetLandTextureNumGrasses rObj")]
    + [_j5v(f"ltex.grass.{i}", "ltex", f"GetLandTextureNthGrass rObj {i}", "form") for i in range(0, 4)]
    + [_j5v("ltex.txst", "ltex", "GetLandTextureTextureSet rObj", "form")]
    + [_j5v(f"ltex.trait.{t}", "ltex", f"GetLandTextureTraitNumeric rObj {t}") for t in range(0, 5)]
    + [_j5("ltex.addgrass", _form("rObj", J5["ltex"]) + _form("rOther", J5["grass"])
           + "LandTextureAddGrass rObj rOther\nlet fRes := GetLandTextureNumGrasses rObj\nLandTextureRemoveGrass rObj rOther\n"
           'SetFunctionValue (sv_Construct "%g %g" fRes (GetLandTextureNumGrasses rObj))', True),
       _j5("ltex.addgrass.result", _form("rObj", J5["ltex"]) + _form("rOther", J5["grass"])
           + "let fRes := LandTextureAddGrass rObj rOther\nlet fSave := LandTextureAddGrass rObj rOther\n"
           "let sText := sv_Construct \"%g %g\" fRes fSave\nlet fRes := LandTextureRemoveGrass rObj rOther\n"
           "let fSave := LandTextureRemoveGrass rObj rOther\n"
           'SetFunctionValue (sv_Construct "%z %g %g %g" sText fRes fSave (GetLandTextureNumGrasses rObj))', True),
       _j5("ltex.settrait.result", _form("rObj", J5["ltex"]) + "let fSave := GetLandTextureTraitNumeric rObj 1\n"
           "let fRes := SetLandTextureTraitNumeric rObj 1 55\nSetLandTextureTraitNumeric rObj 1 fSave\n"
           'SetFunctionValue (sv_Construct "%g %g" fRes (GetLandTextureTraitNumeric rObj 1))', True)]
    + [_j5v(f"txst.decal.{t}", "decal", f"GetTextureSetTraitNumeric rObj {t}") for t in range(0, 10)]
    + [_j5v(f"txst.dirt.{t}", "dirt", f"GetTextureSetTraitNumeric rObj {t}") for t in (0, 5, 8)]
    + [_j5v(f"txst.decal.flag.{t}", "decal", f"GetTextureSetFlag rObj {t}") for t in range(0, 5)]
    + [_j5v(f"txst.dirt.flag.{t}", "dirt", f"GetTextureSetFlag rObj {t}") for t in range(0, 4)]
    + [_j5("txst.setcolor", _form("rObj", J5["decal"]) + "let fRes := GetTextureSetTraitNumeric rObj 8\nSetTextureSetTraitNumeric rObj 8 10020030\n"
           "let sText := sv_Construct \"%g\" (GetTextureSetTraitNumeric rObj 8)\nSetTextureSetTraitNumeric rObj 8 fRes\nSetFunctionValue sText", True),
       _j5("txst.settrait.result", _form("rObj", J5["decal"]) + "let fSave := GetTextureSetTraitNumeric rObj 4\n"
           "let fRes := SetTextureSetTraitNumeric rObj 4 33\nSetTextureSetTraitNumeric rObj 4 fSave\n"
           'SetFunctionValue (sv_Construct "%g %g" fRes (GetTextureSetTraitNumeric rObj 4))', True)]
    + [_j5v(f"world.{k}.noise", k, "GetWorldspaceNoiseTexture rObj", True) for k in ("freeside", "wasteland")]
    + [_j5v(f"world.{k}.water", k, "GetWorldspaceWaterType rObj", "form") for k in ("freeside", "wasteland")]
)



# jip8 (writes, session only, each change put back; #1006): JIP batch 8
# (docs/spec-notes/jip-records-8.md) on FalloutNV.esm forms [observed: fnvtool esm
# inventory and decode]: CraigBoone (PC Level Mult, ACBS raw 750, two SPLO-less factions),
# AngelaWilliams (TPLT NCRTrooperAAMTEMPLATE, no template flags), NVMercenaryFemaleF (takes
# its factions from NVMercenaryMale, which alone has faction 0x173EEF), CrSuperMutant1GunA
# (INAM, two SPLO), Jet and Cateye (ALCH), WithdrawalJet (SPEL), EnchFourEyesHat (ENCH),
# CateyeBaseEffect and ChemIncAPJet (MGEF), the assault carbine (one mod, clip) and the
# 10mm pistol (three mods, the third Silence), RepairWall02 (IDLE), NVGecko (BPTD), Doc
# Mitchell's house and the Prospector Saloon (CELL), NVCleanWater (WATR).
J8 = {"boone": "92BD2", "angela": "F2746", "mercf": "165071", "mercfac": "173EEF", "merc2fac": "152E91",
      "ncr": "A46E7", "ncrarmor": "134B6C", "boonefac": "B160C", "mutant": "1CF93", "jet": "15164",
      "cateyealch": "7EA1B", "withdrawal": "33067", "ench": "17B78D", "cateye": "7EA1F", "apjet": "66EB8",
      "carbine": "8F21E", "pistol": "434F", "idle": "B250F", "gecko": "10CD74", "doccell": "103DF9",
      "saloon": "106185", "water": "1009CA", "caps": "F"}


def _j8(pid, body, text=False):
    return (f"jip8.{pid}", "ref rP\nref rObj\nref rOther\nref rItem\nfloat fRes\nfloat fSave\nfloat fA\nfloat fB\n"
            "string_var sText\nstring_var sSave\nBegin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


def _j8v(pid, key, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return _j8(pid, _form("rObj", J8[key]) + rd, bool(text))


def _j8o(pid, key, other, expr):
    return _j8(pid, _form("rObj", J8[key]) + _form("rOther", J8[other]) + f"SetFunctionValue ({expr})")


# A setter's result and the value read back, then the old value put back and read again.
def _j8set(pid, key, save, setter, read, restore):
    return _j8(pid, _form("rObj", J8[key]) + f"let fSave := {save}\nlet fRes := {setter}\nlet fA := {read}\n"
               f"{restore}\n" 'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fRes fA (' + read + "))", True)


JIP8_PROBES = (
    # ---- actor base data
    [_j8v(f"level.{k}.{t}", k, f"GetActorLevelingData {t} rObj") for k in ("boone", "angela", "mutant") for t in range(0, 4)]
    + [_j8set("level.boone.set0", "boone", "GetActorLevelingData 0 rObj", "SetActorLevelingData 0 0.5 rObj",
              "GetActorLevelingData 0 rObj", "SetActorLevelingData 0 fSave rObj"),
       _j8set("level.angela.set2", "angela", "GetActorLevelingData 2 rObj", "SetActorLevelingData 2 12 rObj",
              "GetActorLevelingData 2 rObj", "SetActorLevelingData 2 fSave rObj")]
    + [_j8o(f"facrank.{k}.{f}", k, f, "GetBaseFactionRank rOther rObj")
       for k, f in (("mercf", "mercfac"), ("mercf", "merc2fac"), ("mercf", "ncr"), ("angela", "ncrarmor"),
                    ("angela", "ncr"), ("angela", "boonefac"), ("boone", "boonefac"))]
    + [_j8v(f"deathitem.{k}", k, "GetDeathItem rObj", "form") for k in ("mutant", "angela", "boone")]
    + [_j8v(f"template.{k}", k, "GetActorTemplate rObj", "form") for k in ("angela", "mercf", "mutant", "boone")]
    + [_j8("deathitem.set", _form("rObj", J8["mutant"]) + _form("rOther", J8["caps"])
           + "let rItem := GetDeathItem rObj\nlet fRes := SetDeathItem rOther rObj\nlet sText := GetFormIDString (GetDeathItem rObj)\n"
           "SetDeathItem rItem rObj\n"
           'SetFunctionValue (sv_Construct "%g %z %z" fRes sText (GetFormIDString (GetDeathItem rObj)))', True)]
    + [_j8v(f"effects.num.{k}", k, "GetNumActorEffects rObj") for k in ("mutant", "boone", "angela")]
    # GetNthActorEffect compiles only into a variable, not inside another call [observed:
    # trial 2026-11-08]; with the calling reference and without.
    + [_j8(f"effects.nth.{k}.{i}", _form("rObj", J8[k]) + f"let rItem := GetNthActorEffect {i} rObj\n"
           "SetFunctionValue (GetFormIDString rItem)", True) for k in ("mutant", "boone") for i in range(0, 3)]
    + [_j8(f"effects.nth.ref.mutant.{i}", _form("rObj", J8["mutant"]) + f"let rItem := rP.GetNthActorEffect {i} rObj\n"
           "SetFunctionValue (GetFormIDString rItem)", True) for i in range(0, 3)]
    + [_j8("effects.addremove", _form("rObj", J8["mutant"]) + _form("rOther", J8["withdrawal"])
           + "let fRes := AddBaseEffectListEffect rOther rObj\nlet fA := GetNumActorEffects rObj\n"
           "AddBaseEffectListEffect rOther rObj\nlet fB := GetNumActorEffects rObj\n"
           "let rItem := GetNthActorEffect 2 rObj\nlet sText := GetFormIDString rItem\n"
           "let fSave := RemoveBaseEffectListEffect rOther rObj\n"
           'SetFunctionValue (sv_Construct "%g %g %g %z %g %g" fRes fA fB sText fSave (GetNumActorEffects rObj))', True),
       # Not an actor effect: an ingestible.
       _j8("effects.add.alch", _form("rObj", J8["mutant"]) + _form("rOther", J8["jet"])
           + "let fRes := AddBaseEffectListEffect rOther rObj\nlet fA := GetNumActorEffects rObj\n"
           "if fA > 2\nRemoveBaseEffectListEffect rOther rObj\nendif\n"
           'SetFunctionValue (sv_Construct "%g %g %g" fRes fA (GetNumActorEffects rObj))', True)]
    + [_j8("combat.excluded", "let fSave := rP.GetExcludedCombatActions\nlet fRes := rP.SetExcludedCombatActions 5\n"
           "let fA := rP.GetExcludedCombatActions\nrP.SetExcludedCombatActions 2147483647\nlet fB := rP.GetExcludedCombatActions\n"
           "rP.SetExcludedCombatActions fSave\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g %g" fSave fRes fA fB (rP.GetExcludedCombatActions))', True)]
    # ---- ingestibles and base effects
    + [_j8v(f"addict.chance.{k}", k, "GetAddictionChance rObj") for k in ("jet", "cateyealch")]
    + [_j8v(f"addict.effect.{k}", k, "GetAddictionEffect rObj", "form") for k in ("jet", "cateyealch")]
    + [_j8set(f"addict.set.{n}", "jet", "GetAddictionChance rObj", f"SetAddictionChance rObj {v}",
              "GetAddictionChance rObj", "SetAddictionChance rObj fSave")
       for n, v in (("half", "0.5"), ("fifty", "50"), ("over", "1.5"), ("neg", "-0.5"))]
    + [_j8("addict.seteffect", _form("rObj", J8["cateyealch"]) + _form("rOther", J8["withdrawal"])
           + "let rItem := GetAddictionEffect rObj\nlet fRes := SetAddictionEffect rObj rOther\n"
           "let sText := GetFormIDString (GetAddictionEffect rObj)\nSetAddictionEffect rObj rItem\n"
           'SetFunctionValue (sv_Construct "%g %z %z" fRes sText (GetFormIDString (GetAddictionEffect rObj)))', True)]
    + [_j8v(f"mgef.script.{k}", k, "GetBaseEffectScript rObj", "form") for k in ("cateye", "apjet")]
    + [_j8v(f"mgef.flag.cateye.{i}", "cateye", f"GetBaseEffectFlag rObj {i}") for i in range(0, 29)]
    + [_j8set("mgef.setflag.2", "cateye", "GetBaseEffectFlag rObj 2", "SetBaseEffectFlag rObj 2 1",
              "GetBaseEffectFlag rObj 2", "SetBaseEffectFlag rObj 2 fSave"),
       # Not in the page's table.
       _j8set("mgef.setflag.11", "cateye", "GetBaseEffectFlag rObj 11", "SetBaseEffectFlag rObj 11 1",
              "GetBaseEffectFlag rObj 11", "SetBaseEffectFlag rObj 11 fSave")]
    + [_j8v(f"effflag.{k}.{i}", k, f"GetEffectFlag rObj {i}") for k in ("withdrawal", "ench", "jet") for i in range(0, 8)]
    + [_j8set(f"effflag.set.{k}.{i}", k, f"GetEffectFlag rObj {i}", f"SetEffectFlag rObj {i} (1 - fSave)",
              f"GetEffectFlag rObj {i}", f"SetEffectFlag rObj {i} fSave")
       for k, i in (("withdrawal", 0), ("withdrawal", 1), ("withdrawal", 2), ("withdrawal", 4), ("ench", 0),
                    ("ench", 1), ("ench", 2), ("jet", 0), ("jet", 1), ("jet", 2), ("jet", 3))]
    # ---- weapons
    + [_j8v(f"weap.sound.{k}", k, "GetWeaponDetectionSoundLevel rObj") for k in ("carbine", "pistol")]
    + [_j8set("weap.setsound", "carbine", "GetWeaponDetectionSoundLevel rObj", "SetWeaponDetectionSoundLevel rObj 2",
              "GetWeaponDetectionSoundLevel rObj", "SetWeaponDetectionSoundLevel rObj fSave")]
    + [_j8v(f"weap.hasmod.{k}.{t}", k, f"WeaponHasModType rObj {t}") for k in ("carbine", "pistol") for t in range(0, 16)]
    + [_j8v(f"weap.reloadanim.{k}", k, "GetWeaponModReloadAnim rObj") for k in ("carbine", "pistol")]
    + [_j8set("weap.setreloadanim", "carbine", "GetWeaponModReloadAnim rObj", "SetWeaponModReloadAnim rObj 3",
              "GetWeaponModReloadAnim rObj", "SetWeaponModReloadAnim rObj fSave"),
       _j8set("weap.setmodeffect", "carbine", "GetWeaponItemModEffect 2 rObj", "SetWeaponItemModEffect rObj 2 14",
              "GetWeaponItemModEffect 2 rObj", "SetWeaponItemModEffect rObj 2 fSave"),
       _j8set("weap.setmodeffect.15", "carbine", "GetWeaponItemModEffect 2 rObj", "SetWeaponItemModEffect rObj 2 15",
              "GetWeaponItemModEffect 2 rObj", "SetWeaponItemModEffect rObj 2 fSave"),
       _j8set("weap.onhit", "carbine", "GetWeaponOnHitSetting rObj", "SetWeaponOnHitSetting rObj 2",
              "GetWeaponOnHitSetting rObj", "SetWeaponOnHitSetting rObj fSave"),
       # The carbine's embedded actor value is PerceptionCondition [observed: decode]; the
       # result only (no getter), then put back.
       _j8("weap.embeddedav", _form("rObj", J8["carbine"]) + "let fRes := SetEmbeddedWeaponAV rObj Strength\n"
           "let fA := SetEmbeddedWeaponAV rObj PerceptionCondition\n"
           'SetFunctionValue (sv_Construct "%g %g" fRes fA)', True)]
    # ---- idles and body parts
    + [_j8v(f"idle.loop.{m}", "idle", f"GetIdleLoopTimes rObj {m}") for m in (0, 1)]
    + [_j8("idle.setloop", _form("rObj", J8["idle"]) + "let fA := GetIdleLoopTimes rObj 0\nlet fB := GetIdleLoopTimes rObj 1\n"
           "let fRes := SetIdleLoopTimes rObj 3 9\nlet sText := sv_Construct \"%g %g\" (GetIdleLoopTimes rObj 0) (GetIdleLoopTimes rObj 1)\n"
           "SetIdleLoopTimes rObj 300 2\nlet sSave := sv_Construct \"%g %g\" (GetIdleLoopTimes rObj 0) (GetIdleLoopTimes rObj 1)\n"
           "SetIdleLoopTimes rObj fA fB\n"
           'SetFunctionValue (sv_Construct "%g %z %z %g %g" fRes sText sSave (GetIdleLoopTimes rObj 0) (GetIdleLoopTimes rObj 1))', True)]
    + [_j8v(f"bpd.replmodel.{p}", "gecko", f"GetBodyPartReplacementModel rObj {p}", True) for p in range(0, 15)]
    # ---- cells and water
    + [_j8v(f"cell.flag.{k}.{i}", k, f"GetCellFlag rObj {i}") for k in ("doccell", "saloon") for i in range(0, 18)]
    + [_j8set(f"cell.setflag.{i}", "doccell", f"GetCellFlag rObj {i}", f"SetCellFlag rObj {i} (1 - fSave)",
              f"GetCellFlag rObj {i}", f"SetCellFlag rObj {i} fSave") for i in (1, 4, 8, 12, 16)]
    + [_j8v(f"water.trait.{t}", "water", f"GetWaterTrait rObj {t}") for t in range(0, 53)]
    # The colours whole (the numbers above keep six digits).
    + [_j8v(f"water.color.{t}", "water", f'sv_Construct "%.0f" (GetWaterTrait rObj {t})', True) for t in (12, 13, 14)]
    # SetWaterFormEffect gives no result: `let fRes := SetWaterFormEffect …` does not
    # compile [observed: trial 2026-11-08].
    + [_j8v("water.effect", "water", "GetWaterFormEffect rObj", "form"),
       _j8("water.seteffect", _form("rObj", J8["water"]) + _form("rOther", J8["withdrawal"])
           + "let rItem := GetWaterFormEffect rObj\nSetWaterFormEffect rObj rOther\n"
           "let sText := GetFormIDString (GetWaterFormEffect rObj)\nSetWaterFormEffect rObj rItem\n"
           'SetFunctionValue (sv_Construct "%z %z" sText (GetFormIDString (GetWaterFormEffect rObj)))', True)]
    # Last: a string argument takes no string variable and no "" [observed: trial
    # 2026-11-08], so the name goes back as the literal read before ("Head") and the
    # replacement model cannot be put back for the session; nothing after reads NVGecko.
    + [_j8("bpd.setname", _form("rObj", J8["gecko"]) + "let sSave := GetBodyPartName rObj 1\n"
           'let fRes := SetBodyPartName rObj 1 "NvfProbe"\nlet sText := GetBodyPartName rObj 1\n'
           'SetBodyPartName rObj 1 "Head"\n'
           'SetFunctionValue (sv_Construct "%g [%z] [%z] [%z]" fRes sSave sText (GetBodyPartName rObj 1))', True),
       _j8("bpd.setreplmodel", _form("rObj", J8["gecko"]) + "let sSave := GetBodyPartReplacementModel rObj 1\n"
           'let fRes := SetBodyPartReplacementModel rObj 1 "nvfprobe.nif"\n'
           'SetFunctionValue (sv_Construct "%g [%z] [%z] [%z]" fRes sSave (GetBodyPartReplacementModel rObj 1) '
           "(GetBodyPartReplacementModel rObj 2))", True)]
)

# jip9 (writes, session only, each switch and value put back; #1006): JIP batch 9
# (docs/spec-notes/jip-ui-9.md): the switches' states without an argument and after one,
# put back; the cursor outside MenuMode; the volumes and their defaults; SetFontFile's
# result; immortal mode; interaction by type; the hotkeys' inventory references; the
# Pip-Boy radio; the enemy health bar's actor; GetPosEx and the NIF block values in
# script variables on the player (Bip01 Head, its world place against GetPos; UpperBody,
# the body shape, for the material values, none of which the batch knows).
def _j9(pid, body, text=False):
    return (f"jip9.{pid}", "ref rP\nref rObj\nref rItem\nfloat fRes\nfloat fSave\nfloat fA\nfloat fB\nfloat fX\nfloat fY\nfloat fZ\n"
            "string_var sText\nBegin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


def _j9v(pid, expr):
    return _j9(pid, f"SetFunctionValue ({expr})")


# A switch read, set, read, put back and read again.
def _j9switch(pid, fn, value):
    return _j9(pid, f"let fSave := {fn}\nlet fRes := {fn} {value}\nlet fA := {fn}\n{fn} fSave\n"
               f'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fRes fA ({fn}))', True)


def _j9xyz(pid, call):
    return _j9(pid, "let fX := -1\nlet fY := -1\nlet fZ := -1\n" f"let fRes := {call}\n"
               'SetFunctionValue (sv_Construct "%g %.3f %.3f %.3f" fRes fX fY fZ)', True)


JIP9_PROBES = (
    [_j9switch("hudcursor", "ToggleHUDCursor", 1),
     _j9switch("mouse.1", "ToggleMouseMovement", 1),
     _j9switch("mouse.9", "ToggleMouseMovement", 9),
     _j9switch("vanity", "ToggleVanityWheel", 0),
     _j9switch("camcoll", "ToggleCameraCollision", 0),
     _j9switch("immortal.1", "ToggleImmortalMode", 1),
     _j9switch("immortal.5", "ToggleImmortalMode", 5)]
    # Outside MenuMode and the HUD cursor [hypothesis: 0].
    + [_j9v(f"cursor.{a}{x}", f"GetCursorPos {a} {x}") for a in ("X", "Y") for x in (0, 1)]
    + [_j9v(f"volume.{t}", f"GetGameVolume {t}") for t in range(0, 7)]
    + [_j9("volume.set", "let fSave := GetGameVolume 4\nSetGameVolume 4 30\nlet fA := GetGameVolume 4\n"
           "SetGameVolume 4 150\nlet fB := GetGameVolume 4\nSetGameVolume 4\nlet fRes := GetGameVolume 4\n"
           "SetGameVolume 4 fSave\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g %g" fSave fA fB fRes (GetGameVolume 4))', True)]
    + [_j9v("font.set.9", 'SetFontFile 9 "textures\\fonts\\glow_monofonto_large.fnt"'),
       _j9v("font.set.missing", 'SetFontFile 2 "textures\\fonts\\nvf no such font.fnt"'),
       _j9("font.get.2", "SetFunctionValue (GetFontFile 2)", True)]
    + [_j9("interact.type", "let fSave := GetInteractionDisabledType 28\nSetInteractionDisabledType 1 28\n"
           "let fA := GetInteractionDisabledType 28\nlet fB := GetInteractionDisabledType 21\nSetInteractionDisabledType fSave 28\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fA fB (GetInteractionDisabledType 28))', True)]
    + [_j9(f"hotkey.{k}", f"let rItem := GetHotkeyItemRef {k}\nlet rObj := GetHotkeyItem {k}\n"
           'if rItem\nlet sText := GetFormIDString (rItem.GetBaseObject)\nelse\nlet sText := "none"\nendif\n'
           'SetFunctionValue (sv_Construct "%z %z" sText (GetFormIDString rObj))', True) for k in range(1, 9)]
    # The same item put on its own key again: the result.
    + [_j9("hotkey.reset", "let fRes := -1\nlet rItem := GetHotkeyItemRef 1\nif rItem\nlet fRes := rItem.SetHotkeyItemRef 1\nendif\n"
           "let fA := -1\nif rItem\nlet fA := rItem.SetHotkeyItemRef 2\nendif\n"
           'SetFunctionValue (sv_Construct "%g %g" fRes fA)', True)]
    + [_j9("radio", 'let rObj := GetPipboyRadio\nSetFunctionValue (GetFormIDString rObj)', True),
       _j9("enemytarget", 'let rObj := GetEnemyHealthTarget\nSetFunctionValue (GetFormIDString rObj)', True)]
    + [_j9xyz("posex", "0\nrP.GetPosEx fX fY fZ"),
       _j9("posex.getpos", 'SetFunctionValue (sv_Construct "%.3f %.3f %.3f" (rP.GetPos X) (rP.GetPos Y) (rP.GetPos Z))', True)]
    + [_j9xyz(f"niftrans.head.{w}", f'rP.GetNifBlockTranslationAlt "Bip01 Head" fX fY fZ {w}') for w in (0, 1)]
    + [_j9xyz(f"nifrot.head.{w}", f'rP.GetNifBlockRotationAlt "Bip01 Head" fX fY fZ {w}') for w in range(0, 4)]
    + [_j9xyz("niftrans.missing", 'rP.GetNifBlockTranslationAlt "NvfNoSuchBlock" fX fY fZ 1')]
    # The rows above, with declared script variables for the outputs, did not compile in
    # the original [observed 2026-11-08]; the same calls with the variables declared
    # inline, as mods' script-runner files write them (ext-script-runner.md).
    + [_j9("posex.inline", "rP.GetPosEx (float fP) (float fQ) (float fW)\n"
           'SetFunctionValue (sv_Construct "%.3f %.3f %.3f" fP fQ fW)', True),
       _j9("nifrot.head.inline", 'let fRes := rP.GetNifBlockRotationAlt "Bip01 Head" (float fP) (float fQ) (float fW) 0\n'
           'SetFunctionValue (sv_Construct "%g" fRes)', True)]
    # GetNifBlockTranslation with an axis, the Fallout 3 plugin's form, did not compile
    # [observed 2026-11-08: niftrans.head.plain]; JIP's takes getWorld and gives an array.
    + [_j9v("niftrans.head.array", 'ar_Size (rP.GetNifBlockTranslation "Bip01 Head" 1)')]
    + [_j9v(f"material.upperbody.{i}", f'rP.GetMaterialPropertyValue "UpperBody" {i}') for i in range(0, 10)]
    + [_j9("material.set", 'let fSave := rP.GetMaterialPropertyValue "UpperBody" 7\n'
           'let fRes := rP.SetMaterialPropertyValue "UpperBody" 7 0.5\nlet fA := rP.GetMaterialPropertyValue "UpperBody" 7\n'
           'rP.SetMaterialPropertyValue "UpperBody" 7 fSave\n'
           'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fRes fA (rP.GetMaterialPropertyValue "UpperBody" 7))', True),
       _j9v("material.missing", 'rP.GetMaterialPropertyValue "NvfNoSuchBlock" 7')]
)



# jg5 (writes, session only, each change put back where the plugin allows; #1007):
# JohnnyGuitar batch 5 (docs/spec-notes/ext-jg5.md) on FalloutNV.esm forms [observed:
# fnvtool esm inventory and decode; fnvtool world alt-textures]: VaultSuit21 (alternate
# textures on the male biped model, index 0, and the female biped, 0 and 6),
# ArmorRadiationSuit (none on the male biped), MS18RangerBattlearmor, WeapNVDynamiteNoDrop (indices 2, 0, 1),
# VMapGoodspringsRegion (map name "Goodsprings") and 0x123CE3 (the other region of the
# Goodsprings cell, no map data), CamDefaultShoot, NVGecko and DefaultBodyPartData, the
# Stimpak, Jet, WithdrawalJet and the 10mm pistol, Doc Mitchell's reference. The player's
# own location name needs travel (console steps) and is not in this set.
JG5 = {"vs21": "104184", "radsuit": "33078", "dynamite": "10108D", "gsregion": "15FF5E", "gsother": "123CE3",
       "cams": "1B2C2", "gecko": "10CD74", "defaultbpd": "1D", "stimpak": "15169", "jet": "15164",
       "withdrawal": "33067", "pistol": "434F", "docref": "104C0F", "ranger": "23030"}


def _jg5(pid, body, text=False):
    return (f"jg5.{pid}", "ref rP\nref rObj\nref rOther\nref rItem\nfloat fRes\nfloat fSave\nfloat fA\nfloat fB\nfloat fC\n"
            "float fD\nfloat fE\nstring_var sText\nstring_var sSave\narray_var aList\narray_var aKeys\nBegin Function { }\n"
            + _form("rP", NF["player"]) + body, text, None, "")


def _jg5v(pid, key, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return _jg5(pid, _form("rObj", JG5[key]) + rd, bool(text))


# An array or map as "size: key=form ...", keys in the array's own order.
def _jg5arr(pid, key, expr):
    return _jg5(pid, _form("rObj", JG5[key]) + f"let aList := {expr}\nlet aKeys := ar_Keys aList\nlet fA := ar_Size aKeys\n"
                'let sText := sv_Construct "%g:" fA\nlet fB := 0\nwhile fB < fA\nlet fC := aKeys[fB]\nlet rItem := aList[fC]\n'
                'let sText := sv_Construct "%z %g=%z" sText fC (GetFormIDString rItem)\nlet fB += 1\nloop\nSetFunctionValue sText', True)


JG5_PROBES = (
    # 1. how GetRGBColor packs the channels (whole numbers)
    [_jg5(f"rgb.{r}.{g}.{b}", f'SetFunctionValue (sv_Construct "%.0f" (GetRGBColor {r} {g} {b}))', True)
     for r, g, b in ((255, 128, 1), (1, 0, 0), (0, 1, 0), (0, 0, 1), (0, 0, 0), (255, 255, 255))]
    # 2. GetTimePlayed's units, as ratios (the values themselves differ run to run): 0/1,
    # 1/2, 3/0 and no argument/0, and whether unit 1 has a fraction.
    + [_jg5("time.units", "let fA := GetTimePlayed 0\nlet fB := GetTimePlayed 1\nlet fC := GetTimePlayed 2\n"
            "let fD := GetTimePlayed 3\nlet fE := GetTimePlayed\nlet sText := sv_Construct \"t\"\n"
            'if fB > 0\nlet sText := sv_Construct "%z %.0f" sText (fA / fB)\nendif\n'
            'if fC > 0\nlet sText := sv_Construct "%z %.0f" sText (fB / fC)\nelse\nlet sText := sv_Construct "%z c0" sText\nendif\n'
            'if fA > 0\nlet sText := sv_Construct "%z %.2f %.2f" sText (fD / fA) (fE / fA)\nendif\n'
            'SetFunctionValue (sv_Construct "%z %g" sText ((fB - (Floor fB)) > 0))', True)]
    # 3. a reference's location name, without travel: Doc Mitchell in his house
    + [_jg5("location.docref", _form("rObj", JG5["docref"]) + "SetFunctionValue (rObj.GetLocationName)", True)]
    # 4. region map names; the region without map data keeps its new name for the session
    + [_jg5v(f"region.name.{k}", k, "GetRegionMapName rObj", True) for k in ("gsregion", "gsother")]
    + [_jg5("region.setname.new", _form("rObj", JG5["gsother"]) + 'let fRes := SetRegionMapName rObj "NvfProbe"\n'
            'SetFunctionValue (sv_Construct "%g [%z]" fRes (GetRegionMapName rObj))', True),
       _jg5("region.setname.back", _form("rObj", JG5["gsregion"]) + 'let fRes := SetRegionMapName rObj "NvfProbe"\n'
            'let sText := GetRegionMapName rObj\nSetRegionMapName rObj "Goodsprings"\n'
            'SetFunctionValue (sv_Construct "%g [%z] [%z]" fRes sText (GetRegionMapName rObj))', True)]
    # 5. alternate textures of an armour: no model given, then each model, then 5
    + [_jg5arr("alt.ex.vs21", "vs21", "GetAltTexturesEx rObj"), _jg5arr("alt.ex.radsuit", "radsuit", "GetAltTexturesEx rObj")]
    + [_jg5arr(f"alt.ex.vs21.{m}", "vs21", f"GetAltTexturesEx rObj {m}") for m in range(0, 6)]
    + [_jg5arr(f"alt.armor.{k}.{m}", k, f"GetArmorAltTextures rObj {m}") for k in ("vs21", "radsuit") for m in range(1, 6)]
    # MS18RangerBattlearmor: every model's entries differ (male biped 6, 7, 1; female biped
    # 4, 1, 5; male world 0), which tells which model the call without one reads.
    + [_jg5arr("alt.ex.ranger", "ranger", "GetAltTexturesEx rObj")]
    + [_jg5arr(f"alt.ex.ranger.{m}", "ranger", f"GetAltTexturesEx rObj {m}") for m in range(1, 5)]
    + [_jg5arr("alt.weapon.dynamite", "dynamite", "GetWeaponAltTextures rObj"),
       _jg5arr("alt.ex.dynamite", "dynamite", "GetAltTexturesEx rObj")]
    # 6. the weapon alt-texture setters' results: clearing an index without an entry, a form
    # that is not a texture set, and index 0's own texture set put back on it
    + [_jg5("alt.clear.none", _form("rObj", JG5["dynamite"]) + "let fRes := ClearWeaponAltTexture rObj 5\n"
            'SetFunctionValue (sv_Construct "%g %g" fRes (ar_Size (GetWeaponAltTextures rObj)))', True),
       _jg5("alt.set.nottxst", _form("rObj", JG5["dynamite"]) + _form("rOther", JG5["stimpak"])
            + "let fRes := SetWeaponAltTexture rObj 5 rOther\n"
            'SetFunctionValue (sv_Construct "%g %g" fRes (ar_Size (GetWeaponAltTextures rObj)))', True),
       _jg5("alt.set.same", _form("rObj", JG5["dynamite"]) + "let aList := GetAltTexturesEx rObj\nlet rOther := aList[0]\n"
            "let fRes := SetWeaponAltTexture rObj 0 rOther\nlet aList := GetAltTexturesEx rObj\nlet rItem := aList[0]\n"
            'SetFunctionValue (sv_Construct "%g %z %z %g" fRes (GetFormIDString rOther) (GetFormIDString rItem) '
            "(ar_Size (GetWeaponAltTextures rObj)))", True)]
    # 7. actor effect types
    + [_jg5v(f"effecttype.{k}", k, "GetActorEffectType rObj") for k in ("withdrawal", "jet", "pistol")]
    # 8. a VATS camera shot's traits and flags; trait 1 set to 7, outside its table
    + [_jg5v(f"cams.trait.{t}", "cams", f"GetCameraShotTraitNumeric rObj {t}") for t in range(0, 10)]
    + [_jg5v("cams.flags", "cams", "GetCameraShotFlags rObj"),
       _jg5v("cams.imod", "cams", "GetCameraShotImageSpaceModifier rObj", "form"),
       _jg5("cams.settrait.1.7", _form("rObj", JG5["cams"]) + "let fSave := GetCameraShotTraitNumeric rObj 1\n"
            "let fRes := SetCameraShotTraitNumeric rObj 1 7\nlet fA := GetCameraShotTraitNumeric rObj 1\n"
            "SetCameraShotTraitNumeric rObj 1 fSave\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fRes fA (GetCameraShotTraitNumeric rObj 1))', True),
       _jg5("cams.settrait.6", _form("rObj", JG5["cams"]) + "let fSave := GetCameraShotTraitNumeric rObj 6\n"
            "let fRes := SetCameraShotTraitNumeric rObj 6 2.5\nlet fA := GetCameraShotTraitNumeric rObj 6\n"
            "SetCameraShotTraitNumeric rObj 6 fSave\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fRes fA (GetCameraShotTraitNumeric rObj 6))', True)]
    # 9. body-part strings, the head (part 1) and the torso (part 0)
    + [_jg5v(f"bpd.{k}.{p}.{t}", k, f"GetBodyPartTraitString rObj {p} {t}", True)
       for k in ("gecko", "defaultbpd") for p in (0, 1) for t in range(0, 7)]
    # 10. equip types: one in the table, one outside it, and -1
    + [_jg5v("equip.stimpak", "stimpak", "GetEquipType rObj")]
    + [_jg5(f"equip.set.{v}", _form("rObj", JG5["stimpak"]) + f"let fSave := GetEquipType rObj\nlet fRes := SetEquipType rObj {v}\n"
            "let fA := GetEquipType rObj\nSetEquipType rObj fSave\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fRes fA (GetEquipType rObj))', True) for v in (5, 40, -1)]
)


# jg6 (writes, session only, each change put back; #1007): JohnnyGuitar batch 6
# (docs/spec-notes/ext-jg6.md) on FalloutNV.esm forms [observed: fnvtool esm inventory and
# decode]: Doc Mitchell's reference, his house (GSDocMitchellHouse), its door and the couch
# trigger (XPRM box), the NovacInterior and RepconnFacility lighting templates, Radio New
# Vegas' talking activator, the Goodsprings schoolhouse. RemovePrimitive cannot be put back
# and is not probed.
JG6 = {"docref": "104C0F", "dochouse": "103DF9", "docdoor": "103E61", "couchtrigger": "1055B9",
       "novacltmp": "172A59", "repconnltmp": "17389F", "rnv": "14DF02", "schoolhouse": "108F39"}


def _jg6(pid, body, text=False):
    return (f"jg6.{pid}", "ref rP\nref rObj\nref rOther\nref rItem\nfloat fRes\nfloat fSave\nfloat fA\nfloat fB\nfloat fC\n"
            "string_var sText\nstring_var sSave\narray_var aList\nBegin Function { }\n"
            + _form("rP", NF["player"]) + body, text, None, "")


def _jg6v(pid, key, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return _jg6(pid, _form("rObj", JG6[key]) + rd, bool(text))


JG6_PROBES = (
    # 1. actor values by code on the player, each change put back: ModAVAlt Luck +1 and the
    # modifier's filters (1 permanent, 2 temporary, 3 both, 4, none), DamageAVAlt and
    # RestoreAVAlt on Health, ForceAVAlt then UnforceAV (documented as doing nothing)
    [_jg6("av.mod", "let fSave := rP.GetAV Luck\nrP.ModActorValueAlt 11 1\nlet fA := rP.GetAV Luck\n"
          "let sText := sv_Construct \"%g %g %g %g %g\" (fA - fSave) (rP.GetActorValueModifierAlt Luck 1) "
          "(rP.GetActorValueModifierAlt Luck 2) (rP.GetActorValueModifierAlt Luck 3) (rP.GetActorValueModifierAlt Luck 4)\n"
          'let sText := sv_Construct "%z %g" sText (rP.GetActorValueModifierAlt Luck)\n'
          "rP.ModActorValueAlt 11 -1\nSetFunctionValue (sv_Construct \"%z %g\" sText ((rP.GetAV Luck) - fSave))", True),
     _jg6("av.damage", "let fSave := rP.GetAV Health\nrP.DamageActorValueAlt 16 5\nlet fA := rP.GetAV Health\n"
          "rP.RestoreActorValueAlt 16 5\nSetFunctionValue (sv_Construct \"%g %g\" (fA - fSave) ((rP.GetAV Health) - fSave))", True),
     _jg6("av.force.unforce", "let fSave := rP.GetAV Luck\nrP.ForceActorValueAlt 11 (fSave + 2)\nlet fA := rP.GetAV Luck\n"
          "rP.UnforceAV Luck\nlet fB := rP.GetAV Luck\nrP.ForceAV Luck fSave\n"
          'SetFunctionValue (sv_Construct "%g %g %g" (fA - fSave) (fB - fSave) ((rP.GetAV Luck) - fSave))', True),
     # SetAVAlt then UnsetAV on Doc's Luck: whether UnsetAV puts the first base back
     _jg6("av.set.unset", _form("rObj", JG6["docref"]) + "let fSave := rObj.GetBaseAV Luck\nrObj.SetActorValueAlt 11 (fSave + 3)\n"
          "let fA := rObj.GetBaseAV Luck\nrObj.SetAV Luck (fSave + 4)\nlet fB := rObj.GetBaseAV Luck\nrObj.UnsetAV Luck\n"
          "let fC := rObj.GetBaseAV Luck\nrObj.SetAV Luck fSave\n"
          'SetFunctionValue (sv_Construct "%g %g %g %g" (fA - fSave) (fB - fSave) (fC - fSave) ((rObj.GetBaseAV Luck) - fSave))', True),
     _jg6("karma.alt", "let fSave := rP.GetAV Karma\nRewardKarmaAlt 1\nlet fA := rP.GetAV Karma\nRewardKarmaAlt -1\n"
          'SetFunctionValue (sv_Construct "%g %g" (fA - fSave) ((rP.GetAV Karma) - fSave))', True)]
    # 2. actor states
    + [_jg6("life.player", "SetFunctionValue (rP.GetLifeState)"),
       _jg6v("life.doc", "docref", "rObj.GetLifeState"),
       _jg6v("dialogue.doc", "docref", "rObj.IsInDialogueWithPlayer"),
       # GetCurrentFurnitureRef is registered without a return type (nvse.log: RegisterCommand,
       # where the form getters that compiled are RegTypedCommand), so its result is a number
       # GetFormIDString does not take [observed 2026-11-08: did not compile]; `set` keeps it.
       _jg6("furniture.player", "set rObj to rP.GetCurrentFurnitureRef\nSetFunctionValue (GetFormIDString rObj)", True),
       _jg6("healthdamage.player", "SetFunctionValue (rP.HasHealthDamageEffect)"),
       _jg6("tempingestible.size", "let aList := GetTempIngestibleEffects\nSetFunctionValue (ar_Size aList)"),
       _jg6("perks.available", "let aList := GetAvailablePerks\nSetFunctionValue (ar_Size aList)"),
       _jg6("shaders.player", "let aList := rP.GetPlayingEffectShaders\nSetFunctionValue (ar_Size aList)"),
       _jg6("minearmedex.player", "SetFunctionValue (rP.GetMineArmedEx)")]
    # 3. cells: Doc's house and the schoolhouse visited, expired; the player's own cell
    + [_jg6v(f"cell.visited.{k}", k, "IsCellVisited rObj") for k in ("dochouse", "schoolhouse")]
    + [_jg6v(f"cell.expired.{k}", k, "IsCellExpired rObj") for k in ("dochouse", "schoolhouse")]
    + [_jg6("cell.expired.here", "SetFunctionValue (IsCellExpired (rP.GetParentCell))"),
       _jg6("cell.buffered", 'SetFunctionValue (sv_Construct "%g %g" (ar_Size (GetBufferedCellsAlt 0)) (ar_Size (GetBufferedCellsAlt 1)))', True)]
    # 4. lighting templates: a cell on record or not; set to Doc's house, read, put back
    + [_jg6v(f"ltmp.cell.{k}", k, "GetLightingTemplateCell rObj", "form") for k in ("novacltmp", "repconnltmp")]
    + [_jg6("ltmp.set", _form("rObj", JG6["novacltmp"]) + _form("rOther", JG6["dochouse"])
            + "let rItem := GetLightingTemplateCell rObj\nSetLightingTemplateCell rObj rOther\nlet sText := GetFormIDString (GetLightingTemplateCell rObj)\n"
            'SetLightingTemplateCell rObj rItem\nSetFunctionValue (sv_Construct "%z %z" sText (GetFormIDString (GetLightingTemplateCell rObj)))', True)]
    # 5. the moon, the frame time (as > 0), radios, talking activators
    + [_jg6("moon", "SetFunctionValue (GetMoonPhase)"),
       _jg6("secondspassed", "SetFunctionValue ((GameGetSecondsPassed) > 0)"),
       _jg6("radios", "let aList := GetAllGameRadios\nlet fA := ar_Size aList\nlet fB := 0\nlet fC := 0\nwhile fB < fA\n"
            "let rObj := aList[fB]\nif eval (GetFormIDString (rObj.GetBaseObject)) == \"0014DF02\"\nlet fC += 1\nendif\nlet fB += 1\nloop\n"
            'SetFunctionValue (sv_Construct "%g %g" fA fC)', True),
       _jg6v("tact.actor.rnv", "rnv", "GetTalkingActivatorActor rObj", "form")]
    # 6. a reference's prompt override, set; the couch trigger's primitive. A string
    # argument takes no `$sSave` here [observed 2026-11-08: jg6.prompt.door did not compile,
    # as jip8's trial], so the door keeps "NvfProbe" for the session; nothing after reads it.
    + [_jg6("prompt.door", _form("rObj", JG6["docdoor"]) + "let sSave := rObj.GetRefActivationPromptOverride\n"
            'rObj.SetRefActivationPromptOverride "NvfProbe"\n'
            'SetFunctionValue (sv_Construct "[%z] [%z]" sSave (rObj.GetRefActivationPromptOverride))', True),
       _jg6v("primitive.couch", "couchtrigger", "rObj.GetPrimitiveType"),
       _jg6v("primitive.door", "docdoor", "rObj.GetPrimitiveType")]
    # 7. calls per reference, counted in an array the lambda appends to: the NPCs (42) of
    # Doc's house, every reference of it against GetRefsInCell, the high-level actors
    # against GetActorsByProcessingLevel 0. The lambdas are `Begin Function` blocks: the
    # arrow form `({ref r} => ar_Append aList r)` did not compile here [observed 2026-11-08,
    # all three rows; hypothesis: the arrow form or the one-letter name, not isolated].
    + [_jg6("callperref.npc", _form("rObj", JG6["dochouse"]) + 'let aList := ar_Construct "array"\n'
            "CallPerRef (Begin Function {ref rA}\nar_Append aList rA\nEnd) 42 0 0 rObj\nSetFunctionValue (ar_Size aList)"),
       _jg6("callperref.all", _form("rObj", JG6["dochouse"]) + 'let aList := ar_Construct "array"\n'
            "CallPerRef (Begin Function {ref rA}\nar_Append aList rA\nEnd) 0 0 0 rObj\n"
            'SetFunctionValue (sv_Construct "%g %g" (ar_Size aList) (ar_Size (GetRefsInCell rObj 0)))', True),
       _jg6("callpermobile.high", 'let aList := ar_Construct "array"\nCallPerMobileObject (Begin Function {ref rA}\nar_Append aList rA\nEnd) 0\n'
            'SetFunctionValue (sv_Construct "%g %g" (ar_Size aList) (ar_Size (GetActorsByProcessingLevel 0)))', True)]
)


# pp (writes, session only, each change put back where the plugin allows; #1014): ppNVSE
# (docs/spec-notes/ext-pp.md), installed as VanillaPluginsPlus.dll, on FalloutNV.esm forms
# [observed: fnvtool esm inventory; fnvtool dialogue lines]: the baseball bat, the 10mm
# pistol and its silencer, two of Doc Mitchell's lines (one with a 27-character prompt, one
# without). Read-only probes first. Left out after the first runs: AX_Exists with no form
# (the game crashed on it), and what did not compile in the game: HasFormTrait with the
# catalogue's index-first arguments, AX_Find without the deep flag, CreateFormInstance
# without a key, and a literal -1 argument (the AX probes pass -1 in a variable). The AX keys are dropped at the end; the form instance
# made here cannot be deleted (the plugin has no such script function) and lives for the
# session only; pp.file.write writes nvfpp_probe.txt into the game's folder (in-game probes may
# write the base install; NVForever writes its own layer). Not probed: CallLoop and the
# instance and item-mod handlers (they run on later frames or on a load),
# ReplaceItemInInventory (needs an inventory reference), the Devkit-only setters, and the
# six names the DLL does not register (EvaluateString, GetAllAttachedWeaponMods,
# GetAllModsInSlot, GetWeaponBase, IsWeaponInstance, NewWeaponInstance).
PP = {"bat": "421C", "pistol": "434F", "silencer": "129878", "info": "10749E", "info.none": "105570"}


def _pp(pid, body, text=False):
    return (f"pp.{pid}", "ref rP\nref rObj\nref rOther\nref rInst\nfloat fRes\nfloat fA\nfloat fB\nfloat fC\n"
            "string_var sText\narray_var aList\nBegin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


def _ppv(pid, key, expr, text=False):
    return _pp(pid, _form("rObj", PP[key]) + f"SetFunctionValue ({expr})", text)


# The instance pp.inst makes, kept under an AX key on the player for the probes after it.
_INST = 'let rInst := AX_Get "nvfppinst" 0 rP\n'

PP_PROBES = [
    # ---- read-only ----
    _pp("third", "SetFunctionValue (IsThirdPerson)"),
    _pp("sight", "SetFunctionValue (GetCurrentSightingNode)", True),
    _ppv("static.bat", "bat", "GetFormIDString (GetStaticForm rObj)", True),
    _ppv("isinst.bat", "bat", "IsFormInstance rObj"),
    _ppv("hasext.pistol", "pistol", "HasExtendedMods rObj"),
    _ppv("hasext.pistol.vanilla", "pistol", "HasExtendedMods rObj 1"),
    _pp("allmods.pistol", _form("rObj", PP["pistol"]) + "let aList := GetAllAttachedMods rObj\n"
        "SetFunctionValue (ar_Size aList)"),
    _ppv("akimbo.pistol", "pistol", "IsAkimboForm rObj"),
    _pp("akimboset.pistol", _form("rObj", PP["pistol"]) + "SetFunctionValue (HasAkimboSet rObj rObj)"),
    _ppv("trait.pistol", "pistol", 'GetFormTrait 0 rObj "MoveNode"'),
    _ppv("prompt", "info", "sv_Length (GetTopicPrompt rObj)"),
    _ppv("prompt.none", "info.none", "sv_Length (GetTopicPrompt rObj)"),
    # The hypothesis (ext-pp.md §4): the camera looks where the player does; the
    # differences from the player's angles, and the roll.
    _pp("camrot", "GetPlayerCameraRotation fA fB fC\n"
        'SetFunctionValue (sv_Construct "%.0f %.0f %.0f" (fA - (rP.GetAngle X)) fB (fC - (rP.GetAngle Z)))', True),
    _ppv("ammo.bat", "bat", "GetFormIDString (rP.FindWeaponAmmo rObj)", True),
    _pp("readfile", 'let aList := pReadFile "Fallout_default.ini"\nSetFunctionValue (ar_Size aList)'),
    _pp("readfile.lines", 'let aList := pReadFile "Fallout_default.ini" 0 3\nSetFunctionValue (ar_Size aList)'),
    _pp("ax.none", _form("rObj", PP["bat"]) + 'SetFunctionValue (sv_Construct "%g %g %g" (AX_Exists "nvfppnone" rObj) '
        '(AX_GetType "nvfppnone" 0 rObj) (AX_Size "nvfppnone" rObj))', True),
    # ---- writing ----
    _pp("ax.set", _form("rObj", PP["bat"]) + 'AX_Set 777 "nvfpp" 0 rObj\nAX_Set "Hello" "nvfpp" 1 rObj\n'
        'let aList := ar_List 4 5 6\nAX_Set aList "nvfpp" 2 rObj\nSetFunctionValue (AX_Size "nvfpp" rObj)'),
    _ppv("ax.get0", "bat", 'AX_Get "nvfpp" 0 rObj'),
    _pp("ax.get1", _form("rObj", PP["bat"]) + 'let sText := AX_Get "nvfpp" 1 rObj\nSetFunctionValue sText', True),
    _pp("ax.get2", _form("rObj", PP["bat"]) + 'let aList := AX_Get "nvfpp" 2 rObj\nSetFunctionValue aList[1]'),
    _ppv("ax.types", "bat", 'sv_Construct "%g %g %g %g" (AX_GetType "nvfpp" 0 rObj) (AX_GetType "nvfpp" 1 rObj) '
         '(AX_GetType "nvfpp" 2 rObj) (AX_GetType "nvfpp" 7 rObj)', True),
    _ppv("ax.exists", "bat", 'sv_Construct "%g %g" (AX_Exists "nvfpp" rObj) (AX_Exists "NVFPP" rObj)', True),
    _ppv("ax.find", "bat", 'sv_Construct "%g %g %g" (AX_Find "hello" "nvfpp" 0 rObj) (AX_Find 5 "nvfpp" 0 rObj) '
         '(AX_Find 5 "nvfpp" 1 rObj)', True),
    _pp("ax.null", _form("rObj", PP["bat"]) + 'let fRes := AX_Null "nvfpp" 0 rObj\n'
        'SetFunctionValue (sv_Construct "%g %g %g" fRes (AX_GetType "nvfpp" 0 rObj) (AX_Size "nvfpp" rObj))', True),
    _pp("ax.erase", _form("rObj", PP["bat"]) + 'let fRes := AX_Erase "nvfpp" 0 rObj\n'
        'SetFunctionValue (sv_Construct "%g %g %g" fRes (AX_Size "nvfpp" rObj) (AX_GetType "nvfpp" 0 rObj))', True),
    # A literal -1 argument did not compile in the game (run 2): the index from a variable.
    _pp("ax.drop", _form("rObj", PP["bat"]) + 'let fA := -1\nlet fRes := AX_Erase "nvfpp" fA rObj\n'
        'SetFunctionValue (sv_Construct "%g %g" fRes (AX_Exists "nvfpp" rObj))', True),
    _ppv("edid.pistol", "pistol", "GetEditorID rObj", True),
    _pp("inst", _form("rObj", PP["pistol"]) + 'let rInst := CreateFormInstance rObj "nvfpp"\nAX_Set rInst "nvfppinst" 0 rP\n'
        'SetFunctionValue (sv_Construct "%g %z %z" (IsFormInstance rInst) (GetFormIDString (GetStaticForm rInst)) '
        '(GetEditorID rInst))', True),
    _pp("inst.mod", _INST + _form("rObj", PP["pistol"])
        + 'SetFunctionValue (sv_Construct "%z %z" (GetFormIDString (GetWeaponItemMod 1 rInst)) '
        "(GetFormIDString (GetWeaponItemMod 1 rObj)))", True),
    # A slot name no Devkit file defines.
    _pp("attach", _INST + _form("rObj", PP["silencer"]) + 'let fRes := AttachMod rInst "nvfslot" rObj\n'
        "let fA := HasExtendedMods rInst\nlet aList := GetAllAttachedMods rInst\nlet fB := ar_Size aList\n"
        'AttachMod rInst "nvfslot"\nlet aList := GetAllAttachedMods rInst\n'
        'SetFunctionValue (sv_Construct "%g %g %g %g %g" fRes fA fB (HasExtendedMods rInst) (ar_Size aList))', True),
    # A vanilla slot on the instance (session only).
    _pp("attach.vanilla", _INST + 'let fRes := AttachMod rInst "1"\nlet aList := GetAllAttachedMods rInst\n'
        'SetFunctionValue (sv_Construct "%g %g" fRes (ar_Size aList))', True),
    _pp("inst.drop", 'let fA := -1\nSetFunctionValue (AX_Erase "nvfppinst" fA rP)'),
    _pp("sight.set", 'SetSightingNode "Bip01 Head"\nlet sText := GetCurrentSightingNode\n'
        'SetSightingNode "##SightingNode"\nSetFunctionValue (sv_Construct "%z %z" sText (GetCurrentSightingNode))', True),
    # Run 2: the default name did not take; another name does?
    _pp("sight.set2", 'SetSightingNode "NVFNodeTwo"\nSetFunctionValue (GetCurrentSightingNode)', True),
    _pp("file.write", 'let aList := ar_List "nvf one" "nvf two"\nSetFunctionValue (pWriteFile "nvfpp_probe.txt" aList)'),
    _pp("file.read", 'let aList := pReadFile "nvfpp_probe.txt"\nlet sText := aList[1]\n'
        'SetFunctionValue (sv_Construct "%g %z" (ar_Size aList) sText)', True),
]


# format1 (read-only; #1006): how sv_Construct writes %g, %G and %e in the game (the wiki's
# "%g as an integer, 1E+006" is the older runtime's; ext-format.md §2), and two texture
# sets' flags (jip5's open row).
_F1_G = (0.5, 2.5, 1000000, 123.456789, None, 1234567, 0.1, 100000, 42, None, 3.14159265)


def _f1(pid, body):
    return (pid, "ref rObj\nfloat fRes\nBegin Function { }\n" + body, True, None, "")


FORMAT1_PROBES = (
    [_f1(f"format.g.{i}", f'SetFunctionValue (sv_Construct "%g" {v!r})') for i, v in enumerate(_F1_G) if v is not None]
    + [_f1("format.e.0", 'SetFunctionValue (sv_Construct "%.3e" 123000)'),
       _f1("format.e.1", 'SetFunctionValue (sv_Construct "%e" 0.5)'),
       _f1("format.G", 'SetFunctionValue (sv_Construct "%G" 1234567)'),
       _f1("format.g2", 'SetFunctionValue (sv_Construct "%.2g" 123.456)'),
       _f1("txst.dirt.dnam", 'let rObj := GetFormFromMod "FalloutNV.esm" "11673A"\nSetFunctionValue (sv_Construct "%g %g %g %g" '
           '(GetTextureSetFlag rObj 0) (GetTextureSetFlag rObj 1) (GetTextureSetFlag rObj 2) (GetTextureSetFlag rObj 3))'),
       _f1("txst.other", 'let rObj := GetFormFromMod "FalloutNV.esm" "782D2"\nSetFunctionValue (sv_Construct "%g %g" '
           '(GetTextureSetFlag rObj 0) (GetTextureSetFlag rObj 1))')]
)


# tmpl2 (read-only; #1057): the cases tmpl1 left open [observed: FalloutNV.esm NPC_ scan].
# No NPC_ there takes Traits from an NPC with a class other than its own; one takes them from
# an NPC with another race: PrimmNCRTrooper3 (own RNAM 0x424A, PrimmNCRTrooper1's 0x19).
# Packages, each using AI Packages from an NPC with another list: a Brotherhood paladin with
# no PKID of its own (template 5) and one with 6 of its own (template 5), FortChildHM (1 vs 2),
# 1ESharecropperCF (5 vs 3); and the template paladin itself.
TM2 = {"trooper3": "099D05", "paladin0": "0E2966", "paladin6": "0E2960", "child": "11E507",
       "sharecropper": "0F6403", "paladintmpl": "15E581"}


def _tm2(pid, key, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return (f"tmpl2.{pid}", "ref rObj\narray_var aA\nfloat fRes\nBegin Function { }\n" + _form("rObj", TM2[key]) + rd,
            bool(text), None, "")


TMPL2_PROBES = [
    _tm2("trooper3.race", "trooper3", "GetRace rObj", "form"),
    _tm2("trooper3.class", "trooper3", "GetClass rObj", "form"),
    *[p for k in ("paladin0", "paladin6", "child", "sharecropper", "paladintmpl") for p in (
        _tm2(f"{k}.packages", k, "ar_Size (GetBasePackages rObj)"),
        _tm2(f"{k}.package0", k, "(GetBasePackages rObj)[0]", "form"),
    )],
]

# tmpl3 (#1057): factions and spells where the record's own list and its NPC template's
# differ (tmpl1's NPC templates all carried the record's own SNAM, so it could not tell)
# [observed: load-order scan, 842 actors for factions and 345 for spells]. Raider1MeleeAM
# (own 3 SNAM, Use Factions from Raider1MeleeAAFTEMPLATE with 5); SniperShackRadroach01 (own
# 1, CrRadroach 2); CrMisterGutsyEnclave (own 1 SPLO, Use Actor Effect List from CrMisterGutsy
# with 3); Shakes (1, CrProtectron 3); FFEU02DeathClaw (0, CrDeathClaw 1); CrFeralGhoul2B
# (2, CrFeralGhoul2A's other 2). Then a reference whose Traits come from a leveled list:
# LvlRaiderRandomMale (own RNAM 0x19; every NPC its list can give has race 0x0004BF72)
# placed beside the player, read and removed (session only).
TM3 = {"raider": "0295FA", "roach": "01FEC8", "gutsy": "021EBE", "shakes": "017A03", "claw": "031E29",
       "feral": "01CF77", "lvlraider": "07E40E"}
_H3 = ("ref rPlayer\nref rObj\nref rF\narray_var aA\nfloat fRes\nBegin Function { }\n" + _form("rPlayer", NF["player"]))
_AUX3 = 'let rF := rPlayer.AuxVarGetRef "*nvfT3"\n'


def _tm3(pid, key, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return (f"tmpl3.{pid}", _H3 + _form("rObj", TM3[key]) + rd, bool(text), None, "")


def _tm3ref(pid, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return (f"tmpl3.{pid}", _H3 + _AUX3 + rd, bool(text), None, "")


TMPL3_PROBES = [
    *[p for k in ("raider", "roach") for p in (
        _tm3(f"{k}.factions", k, "GetBaseNumFactions rObj"),
        _tm3(f"{k}.faction0", k, "GetBaseNthFaction 0 rObj", "form"),
        _tm3(f"{k}.rank0", k, "GetBaseNthRank 0 rObj"),
        _tm3(f"{k}.basefactions", k, "ar_Size (GetBaseFactions rObj)"),
    )],
    *[p for k in ("gutsy", "shakes", "claw", "feral") for p in (
        _tm3(f"{k}.spells", k, "ar_Size (GetBaseSpellListSpells rObj)"),
        _tm3(f"{k}.spell0", k, "(GetBaseSpellListSpells rObj)[0]", "form"),
    )],
    ("tmpl3.lvlraider.place", _H3 + _form("rObj", TM3["lvlraider"]) + 'let rF := rPlayer.PlaceAtMe rObj 1\n'
     'rPlayer.AuxVarSetRef "*nvfT3" rF\nSetFunctionValue 1', False, None, ""),
    _tm3ref("lvlraider.race", "GetRace rF", "form"),
    _tm3ref("lvlraider.class", "GetClass rF", "form"),
    _tm3ref("lvlraider.base", "rF.GetBaseObject", "form"),
    _tm3ref("lvlraider.baserace", "GetRace (rF.GetBaseObject)", "form"),
    _tm3("lvlraider.recordrace", "lvlraider", "GetRace rObj", "form"),
    ("tmpl3.lvlraider.cleanup", _H3 + _AUX3 + 'rF.Disable\nrF.MarkForDelete\nrPlayer.AuxVarErase "*nvfT3"\nSetFunctionValue 1',
     False, None, ""),
]

# jipppln (#1019): JIP++ (JIP PP LN), the 10 functions it adds to JIP LN
# (docs/spec-notes/ext-jippp.md). JIP++ replaces jip_nvse.dll, so the job runs with the
# plugin swapped in for the run and base JIP LN put back afterwards (never left
# installed). FalloutNV.esm forms [observed: fnvtool esm inventory]: the 10mm pistol and
# its rounds, the combat knife, the .308 JSP hand loads, the .357 revolver and its
# rounds. The runtime node lists start empty in a fresh session; every step that adds to
# one clears it again (ClearRuntimeNodes) in the same step.
JPP = {"pistol": "434F", "ammo10mm": "4241", "knife": "4326", "jsp308": "140AA0", "revolver": "8F216",
       "ammo357": "8ED02"}


def _jpp(pid, body, text=False):
    return (f"jpp.{pid}", "ref rP\nref rDoc\nref rObj\nref rOther\nref rBox\nfloat fRes\nfloat fN\nstring_var sText\n"
            "array_var aA\narray_var aE\nBegin Function { }\n"
            + _form("rP", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


_JPP_SPEC = '"Bip01|nvfNode"'
JIPPPLN_PROBES = (
    # ---- read-only ----
    [_jpp("has.fresh", _form("rObj", JPP["pistol"]) + 'SetFunctionValue (HasRuntimeNode rObj "nvfNode")'),
     _jpp("clear.fresh", _form("rObj", JPP["pistol"]) + "SetFunctionValue (ClearRuntimeNodes rObj)")]
    + [_jpp(f"degrade.{w}.{a}", _form("rObj", JPP[w]) + _form("rOther", JPP[a]) + "SetFunctionValue (GetCalculatedWeaponDegradation rObj rOther)")
       for w, a in (("pistol", "ammo10mm"), ("revolver", "ammo357"), ("pistol", "jsp308"))]
    + [_jpp("degrade.pistol", _form("rObj", JPP["pistol"]) + "SetFunctionValue (GetCalculatedWeaponDegradation rObj)")]
    + [_jpp(f"control.{c}.{t}", f"SetFunctionValue (GetControlCodeByID {c} {t})") for c in (0, 4, 5, 7) for t in (0, 1, 2, 3)]
    + [_jpp("effects.player.ingest", "let aA := rP.GetActiveEffectsInfo 3\nSetFunctionValue (ar_Size aA)"),
       _jpp("effects.doc", "let aA := rDoc.GetActiveEffectsInfo\nSetFunctionValue (ar_Size aA)"),
       _jpp("effects.doc.temp", "let aA := rDoc.GetActiveEffectsInfo 0 1\nSetFunctionValue (ar_Size aA)"),
       _jpp("refreshweight", "SetFunctionValue (rP.RefreshInventoryWeight)")]
    # ---- writing, each put back in the same step ----
    + [_jpp("insert.has", _form("rObj", JPP["pistol"]) + f"InsertNode rObj 1 {_JPP_SPEC}\n"
            f'let sText := sv_Construct "%g %g %g %g" (HasRuntimeNode rObj {_JPP_SPEC} 1) (HasRuntimeNode rObj {_JPP_SPEC} 2) '
            '(HasRuntimeNode rObj "nvfNode") (HasRuntimeNode rObj "nvfnode")\n'
            'let fRes := ClearRuntimeNodes rObj\n'
            'SetFunctionValue (sv_Construct "%z %g %g" sText fRes (HasRuntimeNode rObj "nvfNode"))', True),
       _jpp("copy", _form("rObj", JPP["pistol"]) + _form("rOther", JPP["knife"]) + f"InsertNode rObj 1 {_JPP_SPEC}\n"
            "let fRes := CopyRuntimeNodes rOther rObj 0\n"
            'let sText := sv_Construct "%g %g" fRes (HasRuntimeNode rOther "nvfNode")\n'
            "let fN := CopyRuntimeNodes rOther rObj 1\n"
            'let sText := sv_Construct "%z %g %g" sText fN (ClearRuntimeNodes rOther)\n'
            'SetFunctionValue (sv_Construct "%z %g %g" sText (ClearRuntimeNodes rObj) (HasRuntimeNode rOther "nvfNode"))', True),
       _jpp("attachform", _form("rObj", JPP["pistol"]) + _form("rOther", JPP["knife"])
            + 'let fRes := AttachFormModel rObj rOther 1 "Bip01"\n'
            'let sText := sv_Construct "%g %g %g" fRes (HasRuntimeNode rObj "Bip01|") (HasRuntimeNode rObj "Bip01|" 2)\n'
            'let fRes := AttachFormModel rObj rOther 0 "Bip01"\n'
            'let sText := sv_Construct "%z %g %g" sText fRes (HasRuntimeNode rObj "Bip01|")\n'
            'let fRes := AttachFormModel rObj rOther 0 "Bip01"\n'
            'SetFunctionValue (sv_Construct "%z %g %g" sText fRes (ClearRuntimeNodes rObj))', True),
       # SetActiveEffectsInfo with the array GetActiveEffectsInfo gave: nothing changes.
       _jpp("effects.setsame", "let aA := rDoc.GetActiveEffectsInfo\nlet fRes := rDoc.SetActiveEffectsInfo aA\n"
            'SetFunctionValue (sv_Construct "%g %g" fRes (ar_Size aA))', True),
       # A placed pistol, clamped and taken away again.
       _jpp("clamp", _form("rObj", JPP["pistol"]) + "let rBox := rP.PlaceAtMe rObj 1\nlet fRes := rBox.ClampToGround\n"
            "rBox.Disable\nrBox.MarkForDelete\nSetFunctionValue fRes"),
       _jpp("clamp.base", _form("rObj", JPP["pistol"]) + "SetFunctionValue (rObj.ClampToGround)")]
)

# itr (writes, session only, each change put back; #1011): the ITR plugin's functions
# (docs/spec-notes/ext-itr.md) on FalloutNV.esm forms [observed: fnvtool esm inventory]:
# Doc Mitchell's and Sunny's bases, the Caucasian race and eyes, the casinos, the Mojave
# worldspace, two greeting lines, GREETING and GOODBYE, perks, the 10mm pistol, the dog.
# Read-only steps first; each writing step saves what it changes and puts it back in the
# same step. Settings ITR keeps "in static memory" (fall damage) are cleared by the step.
ITR = {"doc": "104C0C", "sunnyref": "104E85", "caucasian": "19", "eyeraider": "4CB10", "eyeblue": "4253",
       "wasteland": "DA726", "info1": "104E7E", "info2": "104E7F", "greeting": "C8", "goodbye": "D4",
       "toughness": "31DE0", "actionboy": "31DBA", "wildwasteland": "ED568", "intense": "44CB1",
       "commando": "99828", "gunslinger": "94EBB", "pistol": "434F", "dog": "14F425", "stimpak": "15169",
       "tops": "144C3F", "ultraluxe": "144C40", "gomorrah": "140521", "wrangler": "144C42", "vikki": "144C41"}


def _itr(pid, body, text=False):
    return (f"itr.{pid}", "ref rP\nref rDoc\nref rObj\nref rOther\nfloat fRes\nfloat fSave\nfloat fA\nfloat fB\nfloat fC\n"
            "string_var sText\narray_var aA\nBegin Function { }\n"
            + _form("rP", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


# one111 (#1012): the 1111 NVSE plugin (OneOneOneOne.dll, installed 2026-11-01;
# docs/spec-notes/ext-one111.md) on FalloutNV.esm forms [observed: fnvtool esm
# inventory]: the Brahmin Wellington recipe, HitReactionIdles, Sunny's sleep package,
# GomorraElevatorMessage, the companion dismissal terminal, the Stealth Boy, the Nerd
# Rage! spell and perk, Run Goodsprings Run, the 10mm pistol, the varmint and sniper
# rifles, 5mm hollow point rounds and their DT effect, fxCateye. Read-only getters first;
# every writing step saves what it changes and puts it back in the same step.
ONE = {"recipe": "175AAA", "idle": "14F54", "package": "176AD3", "message": "10DDBD", "terminal": "17B7A0",
       "stealthboy": "43E94", "nerdspell": "44CA4", "nerdperk": "44CA7", "quest": "15EC5B", "pistol": "434F",
       "varmint": "7EA24", "sniper": "4353", "ammohp": "1613FF", "amefhp": "17BA47", "cateye": "15FF5F"}


def _one(pid, body, text=False):
    return (f"one.{pid}", "ref rP\nref rDoc\nref rObj\nref rOther\nref rSave\nfloat fRes\nfloat fSave\nfloat fN\n"
            "string_var sText\narray_var aA\nBegin Function { }\n"
            + _form("rP", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _itrv(pid, key, expr, text=False):
    return _itr(pid, _form("rObj", ITR[key]) + f"SetFunctionValue ({expr})", text)


ITR_PROBES = (
    # Strings (no page for these three: ext-itr.md §1).
    [_itr("sv.join", 'let aA := ar_List "a" "b" "c"\nSetFunctionValue (Sv_Join aA "-")', True),
     _itr("sv.join.nodelim", 'let aA := ar_List "a" "b" "c"\nSetFunctionValue (Sv_Join aA)', True),
     _itr("sv.reverse", 'SetFunctionValue (Sv_Reverse "abc d")', True),
     _itr("sv.trim", 'let sText := Sv_TrimStr "  ab c  "\nSetFunctionValue (sv_Construct "[%z]" sText)', True)]
    # Switches and settings as the save leaves them.
    + [_itr("fall.global", "SetFunctionValue (GetFallDamageMult)"),
       _itr("fall.player", "SetFunctionValue (rP.GetFallDamageMult)"),
       _itr("noweaponsearch.doc", "SetFunctionValue (rDoc.GetNoWeaponSearch)"),
       _itr("preventswitch.doc", "SetFunctionValue (rDoc.GetPreventWeaponSwitch)"),
       _itr("contactwatch.doc", "SetFunctionValue (rDoc.GetOnContactWatch)"),
       _itr("alertns.doc", "SetFunctionValue (rDoc.GetAlertNS)"),
       _itr("aimzoom", "SetFunctionValue (GetAimZoomFirstPersonOnly)"),
       _itr("radio.queuesize", "SetFunctionValue (GetRadioQueueSize)"),
       _itr("dialogue.displayed", "let aA := GetDisplayedDialogueInfos\nSetFunctionValue (ar_Size aA)"),
       _itr("issaying.doc", _form("rObj", ITR["greeting"]) + "SetFunctionValue (rDoc.IsSaying rObj)"),
       _itr("rigidbody.player", "SetFunctionValue (rP.IsRigidBodyAtRest)")]
    # Records.
    + [_itr(f"hair.doc.{c}", f'SetFunctionValue (sv_Construct "%.0f" (rDoc.GetHairColorAlt {c}))', True) for c in range(4)]
    + [_itrv(f"hair.sunny.{c}", "sunnyref", f'sv_Construct "%.0f" (rObj.GetHairColorAlt {c})', True) for c in (0, 1)]
    + [_itrv(f"casino.{k}", k, "GetCasinoBan rObj") for k in ("tops", "ultraluxe", "gomorrah", "wrangler", "vikki")]
    + [_itrv("casino.notcasino", "doc", "GetCasinoBan rObj")]
    + [_itrv(f"wrld.{n}", "wasteland", f"GetWorldspaceOffset{s} rObj") for n, s in (("scale", "Scale"), ("x", "X"), ("y", "Y"))]
    + [_itrv(f"infoflags.{k}", k, "GetDialogueInfoFlags rObj") for k in ("info1", "info2")]
    # References and doors (the save stands outdoors, Doc indoors).
    + [_itr("exteriordoor.player", "SetFunctionValue (GetFormIDString (rP.GetRefExteriorDoor))", True),
       _itr("exteriordoor.doc", "SetFunctionValue (GetFormIDString (rDoc.GetRefExteriorDoor))", True),
       _itr("nextdoor.doc", "SetFunctionValue (GetFormIDString (rDoc.GetRefNextTeleportDoor))", True),
       _itr("navmesh.player", "SetFunctionValue (IsPointOnNavmesh (rP.GetPos X) (rP.GetPos Y) (rP.GetPos Z))"),
       _itr("cover.zero", "let aA := GetCoverPointsInRadius (rP.GetPos X) (rP.GetPos Y) (rP.GetPos Z) 0\nSetFunctionValue (ar_Size aA)")]
    # Perks: what the records give, then what depends on the player.
    + [_itr(f"perkcond.{k}.size", _form("rObj", ITR[k]) + "let aA := GetPerkConditions rObj\nSetFunctionValue (ar_Size aA)")
       for k in ("toughness", "actionboy", "wildwasteland", "intense", "commando", "gunslinger")]
    + [_itr(f"perkcond.toughness.0.{f}", _form("rObj", ITR["toughness"]) + f'let aA := GetPerkConditions rObj\nSetFunctionValue (aA[0]["{f}"])', t)
       for f, t in (("function", True), ("comparison", True), ("comparisonValue", False), ("kind", True), ("source", True))]
    + [_itr(f"perkelig.toughness.{f}", _form("rObj", ITR["toughness"]) + f'let aA := GetPerkEligibility rObj\nSetFunctionValue (aA["{f}"])', t)
       for f, t in (("name", True), ("maxRank", False), ("ownedRank", False), ("nextRank", False), ("eligible", False), ("status", True))]
    + [_itr(f"perkelig.{k}.status", _form("rObj", ITR[k]) + 'let aA := GetPerkEligibility rObj\nSetFunctionValue (aA["status"])', True)
       for k in ("wildwasteland", "actionboy", "commando")]
    + [_itr("perkblockers.toughness.size", _form("rObj", ITR["toughness"]) + "let aA := GetPerkBlockers rObj\nSetFunctionValue (ar_Size aA)"),
       _itr("perksforform.pistol.size", _form("rObj", ITR["pistol"]) + "let aA := GetPerksForForm rObj\nSetFunctionValue (ar_Size aA)"),
       _itr("eligibleperks.size", "let aA := GetEligiblePerks\nSetFunctionValue (ar_Size aA)")]
    # Writing steps, each put back.
    + [_itr("fall.set", "let fA := SetFallDamageMult 2\nlet fB := rP.SetFallDamageMult 0.5\n"
            "let sText := sv_Construct \"%g %g %g %g\" fA fB (GetFallDamageMult) (rP.GetFallDamageMult)\n"
            "SetFallDamageMult -3\nlet fC := GetFallDamageMult\nClearFallDamageMult\n"
            'SetFunctionValue (sv_Construct "%z %g %g %g" sText fC (GetFallDamageMult) (rP.GetFallDamageMult))', True),
       _itr("switches.doc", "let fA := rDoc.SetNoWeaponSearch 1\nlet fB := rDoc.GetNoWeaponSearch\nrDoc.SetNoWeaponSearch 0\n"
            "let sText := sv_Construct \"%g %g %g\" fA fB (rDoc.GetNoWeaponSearch)\n"
            "let fA := rDoc.SetPreventWeaponSwitch 1\nlet fB := rDoc.GetPreventWeaponSwitch\nrDoc.SetPreventWeaponSwitch 0\n"
            "let sText := sv_Construct \"%z %g %g %g\" sText fA fB (rDoc.GetPreventWeaponSwitch)\n"
            "let fA := rDoc.SetOnContactWatch 1\nlet fB := rDoc.GetOnContactWatch\nrDoc.SetOnContactWatch 0\n"
            'SetFunctionValue (sv_Construct "%z %g %g %g" sText fA fB (rDoc.GetOnContactWatch))', True),
       _itr("aimzoom.set", "let fSave := GetAimZoomFirstPersonOnly\nlet fA := SetAimZoomFirstPersonOnly 1\nlet fB := GetAimZoomFirstPersonOnly\n"
            "let fC := SetAimZoomFirstPersonOnly 0\nSetAimZoomFirstPersonOnly fSave\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fA fB fC (GetAimZoomFirstPersonOnly))', True),
       _itr("alertns.set", "let fA := rDoc.SetAlertNS 1 10\nlet fB := rDoc.GetAlertNS\nrDoc.SetAlertNS 1\nlet fC := rDoc.GetAlertNS\n"
            "let fRes := rDoc.GetIsAlerted\nrDoc.SetAlertNS 0\n"
            'SetFunctionValue (sv_Construct "%g %.0f %g %g %g" fA fB fC fRes (rDoc.GetAlertNS))', True),
       _itr("hair.set", "let fA := rDoc.GetHairColorAlt 1\nlet fB := rDoc.GetHairColorAlt 2\nlet fC := rDoc.GetHairColorAlt 3\n"
            "let fRes := rDoc.SetHairColorAlt 300 20 30\n"
            "let sText := sv_Construct \"%g %g %g %g %.0f\" fRes (rDoc.GetHairColorAlt 1) (rDoc.GetHairColorAlt 2) (rDoc.GetHairColorAlt 3) (rDoc.GetHairColorAlt 0)\n"
            "rDoc.SetHairColorAlt fA fB fC\n"
            'SetFunctionValue (sv_Construct "%z | %.0f" sText (rDoc.GetHairColorAlt 0))', True),
       _itr("infoflags.set", _form("rObj", ITR["info1"]) + "let fSave := GetDialogueInfoFlags rObj\nlet fA := SetDialogueInfoFlags rObj 258\n"
            "let fB := GetDialogueInfoFlags rObj\nSetDialogueInfoFlags rObj fSave\n"
            'SetFunctionValue (sv_Construct "%g %g %g" fA fB (GetDialogueInfoFlags rObj))', True),
       _itr("combatskill.set", _form("rObj", ITR["dog"]) + "let fSave := GetCreatureCombatSkill rObj\nlet fA := SetCreatureCombatSkill 300 rObj\n"
            "let fB := GetCreatureCombatSkill rObj\nSetCreatureCombatSkill fSave rObj\n"
            'SetFunctionValue (sv_Construct "%g %g %g" fA fB ((GetCreatureCombatSkill rObj) == fSave))', True),
       # Mask groups: a key held by two groups stays disabled until both let go
       # [hypothesis, ext-itr.md §3]; EnableKeyEx without a mask; a code that is no key.
       _itr("keys.masks", "let fA := DisableKeyEx 87 1\nDisableKeyEx 87 2\nEnableKeyEx 87 1\nlet fB := IsKeyDisabled 87\n"
            "EnableKeyEx 87 2\nlet fC := IsKeyDisabled 87\n"
            "DisableKeyEx 87 4\nEnableKeyEx 87\nlet fRes := IsKeyDisabled 87\nEnableKeyEx 87\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g %g" fA fB fC fRes (DisableKeyEx 300))', True),
       _itr("radio.queue", 'let fA := QueueRadioTrack "nvf\\a.mp3"\nlet fB := QueueRadioTrack "nvf\\a.ogg"\n'
            'let fC := QueueRadioTrack "C:\\nvf\\b.ogg"\nlet fRes := GetRadioQueueSize\nlet fSave := ClearRadioQueue\n'
            'SetFunctionValue (sv_Construct "%g %g %g %g %g %g" fA fB fC fRes fSave (GetRadioQueueSize))', True),
       _itr("topics.rules", _form("rObj", ITR["greeting"]) + _form("rOther", ITR["goodbye"])
            + "let fA := SetDialogueTopicHidden rObj 1\nSetDialogueTopicHidden rOther 1\nlet fB := SetDialogueTopicOrder rObj 5\n"
            "let fC := SetDialogueTopicHidden rDoc 1\nlet fRes := ClearDialogueTopicOverrides\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g %g" fA fB fC fRes (ClearDialogueTopicOverrides))', True),
       _itr("topics.entry.closed", 'SetFunctionValue (AddDialogueTopicEntry "nvf" 5)'),
       _itr("dialoguecamera.range", 'SetFunctionValue (sv_Construct "%g %g %g" (SetDialogueCameraMode 9) '
            "(SetDialogueCameraFixedAngle 12) (SetDialogueCameraAngleEnabled 15 1))", True),
       _itr("raceeye.raider", _form("rObj", ITR["caucasian"]) + _form("rOther", ITR["eyeraider"])
            + "let fA := AddRaceEye rObj rOther\nlet fB := RemoveRaceEye rObj rOther\n"
            'SetFunctionValue (sv_Construct "%g %g" fA fB)', True),
       _itr("eyemesh.set", _form("rObj", ITR["eyeblue"]) + 'let fA := SetEyeMesh rObj "nvf\\eye.nif"\n'
            "let fB := ClearEyeMesh rObj\n"
            'SetFunctionValue (sv_Construct "%g %g %g" fA fB (ClearEyeMesh rObj))', True),
       _itr("tile.watch", 'let fA := WatchTileValue "HUDMainMenu" "_nvfnone"\nlet fB := WatchTileValue "HUDMainMenu" "visible"\n'
            "let fC := UnwatchTileValue fB\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fA (fB > 0) fC (UnwatchTileValue fB))', True),
       _itr("gesture.player", 'SetFunctionValue (sv_Construct "%g %g" (rP.Gesture "wave" 1 1) (rP.Gesture "Nod" 0.1 0.2))', True),
       _itr("emissive.clear", "SetFunctionValue (ClearWeaponEmissiveColor)"),
       _itr("cameraangle.release", "SetFunctionValue (SetCameraAngle 0 0 0 0)"),
       _itr("useaid.notaid", _form("rObj", ITR["pistol"]) + "SetFunctionValue (rP.UseAidItem rObj)")]
    # Last (a hang in them costs nothing before): which perks GetPerksForForm gives the
    # pistol and which blockers hold Wild Wasteland back (the first run differed:
    # ext-itr.md §13). One row per index, no loops and no if blocks (neither compiled
    # in the game's runtime compiler here); an index past the end is an error row.
    + [_itr(f"perkblockers.wildwasteland.kind.{i}", _form("rObj", ITR["wildwasteland"])
            + f'let aA := GetPerkBlockers rObj\nSetFunctionValue (aA[{i}]["kind"])', True) for i in range(2)]
    + [_itr(f"perksforform.pistol.perk.{i}", _form("rObj", ITR["pistol"])
            + f'let aA := GetPerksForForm rObj\nSetFunctionValue (GetFormIDString (aA[{i}]["perk"]))', True) for i in range(6)]
    + [_itr(f"perksforform.pistol.status.{i}", _form("rObj", ITR["pistol"])
            + f'let aA := GetPerksForForm rObj\nSetFunctionValue (aA[{i}]["status"])', True) for i in range(6)]
)


def _onev(pid, key, expr, text=False):
    return _one(pid, _form("rObj", ONE[key]) + f"SetFunctionValue ({expr})", text)


# FormOne lists, FormTwo (form first), FormThree (form first second): (key, the arguments
# after the form that name the list).
_ONE_LISTS = [("One", "recipe", ""), ("One", "idle", ""), ("One", "package", ""),
              ("Two", "message", " 0"), ("Two", "message", " 1"), ("Two", "terminal", " 0"),
              ("Two", "stealthboy", " 0"), ("Two", "nerdspell", " 0"),
              ("Three", "quest", " (-1) 0"), ("Three", "quest", " 0 0"), ("Three", "nerdperk", " (-1) 0"),
              ("Three", "nerdperk", " 0 0"), ("Three", "nerdperk", " 0 1")]


def _lid(fam, key, args):
    return f"{fam.lower()}.{key}" + args.replace(" (-1)", ".m1").replace(" ", ".")


ONE111_PROBES = (
    [_onev(f"count.{_lid(f, k, a)}", k, f"GetConditionForm{f}Count rObj{a}") for f, k, a in _ONE_LISTS]
    + [_onev("count.one.pistol", "pistol", "GetConditionFormOneCount rObj"),
       _onev("count.two.message.9", "message", "GetConditionFormTwoCount rObj 9")]
    # The first condition of the lists that have one, every value ID of each kind.
    + [_onev(f"int.{_lid(f, k, a)}.{i}", k, f"GetConditionForm{f}Int rObj{a} 0 {i}")
       for f, k, a in _ONE_LISTS[:5] for i in range(0, 6)]
    + [_onev(f"float.{_lid(f, k, a)}.{i}", k, f"GetConditionForm{f}Float rObj{a} 0 {i}")
       for f, k, a in _ONE_LISTS[:5] for i in range(0, 3)]
    # The form goes through a ref variable: the game compiles the call as a number, which
    # GetFormIDString refuses as its argument.
    + [_one(f"form.{_lid(f, k, a)}.{i}", _form("rObj", ONE[k]) + f"let rOther := GetConditionForm{f}Form rObj{a} 0 {i}\n"
            "SetFunctionValue (GetFormIDString rOther)", True)
       for f, k, a in _ONE_LISTS[:5] for i in range(0, 4)]
    + [_onev("int.one.recipe.9", "recipe", "GetConditionFormOneInt rObj 9 0")]
    + [_one("eval.one.recipe", _form("rObj", ONE["recipe"]) + "SetFunctionValue (EvalConditionListFormOne rObj rP rP)"),
       _one("eval.two.message.0", _form("rObj", ONE["message"]) + "SetFunctionValue (EvalConditionListFormTwo rObj 0 rP rP)"),
       _one("eval.three.quest", _form("rObj", ONE["quest"]) + "SetFunctionValue (EvalConditionListFormThree rObj (-1) 0 rP rP)")]
    # Outside a damage event.
    + [_one(f"damage.numb.{i}", f"SetFunctionValue (GetOnDamageCalcNumb {i})") for i in (0, 5)]
    + [_one("damage.addnumb.0", "SetFunctionValue (GetOnDamageCalcAdditionalNumb 0)"),
       _one("damage.flag", "SetFunctionValue (GetOnDamageCalcFlag)"),
       _one("damage.ref.0", "SetFunctionValue (GetFormIDString (GetOnDamageCalcRef 0))", True),
       _one("damage.setnumb", "SetFunctionValue (SetOnDamageCalcNumb 0 5)")]
    + [_onev("weap.flag", "pistol", "GetWeapAdditionalFlag rObj 0"),
       _onev("weap.vector.none", "pistol", "GetWeapAdditionalVectorValue rObj 0 1"),
       _onev("weap.vector.scale.none", "pistol", "GetWeapAdditionalVectorValueScale rObj 0 0.5"),
       _one("weap.vector.array.none", _form("rObj", ONE["pistol"]) + "let aA := GetWeapAdditionalVectorArray rObj 0\nSetFunctionValue (ar_Size aA)")]
    + [_one(f"armor.doc.{k}", f"SetFunctionValue (rDoc.GetActorTotalArmor{k})") for k in ("DR", "DT")]
    + [_onev(f"ammo.apply.{t}", "ammohp", f"ApplyAmmoEffects rObj {t} 10") for t in range(0, 6)]
    + [_onev(f"scope.{k}", k, "GetFormIDString (GetWeaponScopeEffectShader rObj)", True) for k in ("varmint", "sniper", "pistol")]
    + [_one("projammo.player", "SetFunctionValue (GetFormIDString (rP.GetProjectileRefAmmo))", True)]
    # ---- writing steps, each put back ----
    # A new condition at the end: GetVampire == 1 on the subject; then deleted.
    + [_one("add.one.recipe", _form("rObj", ONE["recipe"]) + "let fN := GetConditionFormOneCount rObj\n"
            "let fRes := AddConditionFormOne rObj\nlet sText := sv_Construct \"%g %g %g %g %g\" fRes (GetConditionFormOneCount rObj) "
            "(GetConditionFormOneInt rObj fN 0) (GetConditionFormOneFloat rObj fN 0) (GetConditionFormOneInt rObj fN 5)\n"
            "DeleteConditionFormOne rObj fN\nSetFunctionValue (sv_Construct \"%z %g\" sText (GetConditionFormOneCount rObj))", True),
       _one("add.one.recipe.at0", _form("rObj", ONE["recipe"]) + "let fSave := GetConditionFormOneInt rObj 0 0\n"
            "let fRes := AddConditionFormOne rObj 0\nlet sText := sv_Construct \"%g %g %g\" fRes (GetConditionFormOneInt rObj 0 0) "
            "(GetConditionFormOneInt rObj 1 0)\nDeleteConditionFormOne rObj 0\n"
            "SetFunctionValue (sv_Construct \"%z %g\" sText ((GetConditionFormOneInt rObj 0 0) == fSave))", True),
       _one("add.one.recipe.bad", _form("rObj", ONE["recipe"]) + "let fN := GetConditionFormOneCount rObj\n"
            "let fRes := AddConditionFormOne rObj 40\nSetFunctionValue (sv_Construct \"%g %g\" fRes ((GetConditionFormOneCount rObj) - fN))", True),
       _one("add.two.message", _form("rObj", ONE["message"]) + "let fN := GetConditionFormTwoCount rObj 0\n"
            "let fRes := AddConditionFormTwo rObj 0\nlet sText := sv_Construct \"%g %g %g\" fRes (GetConditionFormTwoCount rObj 0) "
            "(GetConditionFormTwoInt rObj 0 fN 0)\nDeleteConditionFormTwo rObj 0 fN\n"
            "SetFunctionValue (sv_Construct \"%z %g\" sText (GetConditionFormTwoCount rObj 0))", True),
       _one("add.three.quest", _form("rObj", ONE["quest"]) + "let fN := GetConditionFormThreeCount rObj (-1) 0\n"
            "let fRes := AddConditionFormThree rObj (-1) 0\nlet sText := sv_Construct \"%g %g\" fRes (GetConditionFormThreeCount rObj (-1) 0)\n"
            "DeleteConditionFormThree rObj (-1) 0 fN\nSetFunctionValue (sv_Construct \"%z %g\" sText (GetConditionFormThreeCount rObj (-1) 0))", True),
       # Two new ones swapped (the record's own conditions are left in place).
       _one("swap.one.recipe", _form("rObj", ONE["recipe"]) + "let fN := GetConditionFormOneCount rObj\n"
            "AddConditionFormOne rObj\nAddConditionFormOne rObj\nSetConditionFormOneInt rObj fN 4 3\n"
            "let fRes := SwapConditionFormOne rObj fN (fN + 1)\n"
            "let sText := sv_Construct \"%g %g %g\" fRes (GetConditionFormOneInt rObj fN 4) (GetConditionFormOneInt rObj (fN + 1) 4)\n"
            "let fRes := SwapConditionFormOne rObj fN (fN + 9)\n"
            "DeleteConditionFormOne rObj (fN + 1)\nDeleteConditionFormOne rObj fN\n"
            "SetFunctionValue (sv_Construct \"%z %g %g\" sText fRes ((GetConditionFormOneCount rObj) - fN))", True),
       # A condition's comparison, value and flags on a new one, then the new one deleted.
       _one("set.one.recipe", _form("rObj", ONE["recipe"]) + "let fN := GetConditionFormOneCount rObj\nAddConditionFormOne rObj\n"
            "let fRes := SetConditionFormOneInt rObj fN 4 2\nlet sText := sv_Construct \"%g %g\" fRes (GetConditionFormOneInt rObj fN 4)\n"
            "let fRes := SetConditionFormOneFloat rObj fN 0 7.5\nlet sText := sv_Construct \"%z %g %g\" sText fRes (GetConditionFormOneFloat rObj fN 0)\n"
            "let fRes := SetConditionFormOneInt rObj fN 5 9\nlet sText := sv_Construct \"%z %g %g\" sText fRes (GetConditionFormOneInt rObj fN 5)\n"
            "let fRes := SetConditionFormOneInt rObj fN 1 0\nlet sText := sv_Construct \"%z %g %g\" sText fRes (GetConditionFormOneInt rObj fN 1)\n"
            "let fRes := SetConditionFormOneForm rObj fN 3 rP\nlet sText := sv_Construct \"%z %g\" sText fRes\n"
            "let fRes := EvalConditionListFormOne rObj rP rP\nDeleteConditionFormOne rObj fN\n"
            "SetFunctionValue (sv_Construct \"%z %g %g\" sText fRes ((GetConditionFormOneCount rObj) - fN))", True),
       # The weapon's additional vector: two pairs, read, scaled, deleted.
       _one("weap.vector", _form("rObj", ONE["pistol"]) + "let fRes := SetWeapAdditionalVectorValue rObj 0 0.9 1\n"
            "SetWeapAdditionalVectorValue rObj 0 0.5 0.2\nlet fN := SetWeapAdditionalVectorValue rObj 0 0.7 -1\n"
            "let aA := GetWeapAdditionalVectorArray rObj 0\n"
            "let sText := sv_Construct \"%g %g %g %g %g %g %g\" fRes fN (GetWeapAdditionalVectorValue rObj 0 0.9) "
            "(GetWeapAdditionalVectorValueScale rObj 0 0.7) (GetWeapAdditionalVectorValueScale rObj 0 0.2) (ar_Size aA) "
            "(GetWeapAdditionalVectorValueScale rObj 0 1)\n"
            "DeleteWeapAdditionalVectorValue rObj 0 0.9\nDeleteWeapAdditionalVectorValue rObj 0 0.5\n"
            "SetFunctionValue (sv_Construct \"%z %g\" sText (GetWeapAdditionalVectorValue rObj 0 0.9))", True),
       _one("ammo.settrait", _form("rObj", ONE["amefhp"]) + "let fSave := GetAmmoEffectTraitNumeric rObj 2\n"
            "let fRes := SetAmmoEffectTraitNumericAlt rObj 2 42\nlet sText := sv_Construct \"%g %g\" fRes (GetAmmoEffectTraitNumeric rObj 2)\n"
            "SetAmmoEffectTraitNumericAlt rObj 2 fSave\nSetFunctionValue (sv_Construct \"%z %g\" sText ((GetAmmoEffectTraitNumeric rObj 2) == fSave))", True),
       _one("mod.effect", _form("rObj", ONE["varmint"]) + "let fSave := GetWeaponItemModEffect 1 rObj\n"
            "let fRes := SetWeaponItemModEffectAlt rObj 1 20\nlet sText := sv_Construct \"%g %g %g\" fSave fRes (GetWeaponItemModEffect 1 rObj)\n"
            "SetWeaponItemModEffectAlt rObj 1 fSave\nSetFunctionValue (sv_Construct \"%z %g\" sText (GetWeaponItemModEffect 1 rObj))", True),
       _one("mod.value2", _form("rObj", ONE["varmint"]) + "let fSave := GetWeaponItemModValue2 1 rObj\n"
            "let fRes := SetWeaponItemModValue2 rObj 1 3.5\nlet sText := sv_Construct \"%g %g %g\" fSave fRes (GetWeaponItemModValue2 1 rObj)\n"
            "SetWeaponItemModValue2 rObj 1 fSave\nSetFunctionValue (sv_Construct \"%z %g\" sText (GetWeaponItemModValue2 1 rObj))", True),
       # A scope shader that is not an effect shader (the pistol itself): refused or not.
       # No vanilla weapon has a scope shader to put back and a set one may not clear
       # again, so a real shader is not set here.
       _one("scope.set.notshader", _form("rObj", ONE["sniper"]) + _form("rOther", ONE["pistol"])
            + "let fRes := SetWeaponScopeEffectShader rObj rOther\nlet rSave := GetWeaponScopeEffectShader rObj\n"
            "if rSave != rOther\nSetFunctionValue (sv_Construct \"%g %z\" fRes (GetFormIDString rSave))\nreturn\nendif\n"
            "let rOther := GetFormFromMod \"FalloutNV.esm\" \"0\"\nSetWeaponScopeEffectShader rObj rOther\n"
            "SetFunctionValue (sv_Construct \"%g %z %z\" fRes (GetFormIDString rSave) (GetFormIDString (GetWeaponScopeEffectShader rObj)))", True)]
)


# sup2 (writes, session only; #1008): the dialogue setters of SUP batch 2 with the
# JohnnyGuitar, ShowOff and ITR functions that edit the same records
# (docs/spec-notes/ext-sup2.md), on FalloutNV.esm forms [observed: fnvtool esm decode]:
# Sunny Smiles's first greeting (INFO 104E7E: Say Once, ten choices, one response), her
# goodbye (INFO 15CB5B: Goodbye, one response) and the topic of her "What kind of creatures
# are out there?" (DIAL 1055D2: priority 50, no flags), the examples the pages use; GREETING
# (C8) and GOODBYE (D4). Flags and priorities are put back; the choice and related-topic
# lists only grow (no function takes an entry out), so those rows run in order and later
# rows read the lists the earlier ones left.
SUP2 = {"greet": "104E7E", "bye": "15CB5B", "topic": "1055D2", "greeting": "C8", "goodbye": "D4", "quest": "104C66"}


def _sup2(pid, body, text=True):
    return (f"sup2.{pid}", "ref rObj\nref rOther\nref rTopic\nfloat fRes\nfloat fSave\nstring_var sText\nstring_var sSave\n"
            "array_var aList\nBegin Function { }\n" + body, text, None, "")


def _sup2_forms(*keys):
    return "".join(_form(v, SUP2[k]) for v, k in zip(("rObj", "rTopic", "rOther"), keys))


def _sup2_list(expr):
    """The list's size, first and last entries (GetFormIDString), as one text."""
    return (f"let aList := {expr}\nlet fRes := ar_Size aList\nlet sText := sv_Construct \"%g\" fRes\nif fRes > 0\n"
            "let rOther := aList[0]\nlet sText := sv_Construct \"%z %z\" sText (GetFormIDString rOther)\n"
            "let rOther := aList[fRes - 1]\nlet sText := sv_Construct \"%z %z\" sText (GetFormIDString rOther)\nendif\n"
            "SetFunctionValue sText")


def _sup2_set(pid, keys, getter, value, setter=None):
    """Save the getter's value, set `value`, read it back, put the saved value back."""
    setter = setter or getter
    return _sup2(pid, _sup2_forms(*keys) + f"let fSave := {getter}\nlet fRes := {setter} {value}\n"
                 f"let sText := sv_Construct \"%g %g %g\" fSave fRes ({getter})\n{setter} fSave\n"
                 f"SetFunctionValue (sv_Construct \"%z %g\" sText ({getter}))")


SUP2_PROBES = (
    # Topic flags and priority [documented: TopicSetFlags, TopicSetPriority].
    [_sup2("topic.flags", _sup2_forms("greet", "topic") + "SetFunctionValue (TopicSetFlags rTopic)", False),
     _sup2("topic.flags.greeting", _form("rTopic", SUP2["greeting"]) + "SetFunctionValue (TopicSetFlags rTopic)", False),
     _sup2_set("topic.flags.set", ("greet", "topic"), "TopicSetFlags rTopic", 3),
     _sup2_set("topic.flags.wide", ("greet", "topic"), "TopicSetFlags rTopic", 511),
     _sup2("topic.priority", _sup2_forms("greet", "topic") + "SetFunctionValue (TopicSetPriority rTopic)", False),
     _sup2_set("topic.priority.set", ("greet", "topic"), "TopicSetPriority rTopic", 75.5)]
    # A line's flag bytes [documented: TopicInfoSetFlagsHigh, …Low; GetDialogueInfoFlags].
    + [_sup2(f"info.{w}.{k}", _sup2_forms(k) + f"SetFunctionValue (TopicInfoSetFlags{w.title()} rObj)", False)
       for k in ("greet", "bye") for w in ("high", "low")]
    + [_sup2("info.itr.greet", _sup2_forms("greet") + "SetFunctionValue (GetDialogueInfoFlags rObj)", False),
       _sup2("info.high.set", _sup2_forms("greet") + "let fSave := TopicInfoSetFlagsHigh rObj\nlet fRes := TopicInfoSetFlagsHigh rObj 5\n"
             "let sText := sv_Construct \"%g %g %g %g\" fSave fRes (TopicInfoSetFlagsHigh rObj) (GetDialogueInfoFlags rObj)\n"
             "TopicInfoSetFlagsHigh rObj fSave\nSetFunctionValue (sv_Construct \"%z %g\" sText (GetDialogueInfoFlags rObj))"),
       _sup2("info.low.set", _sup2_forms("greet") + "let fSave := TopicInfoSetFlagsLow rObj\nlet fRes := TopicInfoSetFlagsLow rObj 1\n"
             "let sText := sv_Construct \"%g %g %g %g\" fSave fRes (TopicInfoSetFlagsHigh rObj) (GetDialogueInfoFlags rObj)\n"
             "TopicInfoSetFlagsLow rObj fSave\nSetFunctionValue (sv_Construct \"%z %g\" sText (GetDialogueInfoFlags rObj))"),
       _sup2("info.high.wide", _sup2_forms("greet") + "let fSave := TopicInfoSetFlagsHigh rObj\nlet fRes := TopicInfoSetFlagsHigh rObj 511\n"
             "let sText := sv_Construct \"%g %g %g\" fRes (TopicInfoSetFlagsLow rObj) (GetDialogueInfoFlags rObj)\n"
             "TopicInfoSetFlagsHigh rObj fSave\nSetFunctionValue (sv_Construct \"%z %g\" sText (GetDialogueInfoFlags rObj))"),
       _sup2("info.itr.set", _sup2_forms("bye") + "let fSave := GetDialogueInfoFlags rObj\nlet fRes := SetDialogueInfoFlags rObj 260\n"
             "let sText := sv_Construct \"%g %g %g\" fRes (TopicInfoSetFlagsHigh rObj) (TopicInfoSetFlagsLow rObj)\n"
             "SetDialogueInfoFlags rObj fSave\nSetFunctionValue (sv_Construct \"%z %g\" sText (GetDialogueInfoFlags rObj))")]
    # The prompt [documented: TopicInfoSetPrompt]: set, then an empty one.
    + [_sup2("prompt.set", _sup2_forms("bye") + "let sText := TopicInfoSetPrompt rObj \"My custom prompt\"\n"
             "let sSave := TopicInfoSetPrompt rObj \"\"\nSetFunctionValue (sv_Construct \"%z|%z\" sText sSave)"),
       _sup2("prompt.get", _sup2_forms("bye") + "let sText := TopicInfoSetPrompt rObj\nSetFunctionValue sText")]
    # Choices, in order: the end, a topic already there, the middle, past the end, then
    # replacing [documented: TopicInfoAddChoice, TopicInfoReplaceChoice].
    + [_sup2("choices.count", _sup2_forms("greet") + "SetFunctionValue (TopicInfoGetChoiceCount rObj)", False),
       _sup2("choices.add.end", _sup2_forms("greet", "topic") + "TopicInfoAddChoice rObj rTopic -1\n"
             + _sup2_list("TopicInfoGetAllChoices rObj")),
       _sup2("choices.add.again", _sup2_forms("greet", "topic") + "TopicInfoAddChoice rObj rTopic 0\n"
             + _sup2_list("TopicInfoGetAllChoices rObj")),
       _sup2("choices.add.middle", _sup2_forms("greet", "greeting") + "TopicInfoAddChoice rObj rTopic 3\n"
             "let aList := TopicInfoGetAllChoices rObj\nlet rOther := aList[3]\n"
             "SetFunctionValue (sv_Construct \"%g %z\" (ar_Size aList) (GetFormIDString rOther))"),
       _sup2("choices.add.past", _sup2_forms("greet", "goodbye") + "TopicInfoAddChoice rObj rTopic 99\n"
             + _sup2_list("TopicInfoGetAllChoices rObj")),
       _sup2("choices.replace", _sup2_forms("greet", "greeting") + "TopicInfoReplaceChoice rObj rTopic 1\n"
             "let aList := TopicInfoGetAllChoices rObj\nlet rOther := aList[1]\n"
             "SetFunctionValue (sv_Construct \"%g %z\" (ar_Size aList) (GetFormIDString rOther))"),
       _sup2("choices.replace.past", _sup2_forms("greet", "greeting") + "TopicInfoReplaceChoice rObj rTopic 99\n"
             + _sup2_list("TopicInfoGetAllChoices rObj")),
       _sup2("choices.has", _sup2_forms("greet", "goodbye") + "SetFunctionValue (TopicInfoHasChoice rObj rTopic)", False)]
    # JohnnyGuitar's related topics: 0 Link From, 1 Choices, 2 Follow Ups
    # [documented: DialogResponseAddRelatedTopic, DialogResponseRelatedGetAll].
    + [_sup2(f"related.get.{t}", _sup2_forms("greet") + _sup2_list(f"DialogResponseRelatedGetAll rObj {t}")) for t in (0, 1, 2)]
    + [_sup2("related.get.notype", _sup2_forms("greet") + _sup2_list("DialogResponseRelatedGetAll rObj")),
       _sup2("related.add.linkfrom", _sup2_forms("bye", "topic") + "DialogResponseAddRelatedTopic rObj rTopic 0 0\n"
             + _sup2_list("DialogResponseRelatedGetAll rObj 0")),
       _sup2("related.add.choices", _sup2_forms("bye", "topic") + "DialogResponseAddRelatedTopic rObj rTopic 1 0\n"
             + _sup2_list("DialogResponseRelatedGetAll rObj 1")),
       _sup2("related.add.followtopic", _sup2_forms("bye", "topic") + "DialogResponseAddRelatedTopic rObj rTopic 2 -1\n"
             + _sup2_list("DialogResponseRelatedGetAll rObj 2")),
       _sup2("related.add.followinfo", _form("rObj", SUP2["bye"]) + _form("rTopic", SUP2["greet"])
             + "DialogResponseAddRelatedTopic rObj rTopic 2 -1\n" + _sup2_list("DialogResponseRelatedGetAll rObj 2"))]
    # Said once [documented: GetSaidOnce, SetSaidOnce, TopicInfoSetSaidOnce]; put back
    # unsaved.
    + [_sup2("said.get", _sup2_forms("greet") + _form("rOther", SUP2["bye"])
             + "SetFunctionValue (sv_Construct \"%g %g\" (GetSaidOnce rObj) (GetSaidOnce rOther))"),
       _sup2("said.sup", _sup2_forms("bye") + "let fRes := TopicInfoSetSaidOnce rObj 1\n"
             "let sText := sv_Construct \"%g %g %g\" fRes (GetSaidOnce rObj) (TopicInfoSetSaidOnce rObj)\n"
             "let fRes := TopicInfoSetSaidOnce rObj 0\nSetFunctionValue (sv_Construct \"%z %g %g\" sText fRes (GetSaidOnce rObj))"),
       _sup2("said.jg", _sup2_forms("bye") + "let fRes := SetSaidOnce rObj 1 0\nlet sText := sv_Construct \"%g %g\" fRes (GetSaidOnce rObj)\n"
             "let fRes := SetSaidOnce rObj 0 0\nSetFunctionValue (sv_Construct \"%z %g %g\" sText fRes (GetSaidOnce rObj))")]
    # Topics and their lines [documented: GetParentTopic, GetTopicInfo].
    + [_sup2("parent", _sup2_forms("greet") + "SetFunctionValue (GetFormIDString (GetParentTopic rObj))"),
       _sup2("topicinfo", _sup2_forms("greet", "topic") + _sup2_list("GetTopicInfo rTopic")),
       _sup2("topicinfo.quest", _form("rTopic", SUP2["greeting"]) + _form("rOther", SUP2["quest"])
             + "SetFunctionValue (ar_Size (GetTopicInfo rTopic rOther))", False)]
    # Response texts [documented: TopicInfoSetNthResponseString, TopicInfoSetResponseStrings],
    # the original text put back from the getter's array.
    + [_sup2("responses", _sup2_forms("bye") + "let aList := TopicInfoGetResponseStrings rObj\nlet sSave := aList[0]\n"
             "let fRes := TopicInfoSetNthResponseString rObj 0 \"Changed\"\nlet aList := TopicInfoGetResponseStrings rObj\n"
             "let sText := aList[0]\nlet sText := sv_Construct \"%g %z %g\" fRes sText (TopicInfoSetNthResponseString rObj 1 \"None\")\n"
             "let aList := ar_List \"First\", \"Second\"\nlet fRes := TopicInfoSetResponseStrings rObj aList\n"
             "let aList := TopicInfoGetResponseStrings rObj\nlet sText := sv_Construct \"%z|%g %g\" sText fRes (ar_Size aList)\n"
             "let sText := sv_Construct \"%z %z\" sText (aList[0])\nTopicInfoSetNthResponseString rObj 0 sSave\n"
             "let aList := TopicInfoGetResponseStrings rObj\nlet sSave := aList[0]\nSetFunctionValue (sv_Construct \"%z|%z\" sText sSave)")]
)


# sup3 (writes, session only; #1008): SUP batch 3 (docs/spec-notes/ext-sup3.md): the free
# (TFC) camera's position, angles and raw rotation, the calculated angles and positions,
# and the INI readers. One step per game frame: the camera is turned on by Con_TFC in one
# row and read in the next; positions are printed relative to the player's, so the rows do
# not hang on where the save stands. The camera is turned off again in the last camera
# row. The INI rows read the plugins' own INI files in Data\NVSE\Plugins (read only).
def _sup3(pid, body, text=True):
    return (f"sup3.{pid}", "ref rP\nfloat fA\nfloat fB\nfloat fC\nfloat fD\nfloat fX\nfloat fY\nfloat fZ\nstring_var sText\n"
            "array_var aList\nBegin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


def _sup3_rel():
    """The camera's position less the player's, as text."""
    return ("SetFunctionValue (sv_Construct \"%.1f %.1f %.1f\" ((GetTFCPos X) - (rP.GetPos X)) "
            "((GetTFCPos Y) - (rP.GetPos Y)) ((GetTFCPos Z) - (rP.GetPos Z)))")


_SUP3_ANGLES = ("SetFunctionValue (sv_Construct \"%.4f %.4f %.4f %.4f\" (GetTFCAngle X) (GetTFCAngle Z) "
                "(GetTFCRot X) (GetTFCRot Y))")

_SUP3_INI = {"sup": "Data/NVSE/Plugins/supNVSE.ini", "jg": "Data/NVSE/Plugins/JohnnyGuitar.ini",
             "jip": "Data/NVSE/Plugins/jip_nvse.ini"}


def _sup3_list(expr):
    return (f"let aList := {expr}\nlet fA := ar_Size aList\nif fA > 0\n"
            "SetFunctionValue (sv_Construct \"%g|%z|%z\" fA (aList[0]) (aList[fA - 1]))\nelse\n"
            "SetFunctionValue (sv_Construct \"%g\" fA)\nendif")


SUP3_PROBES = (
    # The camera before it flies [documented: GetTFCPos, GetTFCAngle, GetTFCRot].
    [_sup3("cam.off.pos", _sup3_rel()),
     _sup3("cam.off.angles", _SUP3_ANGLES),
     _sup3("cam.off.player", "SetFunctionValue (sv_Construct \"%.4f %.4f\" (rP.GetAngle X) (rP.GetAngle Z))"),
     _sup3("cam.on", "Con_TFC\nSetFunctionValue 1", False),
     _sup3("cam.on.pos", _sup3_rel()),
     _sup3("cam.on.angles", _SUP3_ANGLES),
     # Positions [documented: SetTFCPos, SetTFCPosEx, GetTFCPosEx].
     _sup3("cam.setpos", "let fA := SetTFCPos X ((rP.GetPos X) + 100)\nSetTFCPos Y ((rP.GetPos Y) - 50)\n"
           "SetTFCPos Z ((rP.GetPos Z) + 25)\n"
           "SetFunctionValue (sv_Construct \"%g %.1f %.1f %.1f\" fA ((GetTFCPos X) - (rP.GetPos X)) "
           "((GetTFCPos Y) - (rP.GetPos Y)) ((GetTFCPos Z) - (rP.GetPos Z)))"),
     _sup3("cam.setpos.next", _sup3_rel()),
     _sup3("cam.setposex", "let fX := (rP.GetPos X) + 10\nlet fY := (rP.GetPos Y) + 20\nlet fZ := (rP.GetPos Z) + 30\n"
           "let fA := SetTFCPosEx fX fY fZ\nlet fB := GetTFCPosEx fX fY fZ\n"
           "SetFunctionValue (sv_Construct \"%g %g %.1f %.1f %.1f\" fA fB (fX - (rP.GetPos X)) (fY - (rP.GetPos Y)) (fZ - (rP.GetPos Z)))"),
     # Angles and the raw rotation [documented: SetTFCAngle, SetTFCRot, …Ex; GetTFCAngle's
     # note: "not the RAW value but a rotation value converted to angle"].
     _sup3("cam.setangle.x", "let fA := SetTFCAngle X 30\nSetFunctionValue (sv_Construct \"%g %.4f %.4f %.4f %.4f\" fA (GetTFCAngle X) (GetTFCAngle Z) (GetTFCRot X) (GetTFCRot Y))"),
     _sup3("cam.setangle.z", "SetTFCAngle Z 45\n" + _SUP3_ANGLES),
     _sup3("cam.setangle.next", _SUP3_ANGLES),
     _sup3("cam.setangle.wide", "SetTFCAngle Z 400\nSetTFCAngle X -100\n" + _SUP3_ANGLES),
     _sup3("cam.setrot.x", "let fA := SetTFCRot X 0.5\nSetFunctionValue (sv_Construct \"%g %.4f %.4f %.4f %.4f\" fA (GetTFCAngle X) (GetTFCAngle Z) (GetTFCRot X) (GetTFCRot Y))"),
     _sup3("cam.setrot.y", "SetTFCRot Y 1\n" + _SUP3_ANGLES),
     _sup3("cam.angleex", "SetTFCAngleEx 10 20\nlet fA := GetTFCAngleEx fA fB\nGetTFCRotEx fC fD\n"
           "SetFunctionValue (sv_Construct \"%.4f %.4f %.4f %.4f\" fA fB fC fD)"),
     _sup3("cam.rotex", "SetTFCRotEx 0.25 0.75\nGetTFCRotEx fC fD\nGetTFCAngleEx fA fB\n"
           "SetFunctionValue (sv_Construct \"%.4f %.4f %.4f %.4f\" fA fB fC fD)"),
     # The angle to look at a point from the camera [documented: GetCalculatedAngleX/Z
     # ForTFCCamera], the camera put at the player's feet first.
     _sup3("cam.calc.x", "let fX := rP.GetPos X\nlet fY := rP.GetPos Y\nlet fZ := rP.GetPos Z\nSetTFCPosEx fX fY fZ\n"
           "SetFunctionValue (sv_Construct \"%.4f %.4f %.4f %.4f\" (GetCalculatedAngleXForTFCCamera (fX + 100) fY (fZ + 100)) "
           "(GetCalculatedAngleXForTFCCamera fX (fY + 100) (fZ - 100)) (GetCalculatedAngleXForTFCCamera fX (fY + 100) fZ) "
           "(GetCalculatedAngleXForTFCCamera fX (fY + 30) (fZ + 40)))"),
     _sup3("cam.calc.z", "let fX := rP.GetPos X\nlet fY := rP.GetPos Y\nSetTFCPosEx fX fY (rP.GetPos Z)\n"
           "SetFunctionValue (sv_Construct \"%.4f %.4f %.4f %.4f %.4f\" (GetCalculatedAngleZForTFCCamera (fX + 100) fY) "
           "(GetCalculatedAngleZForTFCCamera fX (fY + 100)) (GetCalculatedAngleZForTFCCamera (fX - 100) fY) "
           "(GetCalculatedAngleZForTFCCamera fX (fY - 100)) (GetCalculatedAngleZForTFCCamera (fX + 100) (fY + 100)))"),
     _sup3("cam.off2", "Con_TFC\nSetFunctionValue 1", False),
     _sup3("cam.off2.pos", _sup3_rel()),
     _sup3("cam.off2.angles", _SUP3_ANGLES)]
    # Positions around a reference or a point [documented: GetCalculatedPos,
    # GetCalculatedPosAlt].
    + [_sup3(f"calc.pos.{a}", f"rP.GetCalculatedPos {a} 25 fX fY\n"
             "SetFunctionValue (sv_Construct \"%.3f %.3f\" (fX - (rP.GetPos X)) (fY - (rP.GetPos Y)))") for a in (0, 90, 45, -90, 180)]
    + [_sup3(f"calc.alt.{a}", f"let fA := GetCalculatedPosAlt {a} 25 fX fY 100 200\n"
             "SetFunctionValue (sv_Construct \"%g %.3f %.3f\" fA fX fY)") for a in (0, 90, 30)]
    # The INI readers [documented: ReadINIFloatFromFile, …Alt, ReadINIStringFromFile,
    # ReadINISectionKeysFromFile, ReadINISectionsFromFile].
    + [_sup3("ini.float.bom", f"SetFunctionValue (ReadINIFloatFromFile \"{_SUP3_INI['sup']}\" \"Settings\" \"bTFCPosOnLoadFix\")", False),
       _sup3("ini.float.case", f"SetFunctionValue (ReadINIFloatFromFile \"{_SUP3_INI['jg']}\" \"Main\" \"bFixItemStackCount\")", False),
       _sup3("ini.float.space", f"SetFunctionValue (ReadINIFloatFromFile \"{_SUP3_INI['jip']}\" \"GamePatches\" \"bIgnoreDTDRFix\")", False),
       _sup3("ini.float.missing", f"SetFunctionValue (ReadINIFloatFromFile \"{_SUP3_INI['jg']}\" \"MAIN\" \"bNoSuchKey\")", False),
       _sup3("ini.alt.found", f"SetFunctionValue (ReadINIFloatFromFileAlt \"{_SUP3_INI['jg']}\" \"MAIN\" \"bFixItemStackCount\")", False),
       _sup3("ini.alt.missing", f"let fA := ReadINIFloatFromFileAlt \"{_SUP3_INI['jg']}\" \"MAIN\" \"bNoSuchKey\"\n"
             "SetFunctionValue (sv_Construct \"%g\" (IsNumberNAN fA))"),
       _sup3("ini.alt.nofile", "let fA := ReadINIFloatFromFileAlt \"Data/NVSE/Plugins/NoSuchFile.ini\" \"MAIN\" \"bX\"\n"
             "SetFunctionValue (sv_Construct \"%g\" (IsNumberNAN fA))"),
       _sup3("ini.float.backslash", "SetFunctionValue (ReadINIFloatFromFile \"Data\\\\NVSE\\\\Plugins\\\\JohnnyGuitar.ini\" \"MAIN\" \"bFixItemStackCount\")", False),
       _sup3("ini.string", f"SetFunctionValue (sv_Construct \"[%z]\" (ReadINIStringFromFile \"{_SUP3_INI['jip']}\" \"GamePatches\" \"bIgnoreDTDRFix\"))"),
       _sup3("ini.string.missing", f"SetFunctionValue (sv_Construct \"[%z]\" (ReadINIStringFromFile \"{_SUP3_INI['jg']}\" \"MAIN\" \"bNoSuchKey\"))"),
       _sup3("ini.keys", _sup3_list(f"ReadINISectionKeysFromFile \"{_SUP3_INI['jg']}\" \"MAIN\"")),
       _sup3("ini.keys.case", _sup3_list(f"ReadINISectionKeysFromFile \"{_SUP3_INI['jg']}\" \"main\"")),
       _sup3("ini.sections.sup", _sup3_list(f"ReadINISectionsFromFile \"{_SUP3_INI['sup']}\"")),
       _sup3("ini.sections.jip", _sup3_list(f"ReadINISectionsFromFile \"{_SUP3_INI['jip']}\"")),
       _sup3("ini.sections.nofile", _sup3_list("ReadINISectionsFromFile \"Data/NVSE/Plugins/NoSuchFile.ini\"")),
       # Outside the game folder: the original may read it; NVForever reads only under the
       # game folder (mod_files::on_disk; ext-sup3.md).
       _sup3("ini.float.outside", "SetFunctionValue (ReadINIFloatFromFile \"../Fallout New Vegas/Data/NVSE/Plugins/supNVSE.ini\" \"Settings\" \"bTFCPosOnLoadFix\")", False)]
)


# lu2 (writes, session only, each change put back; #1009): Lutana batch 2
# (docs/spec-notes/ext-lutana.md §9) through JIP LN: radio data of the pages' own
# references (RNVTARef; AlienRadioBeaconREF is FO3's and not in FalloutNV.esm, so the
# radius rows read Camp Guardian's NVCGRadioREF 13D7A2 instead [observed: fnvtool esm
# decode, XRDO radius 5000, Radius, static 50, position reference 14ADB9]), skill
# magazines, the quest items switch, form counts by type, the plugins of Scotch, and
# SetIngestibleFlag on the page's Jalapeno, put back.
_LU2_HEAD = "ref rP\nref rObj\nref rOther\narray_var aR\nfloat fRes\nfloat fA\nBegin Function { }\n"


def _lu2(pid, body, text=False):
    return (f"lu2.{pid}", _LU2_HEAD + _form("rP", NF["player"]) + body, text, None, "")


def _lu2v(pid, expr, text=False):
    return _lu2(pid, f"SetFunctionValue ({expr})", text)


LU2_PROBES = (
    [_lu2v("radio.rnv.is", "RNVTARef.IsRadioRef"),
     _lu2v("radio.rnv.type", "RNVTARef.GetRadioBroadcastType"),
     # The AlienRadioBeaconREF rows (the pages' FO3 example) did not compile in the original
     # nor here: no such editor ID in the load order [observed 2026-11-08].
     _lu2("radio.cg.radius", _form("rObj", "13D7A2") + "SetFunctionValue (rObj.GetRadioRadius)"),
     _lu2("radio.cg.static", _form("rObj", "13D7A2") + "SetFunctionValue (rObj.GetRadioStatic)"),
     _lu2("radio.cg.type", _form("rObj", "13D7A2") + "SetFunctionValue (rObj.GetRadioBroadcastType)"),
     # GetRadioPosRef is registered with no return type (nvse.log), so `set` takes it first
     _lu2("radio.cg.posref", _form("rObj", "13D7A2") + "set rOther to rObj.GetRadioPosRef\nSetFunctionValue (GetFormIDString rOther)", True),
     _lu2v("radio.player.is", "rP.IsRadioRef"),
     _lu2("radio.cg.set", _form("rObj", "13D7A2") + "rObj.SetRadioRadius 2000\nlet fRes := rObj.GetRadioRadius\nrObj.SetRadioRadius 5000\nSetFunctionValue fRes"),
     _lu2v("marker.player", "rP.IsMapMarker"),
     _lu2v("magazine.explosives", "IsSkillMagazine MagazineNVExplosives"),
     _lu2("magazine.stimpak", _form("rObj", NF["stimpak"]) + "SetFunctionValue (IsSkillMagazine rObj)"),
     _lu2v("questitems", "GetShowQuestItems"),
     # GetFormCountType takes the array's ID (an integer since JIP v40, the page): an array
     # variable as the page's example; `(ar_List …)` in its place did not compile [observed 2026-11-08]
     _lu2("formcount.alch", _form("rObj", NF["stimpak"]) + "let aR := ar_List rObj, rObj, rP\nlet fRes := GetFormCountType aR, 47\nSetFunctionValue fRes"),
     _lu2("formmods.scotch", "let aR := GetFormMods Scotch\nSetFunctionValue (sv_Construct \"%g %z\" (ar_Size aR) (aR[0]))", True),
     _lu2("ingestible.food", "let fRes := GetIngestibleFlag JalapenoPepper 2\nSetIngestibleFlag JalapenoPepper 2 0\nlet fA := GetIngestibleFlag JalapenoPepper 2\n"
          "SetIngestibleFlag JalapenoPepper 2 1\nSetFunctionValue (sv_Construct \"%g %g %g\" fRes fA (GetIngestibleFlag JalapenoPepper 2))", True)]
)

# sup6 (writes, session only; #1008): SUP batch 6 (docs/spec-notes/ext-sup6.md): the
# mouse, the cursor's traits, the debug texts, the screen, fonts and plugin headers, the
# HUD's visibility flags, the lockpick menu and the throw held, all with no menu open
# and nobody at the controls. Writes restored: the cursor's alpha set back to what it
# read, the debug text destroyed, the HUD flags written back. One step per frame.
def _sup6(pid, body, text=True):
    return (f"sup6.{pid}", "float fA\nfloat fB\nfloat fX\nfloat fY\nstring_var sText\nBegin Function { }\n" + body, text, None, "")


SUP6_PROBES = (
    # The cursor at rest (hypothesis: 255, the arrow's size, 255 255 255).
    [_sup6("cursor.rest", "SetFunctionValue (sv_Construct \"%g %g %g %g %g %g\" (GetCursorTrait 0) (GetCursorTrait 1) (GetCursorTrait 2) "
           "(GetCursorTrait 3) (GetCursorTrait 4) (GetCursorTrait 5))"),
     _sup6("cursor.set", "let fA := GetCursorTrait 0\nSetCursorTrait 0 100\nlet fB := GetCursorTrait 0\nSetCursorTrait 0 fA\n"
           "SetFunctionValue (sv_Construct \"%g %g %g\" fA fB (GetCursorTrait 0))"),
     # Gradual: 200 to 50 over 2 seconds, read at once and a frame on (between the two
     # ends), then stopped and set back.
     _sup6("cursor.gradual", "SetCursorTraitGradual 0 200 50 2\nSetFunctionValue (GetCursorTrait 0)", False),
     _sup6("cursor.gradual.next", "let fA := GetCursorTrait 0\nSetCursorTraitGradual 0\nlet fB := GetCursorTrait 0\nSetCursorTrait 0 255\n"
           "SetFunctionValue (sv_Construct \"%g %g\" ((fA > 50) && (fA <= 200)) (fA == fB))"),
     # Debug texts (hypothesis: exists 1 after create, 0 after destroy).
     _sup6("debugtext", "DebugTextCreate \"NVFProbeText\" 100 100 \"probe\"\nlet fA := DebugTextExists \"NVFProbeText\"\n"
           "DebugTextSetPos \"NVFProbeText\" 200 200\nDebugTextSetString \"NVFProbeText\" \"probe 2\"\nDebugTextSetZoom \"NVFProbeText\" 20\n"
           "DebugTextDestroy \"NVFProbeText\"\nSetFunctionValue (sv_Construct \"%g %g %g\" fA (DebugTextExists \"NVFProbeText\") (DebugTextExists \"NVFNoSuchText\"))"),
     # The mouse out of menus (hypothesis: 0 0 and 0).
     _sup6("mouse.moves", "GetMouseMovement fX fY 0\nlet fA := fX\nlet fB := fY\nGetMouseMovement fX fY 1\n"
           "SetFunctionValue (sv_Construct \"%g %g %g %g %g %g\" fA fB fX fY (GetMouseWheelMovement 0) (GetMouseWheelMovement 1))"),
     _sup6("mouse.pos", "GetMousePosition fX fY\nSetFunctionValue (sv_Construct \"%g\" ((fX >= 0) && (fY >= 0)))"),
     # The screen [documented: GetScreenTrait 0-4].
     _sup6("screen", "SetFunctionValue (sv_Construct \"%g %g %g %g %g\" (GetScreenTrait 0) (GetScreenTrait 1) (GetScreenTrait 2) "
           "(GetScreenTrait 3) (GetScreenTrait 4))"),
     # The fonts' heights (hypothesis: base = the file's line height, line = the tallest glyph).
     _sup6("fonts", "let sText := \"\"\nlet fA := 1\nwhile fA <= 9\nlet sText := sText + (sv_Construct \"%g/%g \" (GetFontTrait fA 0) (GetFontTrait fA 1))\n"
           "let fA += 1\nloop\nSetFunctionValue sText"),
     # FalloutNV.esm's author and description [documented].
     _sup6("modtraits", "SetFunctionValue (sv_Construct \"[%z] [%z] [%z]\" (GetModTraitSTR 0 0) (GetModTraitSTR 0 1) (GetModTraitSTR 0 2))"),
     # The machine (NVForever's decision: 0); recorded only.
     _sup6("pctraits", "SetFunctionValue (sv_Construct \"%g %g %g\" ((GetPCTrait 0) > 0) ((GetPCTrait 2) > 0) ((GetPCTrait 6) > 0))"),
     # The HUD flags as JohnnyGuitar's override [documented]: read, set, put back.
     _sup6("hudflags", "let fA := GetHUDVisibilityFlags\nSetHUDVisibilityFlags 512\nlet fB := GetHUDVisibilityOverride\n"
           "SetHUDVisibilityFlags fA\nSetFunctionValue (sv_Construct \"%g %g %g\" fA fB (GetHUDVisibilityFlags))"),
     # The lockpick menu shut [documented: -1]; SetBobbyPinHealth shut (hypothesis: -1).
     _sup6("lockpick.shut", "SetFunctionValue (sv_Construct \"%g %g %g %g\" (GetLockPickMenuValue 0) (GetLockPickMenuValue 1) "
           "(GetLockPickMenuValue 2) (SetBobbyPinHealth))"),
     # The throw held at rest (hypothesis: 0 before any throw).
     _sup6("grenade", "SetFunctionValue (GetGrenadeTimeHeld)", False)]
)


# showoff5 (writes, session only, each change put back; #1010): ShowOff batch 5
# (docs/spec-notes/ext-showoff.md §8): the level-up menu opened by ShowSkillMenu and closed
# again in the same step, the shot switches' reads, fast travel, AP costs, item values,
# VATS reads and the player's small states, on the player, Doc Mitchell and the 10mm pistol.
_SO5_HEAD = "ref rP\nref rDoc\nref rObj\nref rInv\nfloat fRes\nfloat fA\nfloat fB\nBegin Function { }\n"


def _so5(pid, body, text=False):
    return (f"showoff5.{pid}", _SO5_HEAD + _form("rP", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _so5v(pid, expr, text=False):
    return _so5(pid, f"SetFunctionValue ({expr})", text)


SHOWOFF5_PROBES = (
    [_so5v("levelup.page.closed", "GetLevelUpMenuCurrentPage"),
     _so5v("levelup.points.closed", "GetLevelUpMenuPoints"),
     _so5v("levelup.setpage.closed", "SetLevelUpMenuCurrentPage 1"),
     # Opened and closed in one step: the page, the points to allot and allotted, the
     # perks, and what closing returns [hypothesis: the menu reads at once].
     _so5("levelup.skillmenu", "let fRes := ShowSkillMenu 3\nlet fA := GetLevelUpMenuCurrentPage\nlet fB := GetLevelUpMenuPoints\n"
          "SetFunctionValue (sv_Construct \"%g %g %g %g %g %g\" fRes fA fB (GetLevelUpMenuPoints 0 1) (GetLevelUpMenuPoints 1) (SetLevelUpMenuCurrentPage 2))", True),
     _so5v("levelup.after", "GetLevelUpMenuCurrentPage"),
     _so5v("hitscan.read", "SetForceDrawHitscanProjectiles"),
     _so5v("tracer.read", "SetProjectileTracerChanceOverride"),
     _so5v("travel.override", "GetPCHasScriptedFastTravelOverride"),
     _so5v("travel.can", "GetPCCanFastTravel"),
     _so5("ap.pistol", _form("rObj", NF["pistol10"]) + "SetFunctionValue (sv_Construct \"%g %g %g\" (GetCalculatedAPCost 0 rObj) (GetCalculatedAPCost 1 rObj) (GetCalculatedAPCost 2 rObj))", True),
     # Fists (no weapon override): the unarmed costs, the uppercut and the cross.
     _so5("ap.fists", _form("rObj", "1F4") + "SetFunctionValue (sv_Construct \"%g %g %g\" (GetCalculatedAPCost 0 rObj) (GetCalculatedAPCost 1 rObj) (GetCalculatedAPCost 2 rObj))", True),
     _so5v("vats.engage", "GetVATSMaxEngageDistance"),
     _so5v("vats.target.doc", "rDoc.GetVATSTargetable"),
     _so5v("vats.target.player", "rP.GetVATSTargetable"),
     # The keyboard layout: "q" or "Q" [hypothesis: NVForever returns "" without the layout].
     _so5v("ansi.16", "ToANSIChar 16 1", True),
     _so5v("ansi.2", "ToANSIChar 2 1", True),
     _so5v("invisible.doc", "rDoc.IsActorInvisibleToPlayer"),
     _so5v("combatactors.player", "rP.GetNumCombatActorsFromActor"),
     # rP.GetHitLocationLingering on the unhurt player crashed the original [observed
     # 2026-11-08]: left out.
     _so5v("nightvision", "PlayerHasNightVisionActive"),
     _so5v("drinking", "PlayerIsDrinkingPlacedWater"),
     _so5v("prompt", "GetIsActivationPromptShown"),
     _so5v("looking", "IsPlayerLookingAround"),
     _so5("usage.stimpak", _form("rObj", NF["stimpak"]) + "SetFunctionValue (rP.GetSpellUsageNumEx rObj)"),
     # A placed 10mm pistol's value at its condition; with the barter flag outside barter.
     _so5("itemvalue.pistol", _form("rObj", NF["pistol10"]) + "let rInv := rP.PlaceAtMe rObj 1\nlet fA := rInv.GetCalculatedItemValue\nlet fB := rInv.GetCalculatedItemValue 1\nrInv.Disable\nrInv.MarkForDelete\n"
          "SetFunctionValue (sv_Construct \"%g %g\" fA fB)", True),
     _so5v("explosion.none", "rP.GetExplosionRefRadius"),
     _so5v("stuck.none", "rP.GetProjectileRefIsStuck"),
     _so5("stagger.player", "SetFunctionValue (rP.ForceHitStaggerReaction)"),
     _so5("recoil.player", "SetFunctionValue (rP.ForceRecoilAnim)")]
)

# lutana1 (writes, session only, each change put back; #1009, PR #1776): Lutana batch 1
# (docs/spec-notes/ext-lutana.md, crates/fnv-game/src/nvse_lutana.rs), the 92 functions on
# main, run through the installed JIP LN. FalloutNV.esm forms [observed: fnvtool esm
# inventory and decode; world refs]:
# - CHAL: NVChallengeKillOne (Kill from a Form List, threshold 250, flags 2, interval 50,
#   value3 4), NVChallengeStarts (Hit an Enemy, flags 7, values 1/3/1), NVChallengeVault34
#   (SNAM Vault34MapMarker), NVChallengeAimingOptional (XNAM 0x15EAD2), NVChallengeFreeRadical
#   (SNAM 0x15167), NVChallengeCaravanMaster (Misc Stat, Start Disabled).
# - CSNO: AtomicWranglerCasinoData (soft 17, payout 1.2, 1 deck, shuffle 0.35, max 5000),
#   GomorrahCasinoData (no soft 17, 1.5, 3 decks, 0.2, 9000); every reel stop 2.
# - RCPE (DATA, RCIL/RCOD/RCQY raw-read): RecipeBighornerSteak (Survival 44 at 50, in
#   BigHornerMeat x1, out BighornerSteak x1), RecipeHealingPowder (skill -1, level 0),
#   RecipeMassPurifiedWater (out 0x151A3 x4); SpareParts in RecipeWeaponRepairKit and
#   RecipeBreakdownScrapMetalLead (the page's example); DrainedSmallEnergyCell in two
#   recipes that both make AmmoSmallEnergyCell; BrocFlower in three (two Campfire).
# - NPC_: DocMitchell (services 1151, no ZNAM, no aggro radius), vTopsBartender02 (own
#   services 16, "Use AI Data" from vTopsBartender01, services 1148), VHDOliverRangerSniper
#   (aggro radius 200), CraigBoone (ZNAM 0xC80B6). Refs: DocMitchellREF, VMS21Cachino
#   (0x11118D, in Gomorrah01, XEZN GomorrahZone).
# - PERK: FourEyes (trait), ActionBoy (abilities 0x1718B7, 0x31D90), BuiltToDestroy
#   (ability, then an entry point), FortuneFinder (quest+stage, quest Generic 0x38B2),
#   CalmHeartPerk (not playable).
# - WRLD FreesideWorld and WastelandNVmini (WNAM WastelandNV), WastelandNV (no WNAM, NAM2
#   Potomac 0x30009, DNAM water -2300). CELL: the start cell 0xDEB16 (9,-29; XCLW 3.4e38,
#   no XCWT), GSDocMitchellHouse (interior, XCLW -2147483648, no XCWT).
# - WATR NVCleanWater (XNAM 0xB878F). ECZN Vault11Zone (Never Resets), GomorrahZone (none).
#   LVLI CapsChangeFortuneFinder (LVLG 0x645CC), CondNCRGamblerOutfit (no LVLG).
# - ACTI GoodspringsRadio (RNAM vCountryRadio, VNAM 0x36C2A), AverageWaterValve (SNAM
#   EMTWaterPressure, VNAM 0x1F92B), TrapGasValve (no RNAM/SNAM). TACT RadioNewVegasTA.
# - TERM VFSFistoTerminal (Average, PNAM 0x1344A3; ref 0x11F8CB), VMS49CommTerminal
#   (Requires Key), HVSelfDestructTerminal (Very Hard), VNovacTerminalManny (no PNAM).
#   Doors: 0x10DAFA (Gomorrah, lock 50, key VMS21GomorrahKey 0x11BC7F), 0xFB28A (JailDoor01,
#   ref flag 0x100 Inaccessible), 0x10D6AC (XAPD False). DocMitchellDresserRef (flags
#   0x2000400).
# - Playable: Caravan Shotgun Cass (not), 10mm pistol, Caucasian (DATA flags 1), Ghoul (0),
#   HairCurly (1), HairWavyOld (0), EyeBlue (1), eyeReanimate (0).
# - WEAP WeapLaserRifle (crit flags 1, kill impulse 0, impulse distance 0), WeapSuperSledge
#   (VATS MaulerKnockdownSpell). LSCR EnclaveLoadScreen01 (WMI1 StandardLoadingTip).
# - LIGH OfficeLightKeyNeutral (radius 200), Fire420; ref HVRangerFireLight 0xE5196 (XRDS
#   -16.59). NOTE VStripEpicureNote. FLST NPCFollowersLIST.
# Read-and-put-back order: every setter row reads, sets, reads, restores and reads again.
# Not restorable for certain: none (AddNoteNS is followed by RemoveNote when the note was
# not held; the key added for PlayerHasKey is removed again).
LU1 = {
    # challenges
    "killone": "15EB3A", "starts": "15EB1F", "vault34": "15EB3D", "aiming": "15EB10", "freerad": "15EAF2",
    "caravan": "16BA4E", "stimpak": "15169",
    # casinos
    "wrangler": "144C42", "gomorrah": "140521",
    # recipes and items
    "steak": "175AA9", "meat": "16D5BA", "steakout": "175AA8", "powder": "13B2C2", "masswater": "13D3C5",
    "water151": "151A3", "spare": "31944", "lead": "140C2F", "drained": "12115E", "broc": "13B2BA",
    "reloadbench": "153621", "breakdown": "140AB5", "campfire": "13B2C0", "aid": "13E119", "caps": "F",
    # actors
    "doc": "104C0C", "docref": "104C0F", "tender2": "129552", "tender1": "129551", "sniper": "14F3C3",
    "boone": "92BD2", "cachino": "11118D", "alcohol": "66988",
    # perks
    "foureyes": "135EC4", "actionboy": "31DBA", "builtto": "137B02", "fortune": "31DE3", "calmheart": "167723",
    # worlds, cells, water, zones, lists
    "freeside": "10BEEA", "wasteland": "DA726", "mini": "148C05", "startcell": "DEB16", "doccell": "103DF9",
    "cleanwater": "1009CA", "vault11zone": "E9081", "gomzone": "10CC90", "capslist": "645D5", "outfitlist": "17B7E3",
    # activators, terminals, locks
    "gsradio": "109085", "valve": "1699B6", "gasvalve": "47A97", "rnv": "14DF02", "radiosound": "36C2A",
    "fisto": "11F8BF", "fistoref": "11F8CB", "commterm": "14B048", "selfdestruct": "1645E2", "manny": "CCFC7",
    "epicure": "162B5B", "zoaradoor": "10DAFA", "zoarakey": "11BC7F", "jaildoor": "FB28A", "elevator": "10D6AC",
    "dresser": "103E37", "docdoor": "103E61",
    # flags and playability
    "cassgun": "176E55", "pistol": "434F", "caucasian": "19", "ghoul": "3B3E", "haircurly": "5DC7F",
    "hairwavyold": "44527", "eyeblue": "4253", "eyereanimate": "1A",
    # weapons, load screens, lights, lists
    "laser": "4336", "sledge": "4352", "withdrawal": "33067", "loadscreen": "166D93", "loadtype": "166D58",
    "officelight": "4DFDB", "fire420": "4477", "fireref": "E5196", "followers": "BC4BB",
}

_LU1H = ("ref rP\nref rObj\nref rOther\nref rItem\nref rKey\nfloat fRes\nfloat fSave\nfloat fA\nfloat fB\nfloat fC\n"
         "string_var sText\narray_var aList\narray_var aKeys\nBegin Function { }\n")


def _lu1(pid, body, text=False):
    return (f"lu1.{pid}", _LU1H + _form("rP", NF["player"]) + body, text, None, "")


def _lu1v(pid, key, expr, text=False):
    """One getter on rObj; text "form" prints the form ID."""
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return _lu1(pid, _form("rObj", LU1[key]) + rd, bool(text))


def _lu1o(pid, key, other, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return _lu1(pid, _form("rObj", LU1[key]) + _form("rOther", LU1[other]) + rd, bool(text))


def _lu1set(pid, key, read, setter, restore, other=None):
    """A number setter (a bare line): old value, the value read back, then after the
    restore the value read again."""
    pre = _form("rObj", LU1[key]) + (_form("rOther", LU1[other]) if other else "")
    return _lu1(pid, pre + f"let fSave := {read}\n{setter}\nlet fA := {read}\n{restore}\n"
                'SetFunctionValue (sv_Construct "%g %g %g" fSave fA (' + read + "))", True)


def _lu1fset(pid, key, other, read, setter, restore):
    """A form setter: the old form (kept in rItem), the form read back, the form after the
    restore."""
    pre = _form("rObj", LU1[key]) + _form("rOther", LU1[other])
    return _lu1(pid, pre + f"let rItem := {read}\n{setter}\nlet sText := GetFormIDString ({read})\n{restore}\n"
                'SetFunctionValue (sv_Construct "%z %z %z" (GetFormIDString rItem) sText (GetFormIDString (' + read + ")))", True)


def _lu1arr(pid, pre, expr):
    """A list as "size: index=form ...", in the array's own order."""
    return _lu1(pid, pre + f"let aList := {expr}\nlet aKeys := ar_Keys aList\nlet fA := ar_Size aKeys\n"
                'let sText := sv_Construct "%g:" fA\nlet fB := 0\nwhile fB < fA\nlet fC := aKeys[fB]\nlet rItem := aList[fC]\n'
                'let sText := sv_Construct "%z %g=%z" sText fC (GetFormIDString rItem)\nlet fB += 1\nloop\nSetFunctionValue sText', True)


def _lu1f(*keys):
    return "".join(_form(v, LU1[k]) for v, k in zip(("rObj", "rOther", "rKey"), keys))


LUTANA1_PROBES = (
    # ---- 1. challenges
    [_lu1v(f"chal.type.{k}", k, "GetChallengeType rObj") for k in ("killone", "starts", "vault34", "caravan", "freerad")]
    + [_lu1v(f"chal.{f.lower()}.{k}", k, f"GetChallenge{f} rObj") for k in ("killone", "starts")
       for f in ("Threshold", "Interval", "Flags", "Value1", "Value2", "Value3")]
    + [_lu1v("chal.form1.vault34", "vault34", "GetChallengeForm1 rObj", "form"),
       _lu1v("chal.form2.aiming", "aiming", "GetChallengeForm2 rObj", "form"),
       _lu1v("chal.form1.none", "killone", "GetChallengeForm1 rObj", "form"),
       # Not a challenge: 0 [hypothesis].
       _lu1v("chal.threshold.notchal", "caps", "GetChallengeThreshold rObj")]
    + [_lu1set(f"chal.set{f.lower()}.{n}", "killone", f"GetChallenge{f} rObj", f"SetChallenge{f} rObj {v}",
               f"SetChallenge{f} rObj fSave")
       for f, n, v in (("Threshold", "300", "300"), ("Threshold", "neg", "-5"), ("Threshold", "frac", "12.7"),
                       ("Interval", "7", "7"), ("Flags", "5", "5"), ("Value1", "3", "3"), ("Value2", "9", "9"),
                       ("Value3", "2", "2"))]
    + [_lu1fset("chal.setform1", "freerad", "stimpak", "GetChallengeForm1 rObj", "SetChallengeForm1 rObj rOther",
                "SetChallengeForm1 rObj rItem"),
       _lu1fset("chal.setform2", "aiming", "stimpak", "GetChallengeForm2 rObj", "SetChallengeForm2 rObj rOther",
                "SetChallengeForm2 rObj rItem")]
    # ---- 2. casinos
    + [_lu1v(f"csno.{f.lower()}.{k}", k, f"GetCasino{f} rObj") for k in ("wrangler", "gomorrah")
       for f in ("BJ17Stand", "BJPayout", "MaxWinnings", "NumDecks", "ShufflePercent")]
    # Symbols 1-7; 0 and 8 outside [hypothesis: 0].
    + [_lu1v(f"csno.reel.{s}", "gomorrah", f"GetCasinoReelStops rObj {s}") for s in (1, 7, 0, 8)]
    + [_lu1set(f"csno.set{f.lower()}.{n}", "gomorrah", f"GetCasino{f} rObj", f"SetCasino{f} rObj {v}",
               f"SetCasino{f} rObj fSave")
       for f, n, v in (("BJ17Stand", "1", "1"), ("BJ17Stand", "5", "5"), ("BJPayout", "2", "2.25"),
                       ("MaxWinnings", "500", "500"), ("MaxWinnings", "neg", "-1"), ("NumDecks", "6", "6"),
                       ("NumDecks", "0", "0"), ("ShufflePercent", "half", "0.5"), ("ShufflePercent", "over", "1.5"),
                       ("ShufflePercent", "neg", "-0.2"))]
    + [_lu1set(f"csno.setreel.{s}.{v}", "gomorrah", f"GetCasinoReelStops rObj {s}", f"SetCasinoReelStops rObj {s} {v}",
               f"SetCasinoReelStops rObj {s} fSave") for s, v in ((4, 1), (7, 9), (8, 3))]
    # ---- 3. recipes
    + [_lu1o("rcpe.incount.meat", "steak", "meat", "GetRecipeInputCount rObj rOther"),
       _lu1o("rcpe.incount.caps", "steak", "caps", "GetRecipeInputCount rObj rOther"),
       _lu1o("rcpe.outcount.steak", "steak", "steakout", "GetRecipeOutputCount rObj rOther"),
       _lu1o("rcpe.outcount.water4", "masswater", "water151", "GetRecipeOutputCount rObj rOther"),
       _lu1v("rcpe.skill.steak", "steak", "GetRecipeRequiredSkill rObj"),
       _lu1v("rcpe.level.steak", "steak", "GetRecipeRequiredSkillLevel rObj"),
       # DATA skill -1 "none" [hypothesis: -1, level 0].
       _lu1v("rcpe.skill.powder", "powder", "GetRecipeRequiredSkill rObj"),
       _lu1v("rcpe.level.powder", "powder", "GetRecipeRequiredSkillLevel rObj")]
    + [_lu1set("rcpe.setskill.40", "steak", "GetRecipeRequiredSkill rObj", "SetRecipeRequiredSkill rObj 40",
               "SetRecipeRequiredSkill rObj fSave"),
       # A code below 0 [hypothesis: removes the skill]; level read after, both put back.
       _lu1("rcpe.setskill.neg", _lu1f("steak") + "let fSave := GetRecipeRequiredSkillLevel rObj\n"
            "SetRecipeRequiredSkill rObj -1\nlet fA := GetRecipeRequiredSkill rObj\nlet fB := GetRecipeRequiredSkillLevel rObj\n"
            "SetRecipeRequiredSkill rObj 44\nSetRecipeRequiredSkillLevel rObj fSave\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fA fB (GetRecipeRequiredSkill rObj) (GetRecipeRequiredSkillLevel rObj))', True),
       _lu1set("rcpe.setlevel.75", "steak", "GetRecipeRequiredSkillLevel rObj", "SetRecipeRequiredSkillLevel rObj 75",
               "SetRecipeRequiredSkillLevel rObj fSave"),
       # A recipe without a skill [hypothesis: nothing changes].
       _lu1set("rcpe.setlevel.noskill", "powder", "GetRecipeRequiredSkillLevel rObj", "SetRecipeRequiredSkillLevel rObj 30",
               "SetRecipeRequiredSkillLevel rObj fSave")]
    # Add a new input (at the end?), again (summed or a second entry?), a count of 0, then
    # remove it: counts and the input list after each step.
    + [_lu1("rcpe.addinput", _lu1f("steak", "caps") + "AddRecipeInputForm rObj rOther 3\nlet fA := GetRecipeInputCount rObj rOther\n"
            "AddRecipeInputForm rObj rOther 2\nlet fB := GetRecipeInputCount rObj rOther\nlet aList := GetRecipeInputForms rObj\n"
            "let fC := ar_Size aList\nRemoveRecipeInputForm rObj rOther\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g %g" fA fB fC (GetRecipeInputCount rObj rOther) (ar_Size (GetRecipeInputForms rObj)))', True),
       _lu1arr("rcpe.addinput.order", _lu1f("steak", "caps") + "AddRecipeInputForm rObj rOther 1\n",
               "GetRecipeInputForms rObj"),
       _lu1("rcpe.addinput.order.restore", _lu1f("steak", "caps") + "RemoveRecipeInputForm rObj rOther\n"
            "SetFunctionValue (ar_Size (GetRecipeInputForms rObj))"),
       _lu1("rcpe.addinput.zero", _lu1f("steak", "caps") + "AddRecipeInputForm rObj rOther 0\nlet fA := GetRecipeInputCount rObj rOther\n"
            "let fB := ar_Size (GetRecipeInputForms rObj)\nRemoveRecipeInputForm rObj rOther\n"
            'SetFunctionValue (sv_Construct "%g %g %g" fA fB (ar_Size (GetRecipeInputForms rObj)))', True),
       # An item already listed: added to its count [hypothesis] or a second entry; put
       # back by removing it and adding the one.
       _lu1("rcpe.addinput.existing", _lu1f("steak", "meat") + "AddRecipeInputForm rObj rOther 2\nlet fA := GetRecipeInputCount rObj rOther\n"
            "let fB := ar_Size (GetRecipeInputForms rObj)\nRemoveRecipeInputForm rObj rOther\nlet fC := ar_Size (GetRecipeInputForms rObj)\n"
            "AddRecipeInputForm rObj rOther 1\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g %g" fA fB fC (GetRecipeInputCount rObj rOther) (ar_Size (GetRecipeInputForms rObj)))', True),
       _lu1("rcpe.replaceinput", _lu1f("steak", "meat", "caps") + "ReplaceRecipeInputForm rObj rOther rKey\n"
            "let fA := GetRecipeInputCount rObj rOther\nlet fB := GetRecipeInputCount rObj rKey\nReplaceRecipeInputForm rObj rKey rOther\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fA fB (GetRecipeInputCount rObj rOther) (GetRecipeInputCount rObj rKey))', True)]
    + [_lu1set(f"rcpe.setincount.{v}", "steak", "GetRecipeInputCount rObj rOther", f"SetRecipeInputCount rObj rOther {v}",
               "SetRecipeInputCount rObj rOther fSave", "meat") for v in (3, 0)]
    # An item not listed [hypothesis: not added].
    + [_lu1("rcpe.setincount.unlisted", _lu1f("steak", "caps") + "SetRecipeInputCount rObj rOther 2\nlet fA := GetRecipeInputCount rObj rOther\n"
            "let fB := ar_Size (GetRecipeInputForms rObj)\nif fA > 0\nRemoveRecipeInputForm rObj rOther\nendif\n"
            'SetFunctionValue (sv_Construct "%g %g %g" fA fB (GetRecipeInputCount rObj rOther))', True),
       _lu1("rcpe.addoutput", _lu1f("steak", "caps") + "AddRecipeOutputForm rObj rOther 2\nlet fA := GetRecipeOutputCount rObj rOther\n"
            "let fB := ar_Size (GetRecipeOutputForms rObj)\nRemoveRecipeOutputForm rObj rOther\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fA fB (GetRecipeOutputCount rObj rOther) (ar_Size (GetRecipeOutputForms rObj)))', True),
       _lu1("rcpe.replaceoutput", _lu1f("steak", "steakout", "caps") + "ReplaceRecipeOutputForm rObj rOther rKey\n"
            "let fA := GetRecipeOutputCount rObj rOther\nlet fB := GetRecipeOutputCount rObj rKey\nReplaceRecipeOutputForm rObj rKey rOther\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fA fB (GetRecipeOutputCount rObj rOther) (GetRecipeOutputCount rObj rKey))', True)]
    + [_lu1set(f"rcpe.setoutcount.{v}", "steak", "GetRecipeOutputCount rObj rOther", f"SetRecipeOutputCount rObj rOther {v}",
               "SetRecipeOutputCount rObj rOther fSave", "steakout") for v in (4, 0)]
    # GetFormRecipes / GetFormRecipeOutputs: the page's SpareParts example, with the
    # category and a subcategory; the order of a long list (Lead) [hypothesis: FormID];
    # outputs each once (DrainedSmallEnergyCell's two recipes make one item).
    + [_lu1arr("rcpe.formrecipes.spare", _lu1f("spare"), "GetFormRecipes rObj"),
       _lu1arr("rcpe.formrecipes.spare.bench", _lu1f("spare", "reloadbench"), "GetFormRecipes rObj rOther"),
       _lu1arr("rcpe.formrecipes.spare.breakdown", _lu1f("spare", "breakdown"), "GetFormRecipes rObj rOther"),
       _lu1arr("rcpe.formrecipes.broc", _lu1f("broc"), "GetFormRecipes rObj"),
       _lu1arr("rcpe.formrecipes.broc.campfire", _lu1f("broc", "campfire"), "GetFormRecipes rObj rOther"),
       _lu1arr("rcpe.formrecipes.lead", _lu1f("lead"), "GetFormRecipes rObj"),
       _lu1arr("rcpe.formrecipes.caps", _lu1f("caps"), "GetFormRecipes rObj"),
       _lu1arr("rcpe.recipeoutputs.spare", _lu1f("spare"), "GetFormRecipeOutputs rObj"),
       _lu1arr("rcpe.recipeoutputs.spare.bench", _lu1f("spare", "reloadbench"), "GetFormRecipeOutputs rObj rOther"),
       _lu1arr("rcpe.recipeoutputs.drained", _lu1f("drained"), "GetFormRecipeOutputs rObj"),
       _lu1arr("rcpe.recipeoutputs.broc.aid", _lu1f("broc", "aid"), "GetFormRecipeOutputs rObj rOther")]
    # ---- 4. actors
    + [_lu1v("actor.haseffect.alcohol", "alcohol", "rP.ActorHasEffect rObj"),
       # Not a base effect.
       _lu1v("actor.haseffect.caps", "caps", "rP.ActorHasEffect rObj")]
    + [_lu1v(f"actor.service.doc.{b}", "doc", f"GetServiceFlag rObj {b}") for b in (1, 1024, 131072, 3, 0)]
    # Own AI data 16 (Food), template's 1148 (Alcohol … Misc, no Armor) with "Use AI Data"
    # [hypothesis: the template's].
    + [_lu1v(f"actor.service.tender2.{b}", "tender2", f"GetServiceFlag rObj {b}") for b in (4, 16)]
    + [_lu1set("actor.setservice.doc", "doc", "GetServiceFlag rObj 131072", "SetServiceFlag rObj 131072 1",
               "SetServiceFlag rObj 131072 fSave"),
       _lu1set("actor.setservice.doc.clear", "doc", "GetServiceFlag rObj 16", "SetServiceFlag rObj 16 0",
               "SetServiceFlag rObj 16 fSave"),
       # Which record a templated NPC's change lands on: the NPC's and its template's.
       _lu1("actor.setservice.tender2", _lu1f("tender2", "tender1") + "SetServiceFlag rObj 2 1\n"
            "let fA := GetServiceFlag rObj 2\nlet fB := GetServiceFlag rOther 2\nSetServiceFlag rObj 2 0\nSetServiceFlag rOther 2 0\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fA fB (GetServiceFlag rObj 2) (GetServiceFlag rOther 2))', True)]
    + [_lu1v(f"actor.aggro.{k}", k, "LNGetAggroRadius rObj") for k in ("sniper", "doc")]
    + [_lu1v("actor.aggro.ref", "docref", "rObj.LNGetAggroRadius"),
       _lu1set("actor.setaggro.500", "sniper", "LNGetAggroRadius rObj", "LNSetAggroRadius 500 rObj", "LNSetAggroRadius fSave rObj"),
       _lu1set("actor.setaggro.neg", "sniper", "LNGetAggroRadius rObj", "LNSetAggroRadius -5 rObj", "LNSetAggroRadius fSave rObj")]
    + [_lu1v(f"actor.combatstyle.{k}", k, "GetCombatStyle rObj", "form") for k in ("boone", "doc")]
    # A reference: Doc (no ZNAM) [hypothesis: 0, no DefaultCombatstyle], Cachino, the player.
    + [_lu1v(f"actor.combatstyle.ref.{k}", k, "rObj.GetCombatStyle", "form") for k in ("docref", "cachino")]
    + [_lu1("actor.combatstyle.player", "SetFunctionValue (GetFormIDString (rP.GetCombatStyle))", True)]
    # ---- 5. perks, worlds, cells, water, zones, leveled lists
    + [_lu1v(f"perk.istrait.{k}", k, "IsTrait rObj") for k in ("foureyes", "actionboy", "caps")]
    + [_lu1v(f"perk.entryform.{k}.{i}", k, f"GetNthPerkEntryForm rObj {i}", "form")
       for k, i in (("actionboy", 0), ("actionboy", 1), ("actionboy", 2), ("actionboy", -1), ("builtto", 0),
                    ("builtto", 1), ("fortune", 0))]
    + [_lu1v(f"world.parent.{k}", k, "GetWorldspaceParentWorldspace rObj", "form") for k in ("freeside", "wasteland")]
    + [_lu1v("world.parent.alias", "mini", "GetWorldParentWorld rObj", "form")]
    + [_lu1v(f"cell.waterform.{k}", k, "GetCellWaterForm rObj", "form") for k in ("startcell", "doccell")]
    + [_lu1("cell.waterform.none", "SetFunctionValue (GetFormIDString (GetCellWaterForm))", True)]
    # XCLW 3.4e38 in the start cell [hypothesis: the world's -2300]; the interior's
    # -2147483648; no cell, the player's.
    + [_lu1v(f"cell.waterheight.{k}", k, "GetCellWaterHeight rObj") for k in ("startcell", "doccell")]
    + [_lu1("cell.waterheight.none", "SetFunctionValue (GetCellWaterHeight)"),
       _lu1set("cell.setwaterheight", "doccell", "GetCellWaterHeight rObj", "SetCellWaterHeight 100.5 rObj",
               "SetCellWaterHeight fSave rObj")]
    # A string map "x"/"y"; an interior [hypothesis: an empty map].
    + [_lu1(f"cell.coords.{k}", _lu1f(k) + "let aList := GetCellCoords rObj\nlet fA := ar_Size aList\nlet fB := -999\nlet fC := -999\n"
            'if fA > 0\nlet fB := aList["x"]\nlet fC := aList["y"]\nendif\nSetFunctionValue (sv_Construct "%g %g %g" fA fB fC)', True)
       for k in ("startcell", "doccell")]
    + [_lu1("cell.coords.none", "let aList := GetCellCoords\nlet fA := ar_Size aList\nlet fB := -999\nlet fC := -999\n"
            'if fA > 0\nlet fB := aList["x"]\nlet fC := aList["y"]\nendif\nSetFunctionValue (sv_Construct "%g %g %g" fA fB fC)', True),
       _lu1v("water.effect.clean", "cleanwater", "GetWaterFormEffect rObj", "form"),
       # No water [hypothesis: the player's cell's water, Potomac's WaterHeal4Bad].
       _lu1("water.effect.none", "SetFunctionValue (GetFormIDString (GetWaterFormEffect))", True)]
    + [_lu1v(f"zone.ref.{k}", k, "rObj.GetZone", "form") for k in ("cachino", "docref")]
    + [_lu1("zone.player", "SetFunctionValue (GetFormIDString (rP.GetZone))", True)]
    + [_lu1v(f"zone.flag.{k}.{b}", k, f"GetZoneFlag rObj {b}") for k in ("vault11zone", "gomzone") for b in (1, 2, 3)]
    + [_lu1set(f"zone.setflag.{b}", "gomzone", f"GetZoneFlag rObj {b}", f"SetZoneFlag rObj {b} 1", f"SetZoneFlag rObj {b} fSave")
       for b in (1, 2)]
    + [_lu1v(f"lvl.chancenoneglobal.{k}", k, "GetChanceNoneGlobal rObj", "form") for k in ("capslist", "outfitlist")]
    # ---- 6. activators, terminals, locks
    + [_lu1v(f"acti.radio.{k}", k, "GetActivatorRadioStation rObj", "form") for k in ("gsradio", "gasvalve")]
    + [_lu1fset("acti.setradio", "gsradio", "rnv", "GetActivatorRadioStation rObj", "SetActivatorRadioStation rOther rObj",
                "SetActivatorRadioStation rItem rObj"),
       _lu1fset("acti.setsoundactivate", "valve", "radiosound", "GetActivatorSoundActivate rObj",
                "SetActivatorSoundActivate rOther rObj", "SetActivatorSoundActivate rItem rObj"),
       _lu1fset("acti.setsoundlooping", "valve", "radiosound", "GetActivatorSoundLooping rObj",
                "SetActivatorSoundLooping rOther rObj", "SetActivatorSoundLooping rItem rObj"),
       # Not an activator (a terminal) [hypothesis: nothing changes, reads 0].
       _lu1fset("acti.setradio.term", "fisto", "rnv", "GetActivatorRadioStation rObj", "SetActivatorRadioStation rOther rObj",
                "SetActivatorRadioStation rItem rObj")]
    + [_lu1v(f"term.lock.{k}", k, "GetTerminalLock rObj") for k in ("fisto", "commterm", "selfdestruct", "manny")]
    + [_lu1v("term.lock.ref", "fistoref", "rObj.GetTerminalLock"),
       _lu1v("term.note.fisto", "fisto", "GetPasswordNote rObj", "form"),
       _lu1v("term.note.manny", "manny", "GetPasswordNote rObj", "form"),
       _lu1v("term.note.ref", "fistoref", "rObj.GetPasswordNote", "form"),
       _lu1fset("term.setnote", "fisto", "epicure", "GetPasswordNote rObj", "SetPasswordNote rOther rObj", "SetPasswordNote rItem rObj")]
    + [_lu1v(f"lock.lockedout.{k}", k, "GetLockedOut rObj") for k in ("zoaradoor", "jaildoor", "fistoref", "docdoor")]
    + [_lu1v("lock.lockedout.calling", "jaildoor", "rObj.GetLockedOut"),
       # 1, 2, then 7 [hypothesis: nothing], then 0, on a locked door and on a terminal.
       _lu1("lock.setlockedout.door", _lu1f("zoaradoor") + "let fSave := GetLockedOut rObj\nSetLockedOut 1 rObj\nlet fA := GetLockedOut rObj\n"
            "SetLockedOut 2 rObj\nlet fB := GetLockedOut rObj\nSetLockedOut 7 rObj\nlet fC := GetLockedOut rObj\nSetLockedOut fSave rObj\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g %g" fSave fA fB fC (GetLockedOut rObj))', True),
       _lu1("lock.setlockedout.term", _lu1f("fistoref") + "let fSave := GetLockedOut rObj\nSetLockedOut 1 rObj\nlet fA := GetLockedOut rObj\n"
            "SetLockedOut 2 rObj\nlet fB := GetLockedOut rObj\nSetLockedOut fSave rObj\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fA fB (GetLockedOut rObj))', True),
       # The key carried, then not; the key added silently and taken back.
       _lu1("lock.haskey.door", _lu1f("zoaradoor", "caps", "zoarakey") + "let fSave := rP.GetItemCount rKey\nlet fA := rObj.PlayerHasKey\n"
            "rP.AddItem rKey 1 1\nlet fB := rObj.PlayerHasKey\nrP.RemoveItem rKey 1 1\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g %g" fSave fA fB (rObj.PlayerHasKey) (rP.GetItemCount rKey))', True),
       _lu1v("lock.haskey.term", "fistoref", "rObj.PlayerHasKey"),
       _lu1v("lock.haskey.nolock", "docdoor", "rObj.PlayerHasKey")]
    + [_lu1v(f"apd.{k}", k, "IsParentActivateOnly rObj") for k in ("elevator", "docref")]
    + [_lu1v("apd.calling", "elevator", "rObj.IsParentActivateOnly")]
    # ---- 7. flags, playability, weapons, load screens, misc
    # The dresser's record flags 0x2000400; 16 and 8 are runtime flags in the page's table
    # [hypothesis: the game sets "Form initialized" on loaded forms].
    + [_lu1v(f"flag.dresser.{b}", "dresser", f"GetFormFlag rObj {b}") for b in (1024, 33554432, 33555456, 32, 16, 8, 0)]
    + [_lu1v(f"flag.pistol.{b}", "pistol", f"GetFormFlag rObj {b}") for b in (16, 64)]
    + [_lu1set("flag.set.2", "killone", "GetFormFlag rObj 2", "SetFormFlag rObj 2 1", "SetFormFlag rObj 2 fSave")]
    + [_lu1v(f"playable.{k}", k, "LNIsPlayable rObj") for k in ("cassgun", "pistol", "caucasian", "ghoul", "haircurly",
                                                                  "hairwavyold", "eyeblue", "eyereanimate", "actionboy",
                                                                  "calmheart", "caps")]
    + [_lu1set(f"playable.set.{k}", k, "LNIsPlayable rObj", f"LNSetIsPlayable {v} rObj", "LNSetIsPlayable fSave rObj")
       for k, v in (("caucasian", 0), ("hairwavyold", 1), ("eyereanimate", 1), ("calmheart", 1), ("pistol", 0))]
    + [_lu1set("weap.setcritflags.0", "laser", "GetWeaponCritFlags rObj", "SetWeaponCritFlags rObj 0", "SetWeaponCritFlags rObj fSave"),
       _lu1set("weap.setcritflags.300", "laser", "GetWeaponCritFlags rObj", "SetWeaponCritFlags rObj 300", "SetWeaponCritFlags rObj fSave"),
       _lu1set("weap.setkillimpulse", "laser", "GetWeaponKillImpulse rObj", "SetWeaponKillImpulse rObj 15.3",
               "SetWeaponKillImpulse rObj fSave"),
       _lu1set("weap.setimpulsedist", "laser", "GetWeaponImpulseDistance rObj", "SetWeaponImpulseDistance rObj 3.1",
               "SetWeaponImpulseDistance rObj fSave"),
       _lu1fset("weap.setvatseffect", "sledge", "withdrawal", "GetWeaponVATSEffect rObj", "SetWeaponVATSEffect rObj rOther",
                "SetWeaponVATSEffect rObj rItem"),
       _lu1v("lscr.type", "loadscreen", "GetLoadScreenType rObj", "form"),
       _lu1fset("lscr.settype", "loadscreen", "loadtype", "GetLoadScreenType rObj", "SetLoadScreenType rObj rOther",
                "SetLoadScreenType rObj rItem"),
       # Not a load screen type [hypothesis: refused].
       _lu1fset("lscr.settype.caps", "loadscreen", "caps", "GetLoadScreenType rObj", "SetLoadScreenType rObj rOther",
                "SetLoadScreenType rObj rItem"),
       _lu1("misc.pccanusepa", "SetFunctionValue (GetPCCanUsePowerArmor)"),
       _lu1("misc.defaultmessagetime", "SetFunctionValue (GetDefaultMessageTime)"),
       # The third name JIP LN may not register [observed: the plugin's strings]; alone.
       _lu1("misc.messagedisplaytime", "SetFunctionValue (GetMessageDisplayTime)"),
       _lu1("misc.mdt", "SetFunctionValue (GetMDT)"),
       _lu1("misc.setdmt", "let fSave := GetDefaultMessageTime\nSetDefaultMessageTime 3.5\nlet fA := GetDefaultMessageTime\n"
            "SetDMT 0\nlet fB := GetDefaultMessageTime\nSetDMT -2\nlet fC := GetDefaultMessageTime\nSetDefaultMessageTime fSave\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g %g" fSave fA fB fC (GetDefaultMessageTime))', True),
       _lu1("misc.addnotens", _lu1f("epicure") + "let fSave := GetHasNote rObj\nAddNoteNS rObj\nlet fA := GetHasNote rObj\n"
            "if fSave == 0\nRemoveNote rObj\nendif\n"
            'SetFunctionValue (sv_Construct "%g %g %g" fSave fA (GetHasNote rObj))', True),
       # The format only: the value is the clock's.
       _lu1("misc.timestamp", "SetFunctionValue (GetTimeStamp)", True),
       _lu1("misc.timestamp.len", "let sText := GetTimeStamp\nSetFunctionValue (sv_Length sText)"),
       _lu1v("misc.crosshairinlist", "followers", "CrosshairRefInList rObj")]
    + [_lu1v(f"radius.base.{k}", k, "GetRadius rObj") for k in ("officelight", "fire420")]
    # A reference's XRDS of -16.59 [hypothesis: read as it is]; the player (none).
    + [_lu1v("radius.ref.fire", "fireref", "rObj.GetRadius"),
       _lu1("radius.ref.player", "SetFunctionValue (rP.GetRadius)")]
    + [_lu1set(f"radius.set.base.{n}", "officelight", "GetRadius rObj", f"SetRadius {v} rObj", "SetRadius fSave rObj")
       for n, v in (("300", "300"), ("frac", "12.5"), ("neg", "-5"))]
    + [_lu1set("radius.set.ref", "fireref", "rObj.GetRadius", "rObj.SetRadius 50", "rObj.SetRadius fSave")]
)


# Set jg3: JohnnyGuitar batch 3 (docs/spec-notes/ext-jg3.md; #1007). Forms are
# FalloutNV.esm records checked with fnvtool esm inventory / decode [observed 2026-11-08]:
#  EFSH NukeAtomShader 446D3 (ICON "", ICO2 Effects\FXDustSmallGen01.dds, NAM7 set; fill
#   colour 75,222,39, edge 154,222,131; z test "Equal To"), fxCateye 15FF5F (flags 8).
#  MSET musSetINCFreeside 16BA51 (Incidental, HNAM=INAM musINCCreepy 16B41B, no NAM2-7),
#   musSetLOCCorpRuinsDayABC 176E3F (Location, NAM2-7, PNAM 63, KNAM 66.6),
#   musSetBTTLRuralGood4Full 14E8AC (Battle, PNAM 80 with an unknown bit 0x40, HNAM 14E8DA,
#   INAM 14E8DB), musSetOldFO3Dungeon3 166BFD (Location, only NAM2 and NAM5).
#  CSNO Gomorrah 140521 (chip 139FE0), Tops 144C3F (chip 144C3B), UltraLuxe 144C40,
#   VikkiVance 144C41, AtomicWrangler 144C42; CHIP UltraLuxeChip 144C3C.
#  TERM V34SecurityNorthTerminal 13253C (5 items; INAM on 0-3, TNAM 13253D on 4, RNAM ""),
#   GameTerminalSubMenuEast 9596E (10 items, ANAM 2 on 0-7, item 8 RNAM "You walk to the
#   west..."), SupplyOfficerContrerasTerminal EFB7B (one item, ANAM 3),
#   NellisArrayTerminalCompleted 1672C6 (no items); NOTE V34FloodingAltWarningNote 132543;
#   TERM GameTerminalSubMenuNorth 95966.
#  QUST VERLegionVets 17A836 (delay 33), Patch04Quest00 17BA15 (0.1), ComplexSceneScriptPS3
#   17B713 (0.001), VCG02 10A214 (0).
# Every setter row puts the old value back before it ends.

JG3 = {"nuke": "446D3", "cateye": "15FF5F", "freeside": "16BA51", "corp": "176E3F", "bttl": "14E8AC",
       "dung": "166BFD", "creepy": "16B41B", "gomorrah": "140521", "tops": "144C3F", "ultra": "144C40",
       "vikki": "144C41", "wrangler": "144C42", "ultrachip": "144C3C", "v34": "13253C", "east": "9596E",
       "contreras": "EFB7B", "nellis": "1672C6", "altnote": "132543", "northterm": "95966",
       "legion": "17A836", "patch04": "17BA15", "complex": "17B713", "vcg02": "10A214", "pistol": "434F",
       "caps": "F"}


def _jg3(pid, body, text=False):
    return (f"jg3.{pid}", "ref rP\nref rObj\nref rOther\nref rItem\nref rNone\nfloat fRes\nfloat fSave\nfloat fA\nfloat fB\n"
            "string_var sText\nstring_var sSave\nBegin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


def _jg3v(pid, key, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return _jg3(pid, _form("rObj", JG3[key]) + rd, bool(text))


# A numeric setter: old value, the setter's result (or "-" for a void setter), the value
# read back, then the old value put back and read again.
def _jg3set(pid, key, read, setter, restore, void=False):
    res = f"{setter}\nlet fRes := -1\n" if void else f"let fRes := {setter}\n"
    return _jg3(pid, _form("rObj", JG3[key]) + f"let fSave := {read}\n{res}let fA := {read}\n{restore}\n"
                'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fRes fA (' + read + "))", True)


EFSH_SET = (  # trait, new value [hypothesis targets in comments]
    (13, 1.5),   # documented float
    (0, 2.7),    # flags keep whole numbers
    (1, 5.5),    # an enum keeps whole numbers
    (4, 255),    # the fill colour: how one number makes three channels
    (42, 7.25),  # the last trait the setter's page lists
    (43, 9),     # past the setter's 0-42
    (47, 1),     # past every table
)

JG3_PROBES = (
    # ---- effect shader traits: every id, the colours (4, 14) and past the table
    [_jg3v(f"efsh.trait.nuke.{t}", "nuke", f"GetEffectShaderTraitNumeric rObj {t}") for t in list(range(0, 48)) + [-1]]
    + [_jg3v(f"efsh.trait.cateye.{t}", "cateye", f"GetEffectShaderTraitNumeric rObj {t}") for t in (0, 4, 13, 14, 23, 27)]
    + [_jg3v("efsh.trait.notefsh", "freeside", "GetEffectShaderTraitNumeric rObj 0")]
    + [_jg3set(f"efsh.set.nuke.{t}", "nuke", f"GetEffectShaderTraitNumeric rObj {t}",
               f"SetEffectShaderTraitNumeric rObj {t} {v}", f"SetEffectShaderTraitNumeric rObj {t} fSave") for t, v in EFSH_SET]
    + [_jg3("efsh.set.notefsh", _form("rObj", JG3["freeside"]) + "let fRes := SetEffectShaderTraitNumeric rObj 13 1.5\n"
            'SetFunctionValue (sv_Construct "%g %g" fRes (GetMediaSetTraitNumeric rObj 0))', True)]
    # ---- effect shader textures: "" for a missing one, past the table
    + [_jg3v(f"efsh.tex.nuke.{i}", "nuke", f"GetEffectShaderTexturePath rObj {i}", True) for i in (0, 1, 2, 3, -1)]
    + [_jg3v("efsh.tex.cateye.1", "cateye", "GetEffectShaderTexturePath rObj 1", True)]
    + [_jg3("efsh.settex.nuke.1", _form("rObj", JG3["nuke"])
            + 'SetEffectShaderTexturePath rObj 1 "Effects\\NvfProbe.dds"\nlet sText := GetEffectShaderTexturePath rObj 1\n'
            'SetEffectShaderTexturePath rObj 1 "Effects\\FXDustSmallGen01.dds"\n'
            'SetFunctionValue (sv_Construct "[%z] [%z]" sText (GetEffectShaderTexturePath rObj 1))', True)]
    # ---- media sets: numeric traits (the type's numbers: -1 none, 0 battle .. 3 incidental)
    + [_jg3v(f"mset.num.{k}.{t}", k, f"GetMediaSetTraitNumeric rObj {t}") for k in ("freeside", "corp") for t in range(0, 18)]
    + [_jg3v(f"mset.num.{k}.{t}", k, f"GetMediaSetTraitNumeric rObj {t}")
       for k, t in (("bttl", 0), ("bttl", 13), ("dung", 0), ("corp", 18), ("corp", -1))]
    + [_jg3v("mset.num.notmset", "nuke", "GetMediaSetTraitNumeric rObj 1")]
    + [_jg3set(f"mset.set.corp.{t}", "corp", f"GetMediaSetTraitNumeric rObj {t}", f"SetMediaSetTraitNumeric rObj {t} {v}",
               f"SetMediaSetTraitNumeric rObj {t} fSave", void=True) for t, v in ((8, 50.5), (0, 2.6), (13, 5.5), (14, 7.25))]
    # ---- media sets: sound paths, missing ones and past the table
    + [_jg3v(f"mset.str.corp.{t}", "corp", f"GetMediaSetTraitString rObj {t}", True) for t in (0, 1, 2, 3, 4, 5, 6, -1)]
    + [_jg3v(f"mset.str.dung.{t}", "dung", f"GetMediaSetTraitString rObj {t}", True) for t in (0, 1, 3)]
    + [_jg3("mset.setstr.corp.0", _form("rObj", JG3["corp"])
            + 'SetMediaSetTraitString rObj 0 "loc\\NvfProbe.mp3"\nlet sText := GetMediaSetTraitString rObj 0\n'
            'SetMediaSetTraitString rObj 0 "loc\\corporateruins\\mus_loc_corpruins_day_1low.mp3"\n'
            'SetFunctionValue (sv_Construct "[%z] [%z]" sText (GetMediaSetTraitString rObj 0))', True)]
    # ---- media sets: sound forms; a non-sound form and none given to the setter
    + [_jg3v(f"mset.sound.{k}.{t}", k, f"GetMediaSetTraitSound rObj {t}", "form")
       for k, t in (("bttl", 0), ("bttl", 1), ("freeside", 0), ("freeside", 1), ("bttl", 2))]  # ("corp", 0) crashed the game [observed 2026-11-08]
    + [_jg3(f"mset.setsound.bttl.{n}", _form("rObj", JG3["bttl"]) + _form("rOther", JG3[o])
            + "let rItem := GetMediaSetTraitSound rObj 0\nSetMediaSetTraitSound rObj 0 rOther\nlet sText := GetFormIDString (GetMediaSetTraitSound rObj 0)\n"
            "SetMediaSetTraitSound rObj 0 rItem\n"
            'SetFunctionValue (sv_Construct "%z %z %z" (GetFormIDString rItem) sText (GetFormIDString (GetMediaSetTraitSound rObj 0)))', True)
       for n, o in (("sound", "creepy"), ("notsound", "pistol"))]
    + [_jg3("mset.setsound.bttl.none", _form("rObj", JG3["bttl"])
            + "let rItem := GetMediaSetTraitSound rObj 1\nSetMediaSetTraitSound rObj 1 rNone\nlet sText := GetFormIDString (GetMediaSetTraitSound rObj 1)\n"
            "SetMediaSetTraitSound rObj 1 rItem\n"
            'SetFunctionValue (sv_Construct "%z %z %z" (GetFormIDString rItem) sText (GetFormIDString (GetMediaSetTraitSound rObj 1)))', True)]
    # ---- casinos: chips, winnings
    + [_jg3v(f"casino.chip.{k}", k, "GetCasinoChip rObj", "form") for k in ("gomorrah", "tops", "ultra", "vikki", "wrangler", "nuke")]
    + [_jg3(f"casino.setchip.tops.{n}", _form("rObj", JG3["tops"]) + _form("rOther", JG3[o])
            + "let rItem := GetCasinoChip rObj\nSetCasinoChip rObj rOther\nlet sText := GetFormIDString (GetCasinoChip rObj)\n"
            "SetCasinoChip rObj rItem\n"
            'SetFunctionValue (sv_Construct "%z %z %z" (GetFormIDString rItem) sText (GetFormIDString (GetCasinoChip rObj)))', True)
       for n, o in (("chip", "ultrachip"), ("notchip", "caps"))]
    + [_jg3v(f"casino.win.{k}", k, "GetCasinoWinnings rObj") for k in ("gomorrah", "tops", "ultra", "vikki", "wrangler", "nuke")]
    + [_jg3set(f"casino.setwin.tops.{n}", "tops", "GetCasinoWinnings rObj", f"SetCasinoWinnings rObj {v}",
               "SetCasinoWinnings rObj fSave", void=True) for n, v in (("1234", 1234), ("frac", 2.7), ("neg", -50))]
    # ---- terminals: counts, texts (textID 1/2, a bad textID, past the list)
    + [_jg3v(f"term.count.{k}", k, "GetTerminalMenuItemCount rObj") for k in ("v34", "east", "contreras", "nellis", "corp")]
    + [_jg3v(f"term.text.{k}.{w}.{i}", k, f"GetTerminalMenuItemText rObj {w} {i}", True)
       for k, w, i in (("v34", 1, 0), ("v34", 2, 0), ("v34", 1, 4), ("v34", 0, 0),
                       ("v34", 3, 0), ("east", 2, 0), ("east", 2, 8))]
    # an item past the list (v34 5 or -1, nellis 0: it has none) crashed the game [observed 2026-11-08]
    + [_jg3v(f"term.note.v34.{i}", "v34", f"GetTerminalMenuItemNote rObj {i}", "form") for i in (0, 3, 4)]
    + [_jg3v(f"term.submenu.{k}.{i}", k, f"GetTerminalMenuItemSubmenu rObj {i}", "form") for k, i in (("v34", 0), ("v34", 4), ("east", 9))]
    + [_jg3v(f"term.flags.{k}.{i}", k, f"GetTerminalMenuItemFlags rObj {i}")
       for k, i in (("east", 0), ("east", 8), ("contreras", 0), ("v34", 0), ("v34", 7))]
    + [_jg3("term.settext.v34.1.4", _form("rObj", JG3["v34"])
            + 'SetTerminalMenuItemText rObj 1 4 "NvfProbe"\nlet sText := GetTerminalMenuItemText rObj 1 4\n'
            'SetTerminalMenuItemText rObj 1 4 "System"\n'
            'SetFunctionValue (sv_Construct "[%z] [%z]" sText (GetTerminalMenuItemText rObj 1 4))', True),
       _jg3("term.settext.east.2.8", _form("rObj", JG3["east"])
            + 'SetTerminalMenuItemText rObj 2 8 "NvfProbe"\nlet sText := GetTerminalMenuItemText rObj 2 8\n'
            'SetTerminalMenuItemText rObj 2 8 "You walk to the west..."\n'
            'SetFunctionValue (sv_Construct "[%z] [%z]" sText (GetTerminalMenuItemText rObj 2 8))', True)]
    + [_jg3(f"term.set{fn.lower()}.{n}", _form("rObj", JG3[k]) + _form("rOther", JG3[o])
            + f"let rItem := GetTerminalMenuItem{fn} rObj {i}\nSetTerminalMenuItem{fn} rObj {i} rOther\n"
            f"let sText := GetFormIDString (GetTerminalMenuItem{fn} rObj {i})\nSetTerminalMenuItem{fn} rObj {i} rItem\n"
            f'SetFunctionValue (sv_Construct "%z %z %z" (GetFormIDString rItem) sText (GetFormIDString (GetTerminalMenuItem{fn} rObj {i})))', True)
       for fn, n, k, i, o in (("Note", "v34.0", "v34", 0, "altnote"), ("Note", "v34.0.notnote", "v34", 0, "northterm"),
                              ("Submenu", "v34.4", "v34", 4, "northterm"), ("Submenu", "v34.4.notterm", "v34", 4, "altnote"))]
    + [_jg3set(f"term.setflags.{k}.{i}.{v}", k, f"GetTerminalMenuItemFlags rObj {i}", f"SetTerminalMenuItemFlags rObj {i} {v}",
               f"SetTerminalMenuItemFlags rObj {i} fSave", void=True) for k, i, v in (("east", 0, 7), ("v34", 4, 1), ("v34", 4, 6))]
    # ---- quest delay: the record's, 0 (the default), after SetQuestDelay, not a quest
    + [_jg3v(f"quest.delay.{k}", k, "GetQuestDelay rObj") for k in ("legion", "patch04", "complex", "vcg02", "corp")]
    + [_jg3set("quest.setdelay.legion", "legion", "GetQuestDelay rObj", "SetQuestDelay rObj 2.5", "SetQuestDelay rObj fSave", void=True)]
)


# Set jg4: JohnnyGuitar batch 4 (docs/spec-notes/ext-jg4.md; #1007). Forms are
# FalloutNV.esm records checked with fnvtool esm inventory / decode [observed 2026-11-08]:
#  IDLM TopsGuardMarker 17179B (flags 0, timer 0, 3 idles), VES03WastelandHunterIdleMarker12
#   15AD22 (flags 1, timer 3, 4 idles), DefaultSuperMutantIdleMarkerGroundGrab 63301
#   (flags 4, timer 5, 1 idle), VStripFluffPortMarker 16A940 (no IDLF/IDLT/IDLA);
#   IDLE LooseReadPaperOnHandA 5DB21, 2hrCrouch 269B8.
#  REGN NewVegasRegion 104160 (RDAT flags 1, priority 100; NVWastelandClearNight 176574 100
#   global VNight 176537, NVWastelandClear FFC88 100), GSWeatherRegion 123CE3 (flags 0,
#   priority 100; NVWastelandGS 1237D7 100), WeatherMegaton 73EB4 (flags 1; two weathers
#   at chance 0 with globals 73A79, 73A78), NVUrbanRegion 173566 (85/15),
#   VMapGoodspringsRegion 15FF5E and DC01 A0B (no weather block).
#  RACE Caucasian 19 (Playable), CaucasianChild 42C2 (Child), Ghoul 3B3E, TestQACaucasian
#   33184 (flags 0; the only race the setters touch).
#  FACT GoodspringsRingoFaction 162CAB, GoodspringsMilitiaFaction 15EC58, VBooneRogueFaction
#   152978, the mercenary faction 173EEF (only NVMercenaryMale lists it; NVMercenaryFemaleF
#   takes factions from it by template), NCR A46E7.
# Every setter row puts the old value back before it ends.

JG4 = {"tops": "17179B", "hunter": "15AD22", "grab": "63301", "nodata": "16A940", "paper": "5DB21", "crouch": "269B8",
       "nv": "104160", "gs": "123CE3", "megaton": "73EB4", "urban": "173566", "gsmap": "15FF5E", "dc01": "A0B",
       "clear": "FFC88", "clearnight": "176574", "gsweather": "1237D7", "vnight": "176537",
       "megw1": "64609", "megw2": "6A076", "megg1": "73A79", "megg2": "73A78",
       "caucasian": "19", "child": "42C2", "ghoul": "3B3E", "testqa": "33184",
       "ringo": "162CAB", "militia": "15EC58", "boonerogue": "152978", "mercfac": "173EEF", "ncr": "A46E7"}


def _jg4(pid, body, text=False):
    return (f"jg4.{pid}", "ref rP\nref rObj\nref rOther\nref rItem\nref rThird\nfloat fRes\nfloat fSave\nfloat fA\nfloat fB\nfloat fC\n"
            "int iA\nstring_var sText\nstring_var sSave\narray_var aList\narray_var aKeys\narray_var aSave\nBegin Function { }\n"
            + _form("rP", NF["player"]) + body, text, None, "")


def _jg4v(pid, key, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return _jg4(pid, _form("rObj", JG4[key]) + rd, bool(text))


# Appends aList as "size: key=form ..." to sText (keys in the array's own order).
_JG4_LIST = ('let aKeys := ar_Keys aList\nlet fA := ar_Size aKeys\nlet sText := sv_Construct "%z %g:" sText fA\nlet fB := 0\n'
             'while fB < fA\nlet fC := aKeys[fB]\nlet rItem := aList[fC]\n'
             'let sText := sv_Construct "%z %g=%z" sText fC (GetFormIDString rItem)\nlet fB += 1\nloop\n')


def _jg4arr(pid, key, expr, pre=""):
    return _jg4(pid, _form("rObj", JG4[key]) + pre + f'let sText := sv_Construct "a"\nlet aList := {expr}\n' + _JG4_LIST
                + "SetFunctionValue sText", True)


def _jg4set(pid, key, read, setter, restore):
    return _jg4(pid, _form("rObj", JG4[key]) + f"let fSave := {read}\n{setter}\nlet fA := {read}\n{restore}\n"
                'SetFunctionValue (sv_Construct "%g %g %g" fSave fA (' + read + "))", True)


# A region's weather list after `change`, then `undo`, then the list again.
def _jg4reg(pid, key, change, undo, extra=""):
    return _jg4(pid, _form("rObj", JG4[key]) + _form("rOther", JG4["clear"]) + _form("rThird", JG4["vnight"])
                + 'let sText := sv_Construct "a"\n' + change + "let aList := GetRegionWeathers rObj\n" + _JG4_LIST + extra
                + undo + "let aList := GetRegionWeathers rObj\n" + _JG4_LIST + "SetFunctionValue sText", True)


JG4_PROBES = (
    # ---- idle markers: animations (record order), traits 1-3 and outside the table
    [_jg4arr(f"idlm.anims.{k}", k, "GetIdleMarkerAnimations rObj") for k in ("tops", "hunter", "grab", "nodata", "ringo")]
    + [_jg4v(f"idlm.trait.{k}.{t}", k, f"GetIdleMarkerTraitNumeric rObj {t}") for k in ("tops", "hunter", "grab", "nodata") for t in range(0, 5)]
    + [_jg4set(f"idlm.settrait.hunter.{t}.{n}", "hunter", f"GetIdleMarkerTraitNumeric rObj {t}",
               f"SetIdleMarkerTraitNumeric rObj {t} {v}", f"SetIdleMarkerTraitNumeric rObj {t} fSave")
       for t, n, v in ((2, "7.5", 7.5), (2, "neg", -1), (1, "5", 5), (1, "2", 2), (1, "0", 0), (3, "1", 1), (0, "9", 9))]
    # one entry replaced, then put back; past the list; a form that is not an idle
    + [_jg4(f"idlm.setanim.hunter.{n}", _form("rObj", JG4["hunter"]) + _form("rOther", JG4[o])
            + "let aSave := GetIdleMarkerAnimations rObj\n" + (f"let rThird := aSave[{i}]\n" if 0 <= i < 4 else "")
            + f"SetIdleMarkerAnimation rObj {i} rOther\nlet aList := GetIdleMarkerAnimations rObj\n"
            'let sText := sv_Construct "a"\n' + _JG4_LIST
            + (f"SetIdleMarkerAnimation rObj {i} rThird\n" if 0 <= i < 4 else "let iA := aSave\nSetIdleMarkerAnimations rObj iA\n")
            + "let aList := GetIdleMarkerAnimations rObj\n" + _JG4_LIST + "SetFunctionValue sText", True)
       for n, i, o in (("0", 0, "paper"), ("3", 3, "paper"), ("9", 9, "paper"), ("neg", -1, "paper"), ("notidle", 0, "testqa"))]
    # the whole list from an array (the count follows), with a non-idle entry, then put back
    + [_jg4(f"idlm.setanims.tops.{n}", _form("rObj", JG4["tops"]) + _form("rOther", JG4["paper"]) + _form("rThird", JG4[t])
            + "let aSave := GetIdleMarkerAnimations rObj\nlet aList := ar_List rOther rThird rOther\nlet iA := aList\n"
            "SetIdleMarkerAnimations rObj iA\nlet fRes := GetIdleMarkerTraitNumeric rObj 3\nlet aList := GetIdleMarkerAnimations rObj\n"
            'let sText := sv_Construct "n%g" fRes\n' + _JG4_LIST + "let iA := aSave\nSetIdleMarkerAnimations rObj iA\n"
            "let aList := GetIdleMarkerAnimations rObj\n" + _JG4_LIST
            + 'SetFunctionValue (sv_Construct "%z n%g" sText (GetIdleMarkerTraitNumeric rObj 3))', True)
       for n, t in (("idles", "crouch"), ("notidle", "testqa"))]
    # ---- region weathers: lists, nth entries (past the list), membership
    + [_jg4arr(f"regn.weathers.{k}", k, "GetRegionWeathers rObj") for k in ("nv", "gs", "megaton", "urban", "gsmap", "tops")]
    + [_jg4v(f"regn.nth.{k}.{i}.{f}", k, f"GetNthRegionWeather{fn} rObj {i}", "form" if f != "chance" else False)
       for k, i in (("nv", 0), ("nv", 1), ("nv", 2), ("nv", -1), ("megaton", 0), ("megaton", 1), ("urban", 1), ("gsmap", 0))
       for f, fn in (("type", "Type"), ("chance", "Chance"), ("global", "Global"))
       if (k, i, f) != ("urban", 1, "global")]  # GetNthRegionWeatherGlobal on NVUrbanRegion 1, and on GSWeatherRegion 1 after AddRegionWeather, crashed the game [observed 2026-11-08]
    + [_jg4(f"regn.in.{k}.{w}", _form("rObj", JG4[k]) + _form("rOther", JG4[w]) + "SetFunctionValue (IsWeatherInRegion rObj rOther)")
       for k, w in (("nv", "clearnight"), ("nv", "gsweather"), ("gs", "gsweather"), ("gsmap", "gsweather"))]
    + [_jg4v(f"regn.override.{k}", k, "GetRegionWeatherOverride rObj") for k in ("nv", "gs", "megaton", "gsmap")]
    + [_jg4v(f"regn.priority.{k}", k, "GetRegionWeatherPriority rObj") for k in ("nv", "gs", "megaton", "urban", "gsmap")]
    + [_jg4set(f"regn.setoverride.gs.{v}", "gs", "GetRegionWeatherOverride rObj", f"SetRegionWeatherOverride rObj {v}",
               "SetRegionWeatherOverride rObj fSave") for v in (1, 2)]
    + [_jg4set("regn.setoverride.nv.0", "nv", "GetRegionWeatherOverride rObj", "SetRegionWeatherOverride rObj 0",
               "SetRegionWeatherOverride rObj fSave")]
    + [_jg4set(f"regn.setpriority.gs.{n}", "gs", "GetRegionWeatherPriority rObj", f"SetRegionWeatherPriority rObj {v}",
               "SetRegionWeatherPriority rObj fSave") for n, v in (("50", 50), ("300", 300), ("neg", -5), ("frac", 20.6))]
    # add (chance, global, a chance over 100, one already listed), remove, clear; each undone
    + [_jg4reg("regn.add.gs", "gs", "AddRegionWeather rObj rOther 40\n", "RemoveRegionWeather rObj rOther\n",
               'let sText := sv_Construct "%z c%g" sText (GetNthRegionWeatherChance rObj 1)\n'),
       _jg4reg("regn.add.gs.global", "gs", "AddRegionWeather rObj rOther 30 rThird\n", "RemoveRegionWeather rObj rOther\n",
               'let sText := sv_Construct "%z c%g" sText (GetNthRegionWeatherChance rObj 1)\n'),
       _jg4reg("regn.add.gs.150", "gs", "AddRegionWeather rObj rOther 150\n", "RemoveRegionWeather rObj rOther\n",
               'let sText := sv_Construct "%z c%g" sText (GetNthRegionWeatherChance rObj 1)\n'),
       # NVWastelandGS is listed already at 100: listed twice, or its chance changed? Undone by
       # clearing and adding it back at 100.
       _jg4reg("regn.add.gs.dup", "gs", _form("rOther", JG4["gsweather"]) + "AddRegionWeather rObj rOther 50\n",
               "ClearRegionWeathers rObj\nAddRegionWeather rObj rOther 100\n",
               'let sText := sv_Construct "%z c%g" sText (GetNthRegionWeatherChance rObj 0)\n'),
       _jg4reg("regn.remove.gs.notlisted", "gs", "RemoveRegionWeather rObj rOther\n", ""),
       # a region without a weather block (DC01, far from the player): gains one?
       _jg4reg("regn.add.dc01", "dc01", "AddRegionWeather rObj rOther 60\n", "ClearRegionWeathers rObj\n",
               'let sText := sv_Construct "%z o%g p%g" sText (GetRegionWeatherOverride rObj) (GetRegionWeatherPriority rObj)\n'),
       # WeatherMegaton (Capital Wasteland) cleared, then its two entries added back as in the record
       _jg4reg("regn.clear.megaton", "megaton", "ClearRegionWeathers rObj\n",
               _form("rOther", JG4["megw1"]) + _form("rThird", JG4["megg1"]) + "AddRegionWeather rObj rOther 0 rThird\n"
               + _form("rOther", JG4["megw2"]) + _form("rThird", JG4["megg2"]) + "AddRegionWeather rObj rOther 0 rThird\n",
               'let sText := sv_Construct "%z o%g p%g" sText (GetRegionWeatherOverride rObj) (GetRegionWeatherPriority rObj)\n')]
    # ---- race flags: GetRaceFlag reads, SetRaceFlag on TestQACaucasian only (0, 2, another flag, a value of 5)
    + [_jg4v(f"race.get.{k}.{f}", k, f"GetRaceFlag rObj {f}") for k in ("caucasian", "child", "ghoul", "testqa") for f in (0, 1, 2)]
    + [_jg4set(f"race.set.testqa.{f}.{v}", "testqa", f"GetRaceFlag rObj {f}", f"SetRaceFlag rObj {f} {v}", f"SetRaceFlag rObj {f} fSave")
       for f, v in ((0, 1), (2, 1), (1, 1), (3, 1), (0, 5))]
    # ---- faction members: rank filter, template-inherited factions, not a faction
    + [_jg4arr(f"fact.members.{k}", k, "GetFactionMembers rObj") for k in ("ringo", "militia", "boonerogue", "mercfac", "testqa")]
    + [_jg4arr(f"fact.members.militia.{r}", "militia", f"GetFactionMembers rObj {r}") for r in (0, 1, 2, -1)]
    + [_jg4("fact.members.ncr.size", _form("rObj", JG4["ncr"]) + "SetFunctionValue (ar_Size (GetFactionMembers rObj))")]
)


# sup1 + showoff3 probe sets (drafted 2026-11-08 for the observer session).
# Exec'd into jip_probes' namespace: uses _form and NF from there.
#
# sup1 (read-only; #1008): SUP batch 1 (docs/spec-notes/ext-sup1.md, nvse_sup1.rs):
# rounding, points and angles, numbers in words, the Cyrillic test, the actor finders and
# the topic getters. Forms from FalloutNV.esm [observed: fnvtool esm decode / inventory]:
# DIAL GREETING C8 (Topic), GOODBYE D4 (Conversation), Attack DC (Combat),
# PersuasionAttempt FA (Persuasion), NormalToAlert F4 (Detection), Repair 111 (Service),
# ObserveCombat 11A (Miscellaneous), RadioHello 126 (Radio), Sunny's topic 1055D2 (Topic,
# quest 104C66); INFO 104E7E (ten TCLT choices, first 104E60, eighth 105CBC), INFO 15CB5B
# (no choices); quests 104C66 VFreeformGoodsprings and 10A214 VCG02; the stimpak 15169 (not
# a topic); the runway piece 13DA0F (REFR of DLC03RunwayTransition in the start cell
# 0x000DEB16). The finders read whatever actors the airport save has loaded, so the rows
# print the found actor, whether it is the player, and its distance.
SUP1F = {"greeting": "C8", "goodbye": "D4", "attack": "DC", "persuade": "FA", "alert": "F4", "repair": "111",
         "observe": "11A", "radio": "126", "creatures": "1055D2", "greet": "104E7E", "bye": "15CB5B",
         "quest": "104C66", "vcg02": "10A214", "stimpak": "15169", "runway": "13DA0F", "choice0": "104E60",
         "choice7": "105CBC"}

_SUP1_DECL = ("ref rP\nref rObj\nref rOther\nref rA\nref rB\nfloat fA\nfloat fB\nfloat fC\nfloat fD\nfloat fE\n"
              "float fN\nfloat fM\nfloat fK\nfloat fX\nfloat fY\nfloat fZ\nstring_var sText\nstring_var sSave\n"
              "array_var aList\n")


def _sup1(pid, body, text=False):
    return (f"sup1.{pid}", _SUP1_DECL + "Begin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


def _sup1v(pid, expr, text=False):
    return _sup1(pid, f"SetFunctionValue ({expr})", text)


def _sup1f(pid, keys, expr, text=False):
    """rObj (and rOther) fetched from SUP1F, then the expression."""
    keys = keys if isinstance(keys, tuple) else (keys,)
    pre = "".join(_form(v, SUP1F[k]) for v, k in zip(("rObj", "rOther"), keys))
    return _sup1(pid, pre + f"SetFunctionValue ({expr})", text)


def _sup1_list(pid, keys, expr):
    """A form array as "size first last"."""
    keys = keys if isinstance(keys, tuple) else (keys,)
    pre = "".join(_form(v, SUP1F[k]) for v, k in zip(("rObj", "rOther"), keys))
    return _sup1(pid, pre + f"let aList := {expr}\nlet fN := ar_Size aList\nlet sText := sv_Construct \"%g\" fN\nif fN > 0\n"
                 "let rA := aList[0]\nlet sText := sv_Construct \"%z %z\" sText (GetFormIDString rA)\n"
                 "let rA := aList[fN - 1]\nlet sText := sv_Construct \"%z %z\" sText (GetFormIDString rA)\nendif\n"
                 "SetFunctionValue sText", True)


_SUP1_HERE = "let fX := rP.GetPos X\nlet fY := rP.GetPos Y\nlet fZ := rP.GetPos Z\n"


def _sup1_found(var="rA"):
    """The found actor: its FormID, 1 when it is the player, its distance from the player."""
    return (f"if {var}\nlet sText := sv_Construct \"%z %g %.0f\" (GetFormIDString {var}) ({var} == rP) (rP.GetDistance {var})\n"
            "else\nlet sText := sv_Construct \"none\"\nendif\nSetFunctionValue sText")


def _sup1_random(pid, call, pre=""):
    """Eight draws: "draws none player maxDistance" (the actors differ run to run)."""
    return _sup1(pid, pre + "let fN := 0\nlet fA := 0\nlet fB := 0\nlet fM := 0\nwhile fN < 8\n"
                 f"let rB := {call}\nif rB\nif rB == rP\nlet fB += 1\nendif\nlet fK := rP.GetDistance rB\n"
                 "if fK > fM\nlet fM := fK\nendif\nelse\nlet fA += 1\nendif\nlet fN += 1\nloop\n"
                 "SetFunctionValue (sv_Construct \"%g %g %g %.0f\" fN fA fB fM)", True)


_SUP1_WORDS = (10, 332323, 10020, 0, -15, 100, 115, 90, 1001, 1000000, 1000000000, 2147483647, 7.9, -7.9)

SUP1_PROBES = (
    # Round [documented: 37.4 -> 37, 37.8 -> 38; hypothesis: halves away from zero].
    [_sup1v("round.page", 'sv_Construct "%g %g" (Round 37.4) (Round 37.8)', True),
     _sup1v("round.halves", 'sv_Construct "%g %g %g %g %g %g" (Round 0.5) (Round 1.5) (Round 2.5) (Round -0.5) (Round -1.5) (Round -2.5)', True),
     _sup1v("round.other", 'sv_Construct "%.1f %g %g" (Round 123456.7) (Round -0.4) (Round -37.6)', True)]
    # RoundAlt [documented: the page's six examples; hypothesis: places outside 0-4
    # clamped, negative numbers cut toward zero, halves away from zero, any nonzero round].
    + [_sup1v("roundalt.page", 'sv_Construct "%.6f %.6f %.6f %.6f %.6f %.6f" (RoundAlt 37.55 0 0) (RoundAlt 37.55 1 0) '
              "(RoundAlt 37.6 1 0) (RoundAlt 37.66 1 1) (RoundAlt 37.665454 3 1) (RoundAlt 37.665654 3 1)", True),
       _sup1v("roundalt.places", 'sv_Construct "%.8f %.8f %.8f %.8f" (RoundAlt 37.123456789 4 1) (RoundAlt 37.123456789 5 1) '
              "(RoundAlt 37.123456789 9 0) (RoundAlt 37.55 -1 1)", True),
       _sup1v("roundalt.negative", 'sv_Construct "%.6f %.6f %.6f" (RoundAlt -37.55 1 0) (RoundAlt -37.55 1 1) (RoundAlt -37.66 0 1)', True),
       _sup1v("roundalt.halves", 'sv_Construct "%.6f %.6f %.6f %.6f" (RoundAlt 0.25 1 1) (RoundAlt 2.5 0 1) (RoundAlt 0.125 2 1) (RoundAlt -2.5 0 1)', True),
       _sup1v("roundalt.flag2", 'sv_Construct "%.6f %.6f" (RoundAlt 37.66 1 2) (RoundAlt 37.66 2.7 1)', True)]
    # IsNumberNAN [documented: 1/0 for a number; no argument makes a NaN].
    + [_sup1v("nan.five", "IsNumberNAN 5"),
       _sup1("nan.make", "let fA := IsNumberNAN\nSetFunctionValue (sv_Construct \"%g %g %g\" (IsNumberNAN fA) (fA == fA) (IsNumberNAN 0))", True),
       _sup1("nan.print", "let fA := IsNumberNAN\nSetFunctionValue (sv_Construct \"%g\" fA)", True)]
    # GetDistanceBetweenPoints [documented].
    + [_sup1v("distance", 'sv_Construct "%.4f %.4f %.4f" (GetDistanceBetweenPoints 0 0 0 3 4 12) '
              "(GetDistanceBetweenPoints -1 -2 -3 2 2 9) (GetDistanceBetweenPoints 5 5 5 5 5 5)", True)]
    # GetHeadingAngleBetweenPoints [documented: as GetHeadingAngle; hypothesis: degrees,
    # clockwise, due behind is 180 not -180, facings past a turn wrap, same point 0].
    + [
       # Against the game's own GetHeadingAngle, from the player to the runway piece, as
       # differences so the player's place and facing drop out (rewritten 2026-11-08).
       _sup1("heading.vsgame", _form("rObj", SUP1F["runway"]) + _SUP1_HERE + "let fA := rP.GetAngle Z\n"
             "let fB := rP.GetHeadingAngle rObj\n"
             'SetFunctionValue (sv_Construct "%.3f %.3f" ((GetHeadingAngleAlt fX fY fA rObj) - fB) '
             "((GetHeadingAngleBetweenPoints fX fY fA (rObj.GetPos X) (rObj.GetPos Y)) - fB))", True),
       _sup1("heading.alt.turned", _form("rObj", SUP1F["runway"]) + _SUP1_HERE + "let fA := rObj.GetPos X\nlet fB := rObj.GetPos Y\n"
             'SetFunctionValue (sv_Construct "%.3f %.3f" ((GetHeadingAngleAlt fX fY 0 rObj) - (GetHeadingAngleBetweenPoints fX fY 0 fA fB)) '
             "((GetHeadingAngleAlt fX fY 180 rObj) - (GetHeadingAngleBetweenPoints fX fY 180 fA fB)))", True)]
    # NumberToWords [documented: 10, 332323, 10020; hypothesis: "zero", "minus", a
    # fraction cut, words past a billion].
    + [_sup1v(f"words.{str(n).replace('-', 'm').replace('.', 'p')}", f"NumberToWords {n}", True) for n in _SUP1_WORDS]
    # StringContainsCyrillic [documented; hypothesis: the Unicode block U+0400-04FF].
    # How the step's UTF-8 text reaches the game's compiler is not known, so the
    # literals are paired with characters built by code (%a): 207 and 233 are Cyrillic
    # letters in Windows-1251 but Latin letters in Unicode; 1055 is U+041F.
    + [_sup1v("cyr.latin", 'StringContainsCyrillic "abc XYZ"'),
       _sup1v("cyr.utf8", 'StringContainsCyrillic "Привет"'),
       _sup1v("cyr.accent", 'StringContainsCyrillic "café"')]
    + [_sup1(f"cyr.code.{c}", f'let sText := sv_Construct "x%ax" {c}\nSetFunctionValue (StringContainsCyrillic sText)')
       for c in (207, 233, 192, 1055, 65)]
    # GetSUPVersion [documented: always -1]; IsPlayerOverencumbered [documented: as
    # GetIsPlayerOverencumbered].
    + [_sup1v("version", "GetSUPVersion"),
       _sup1v("overencumbered", 'sv_Construct "%g %g" (IsPlayerOverencumbered) (GetIsPlayerOverencumbered)', True)]
    # The actor finders [documented: loaded actors, never the player; hypothesis: a
    # distance of 0 means none, the calling reference is not its own find].
    + [_sup1("closest.here", _SUP1_HERE + "let rA := FindClosestActor fX fY fZ\n" + _sup1_found(), True),
       _sup1("closest.here.zeros", _SUP1_HERE + "let rA := FindClosestActor fX fY fZ\nlet rB := FindClosestActor fX fY fZ 0 0 0\n"
             "SetFunctionValue (rA == rB)"),
       _sup1("closest.dist1", _SUP1_HERE + "let rA := FindClosestActor fX fY fZ 0 0 1\n" + _sup1_found(), True),
       _sup1("closest.far.limited", _SUP1_HERE + "let rA := FindClosestActor fX fY (fZ + 1000000) 0 0 100\n" + _sup1_found(), True),
       _sup1("closest.far.unlimited", _SUP1_HERE + "let rA := FindClosestActor fX fY (fZ + 1000000)\n" + _sup1_found(), True),
       _sup1("closest.dead", _SUP1_HERE + "let rA := FindClosestActor fX fY fZ 1\n" + _sup1_found(), True),
       _sup1("closest.teammates", _SUP1_HERE + "let rA := FindClosestActor fX fY fZ 0 1\n" + _sup1_found(), True),
       _sup1("fromref.player", _SUP1_HERE + "let rA := rP.FindClosestActorFromRef\nlet rB := FindClosestActor fX fY fZ\n"
             "if rA\nSetFunctionValue (sv_Construct \"%z %g %g\" (GetFormIDString rA) (rA == rP) (rA == rB))\nelse\n"
             "SetFunctionValue (sv_Construct \"none %g\" (rB == 0))\nendif", True),
       _sup1("fromref.self", "let rA := rP.FindClosestActorFromRef\nif rA\nlet rB := rA.FindClosestActorFromRef\n"
             "SetFunctionValue (sv_Construct \"%z %g %g\" (GetFormIDString rB) (rB == rA) (rB == rP))\nelse\n"
             "SetFunctionValue (sv_Construct \"none\")\nendif", True),
       _sup1("fromref.dist1", "let rA := rP.FindClosestActorFromRef 0 0 1\n" + _sup1_found(), True),
       _sup1("fromref.dead.far", "let rA := rP.FindClosestActorFromRef 1 0 100000\n" + _sup1_found(), True),
       _sup1("fromref.static", _form("rObj", SUP1F["runway"]) + "let rA := rObj.FindClosestActorFromRef\n" + _sup1_found(), True),
       _sup1_random("random.here", "FindRandomActor fX fY fZ 1 0 20000", _SUP1_HERE),
       _sup1_random("random.here.dist1", "FindRandomActor fX fY fZ 1 1 1", _SUP1_HERE),
       _sup1_random("random.fromref", "rP.FindRandomActorFromRef 1 0 20000"),
       _sup1("random.fromref.self", "let rA := rP.FindClosestActorFromRef 1 1\nif rA == 0\nSetFunctionValue (sv_Construct \"none\")\nreturn\nendif\n"
             "let fN := 0\nlet fA := 0\nwhile fN < 8\nlet rB := rA.FindRandomActorFromRef 1 1\nif rB == rA\nlet fA += 1\nendif\n"
             "let fN += 1\nloop\nSetFunctionValue (sv_Construct \"%g %g\" fN fA)", True)]
    # TopicGetType [documented: the page's table, 0 Topic ... 7 Radio; hypothesis: 0 for a
    # form that is not a topic].
    + [_sup1f(f"type.{k}", k, "TopicGetType rObj")
       for k in ("greeting", "goodbye", "attack", "persuade", "alert", "repair", "observe", "radio", "creatures", "greet", "stimpak")]
    # TopicGetAllTopicInfos [documented: all, or the quest's only; hypothesis: topic order,
    # a form that is not a quest filters everything out].
    + [_sup1_list("infos.creatures", "creatures", "TopicGetAllTopicInfos rObj"),
       _sup1_list("infos.creatures.quest", ("creatures", "quest"), "TopicGetAllTopicInfos rObj rOther"),
       _sup1_list("infos.creatures.otherquest", ("creatures", "vcg02"), "TopicGetAllTopicInfos rObj rOther"),
       _sup1_list("infos.creatures.notquest", ("creatures", "stimpak"), "TopicGetAllTopicInfos rObj rOther"),
       _sup1_list("infos.greeting.quest", ("greeting", "quest"), "TopicGetAllTopicInfos rObj rOther"),
       _sup1_list("infos.goodbye.quest", ("goodbye", "quest"), "TopicGetAllTopicInfos rObj rOther"),
       _sup1f("infos.greeting.size", "greeting", "ar_Size (TopicGetAllTopicInfos rObj)"),
       _sup1_list("infos.notopic", "stimpak", "TopicGetAllTopicInfos rObj")]
    # A line's choices [documented: TCLT, TopicInfoGetAllChoices, …ChoiceCount, …HasChoice].
    + [_sup1_list("choices.greet", "greet", "TopicInfoGetAllChoices rObj"),
       _sup1_list("choices.bye", "bye", "TopicInfoGetAllChoices rObj"),
       _sup1_list("choices.notinfo", "stimpak", "TopicInfoGetAllChoices rObj"),
       _sup1("choices.greet.order", _form("rObj", SUP1F["greet"]) + "let aList := TopicInfoGetAllChoices rObj\nlet rA := aList[7]\n"
             "SetFunctionValue (GetFormIDString rA)", True),
       _sup1f("count.greet", "greet", "TopicInfoGetChoiceCount rObj"),
       _sup1f("count.bye", "bye", "TopicInfoGetChoiceCount rObj"),
       _sup1f("count.notinfo", "stimpak", "TopicInfoGetChoiceCount rObj"),
       _sup1f("has.first", ("greet", "choice0"), "TopicInfoHasChoice rObj rOther"),
       _sup1f("has.eighth", ("greet", "choice7"), "TopicInfoHasChoice rObj rOther"),
       _sup1f("has.not", ("greet", "creatures"), "TopicInfoHasChoice rObj rOther"),
       _sup1f("has.bye", ("bye", "choice0"), "TopicInfoHasChoice rObj rOther")]
    # GetHeadingAngleBetweenPoints, one call a row and last: the eight in one row crashed the
    # game [observed 2026-11-08, sup1.heading.points].
    + [_sup1v(f"heading.points.{n}", f"GetHeadingAngleBetweenPoints {a}")
       for n, a in (("e", "0 0 0 10 0"), ("s", "0 0 0 0 -10"), ("n.90", "0 0 90 0 10"), ("n.765", "0 0 765 0 10"),
                    ("n.m270", "0 0 -270 0 10"), ("nw", "0 0 0 -10 10"), ("rad", "0 0 1.5708 10 0"), ("same", "5 5 0 5 5"))]
)


# showoff3 (writes, session only, each change put back; #1010): ShowOff batch 3
# (docs/spec-notes/ext-showoff.md §6, nvse_showoff3.rs) on the player at the airport save,
# VBooneFaction B160C and VMojaveMercenary 173EEF (no XNAM relation either way [observed:
# fnvtool esm decode]), AmmoList127mm 1613D5 (two ammo forms), the stimpak, the runway
# piece 13DA0F, challenges NVChallengeExplore 15EB33 (threshold 50), NVChallengeSimpak
# 15EB3E (recurring), NVChallengeChallenge 15EB36 (recurring), NVChallengeVault3 15EB3C
# (map marker), NVChallengeOldTyme 1690E8 (damage), INFO E8911 (three responses) and
# 15CB5B (one). The equipped rows read whatever the save wears.
#
# Changes, each put back: the player's weapon unequipped and re-equipped (UnequipItems),
# the murderer flag, the sleep/wait override, the two factions' temporary reactions (left
# neutral, as the records have them), the player's Radiation ExtraData (back to its old
# value, the extra data may stay), fDefaultFOV:Display (+5 and back), the 10mm-list (the
# weapon added and removed), the seeded randomizer of the calling mod (left seeded; it has
# no reset), temporary (*) aux timers and aux variables (stopped and erased), aux-timer
# start/update handlers (removed in the last row, and their timers stopped, so a handler
# whose removal fails never fires again). Steps run one per frame, in order.
SO3F = {"boonefac": "B160C", "mercfac": "173EEF", "ammolist": "1613D5", "stimpak": "15169", "runway": "13DA0F",
        "caps": "F", "explore": "15EB33", "simpak": "15EB3E", "challenge": "15EB36", "vault3": "15EB3C",
        "oldtyme": "1690E8", "caravan": "E8911", "bye": "15CB5B"}

_SO3_DECL = ("ref rP\nref rObj\nref rOther\nref rItem\nref rH\nref rH2\nfloat fRes\nfloat fSave\nfloat fA\nfloat fB\n"
             "float fC\nfloat fD\nfloat fE\nfloat fF\nfloat fG\nfloat fH\nfloat fI\nfloat fJ\nfloat fN\nfloat fK\n"
             "string_var sText\nstring_var sSave\narray_var aList\narray_var aKeys\n")


def _so3(pid, body, text=False):
    return (f"so3.{pid}", _SO3_DECL + "Begin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


def _so3v(pid, expr, text=False):
    return _so3(pid, f"SetFunctionValue ({expr})", text)


def _so3f(pid, key, expr, text=False):
    return _so3(pid, _form("rObj", SO3F[key]) + f"SetFunctionValue ({expr})", text)


def _so3_items(pid, expr, refs=False):
    """A form array as "size: id id ...", inventory references by their base."""
    each = "let rItem := aList[fC]\n" + ("let rOther := rItem.GetBaseObject\n" if refs else "let rOther := rItem\n")
    return _so3(pid, f"let aList := {expr}\nlet aKeys := ar_Keys aList\nlet fA := ar_Size aKeys\n"
                'let sText := sv_Construct "%g:" fA\nlet fB := 0\nwhile fB < fA\nlet fC := aKeys[fB]\n' + each
                + 'let sText := sv_Construct "%z %z" sText (GetFormIDString rOther)\nlet fB += 1\nloop\nSetFunctionValue sText', True)


def _so3_draws(n, call="RandSeeded 1 100", first="fA"):
    """`n` draws into fA, fB, ... and their text."""
    vs = [chr(ord(first[1]) + i) for i in range(n)]
    return "".join(f"let f{v} := {call}\n" for v in vs), " ".join("%g" for _ in vs), " ".join(f"f{v}" for v in vs)


def _so3_seq(pid, setup, n=5, call="RandSeeded 1 100", tail=""):
    lets, fmt, args = _so3_draws(n, call)
    return _so3(pid, setup + lets + tail + f'SetFunctionValue (sv_Construct "{fmt} | %g" {args} (GetRandomizerSeed))', True)


_SO3_EASE = range(0, 10)

# Aux-timer handler lambdas: each counts its calls and keeps what it was given in its
# owner's temporary aux variables (the owner is a parameter, so nothing is captured).
_SO3_START = ("(Begin Function {string_var sN, ref rOwn}\n"
              'rOwn.AuxVarSetFlt "*nvfSO3S" ((rOwn.AuxVarGetFlt "*nvfSO3S") + 1)\n'
              'rOwn.AuxVarSetFlt "*nvfSO3SL" (sv_Length sN)\nEnd)')
_SO3_UPD = ("(Begin Function {string_var sN, ref rOwn, float fDelta}\n"
            'rOwn.AuxVarSetFlt "*nvfSO3{c}" ((rOwn.AuxVarGetFlt "*nvfSO3{c}") + 1)\n'
            'rOwn.AuxVarSetFlt "*nvfSO3{c}D" fDelta\nEnd)')


def _so3_count(*names, owner="rP"):
    return " ".join(f'({owner}.AuxVarGetFlt "*nvfSO3{n}")' for n in names)


SHOWOFF3_PROBES = (
    # ---- the seeded randomizer, first: nothing may draw before these rows ----
    # [documented: -1 with no seed; AdvanceSeed 0 with no seed; the default seed 5489;
    # hypothesis: MT19937 and a draw mapped by its remainder, so the values need not match].
    [_so3v("seed.get.none", "GetRandomizerSeed"),
     _so3v("seed.advance.none", "AdvanceSeed 3"),
     _so3_seq("seed.default", ""),
     _so3v("seed.get.afterdefault", "GetRandomizerSeed"),
     _so3v("seed.advance.afterdefault", "AdvanceSeed 0"),
     # The same seed repeats its sequence: twice in one frame, again in the next frame.
     _so3("seed.20.twice", "SetRandomizerSeed 20\n" + _so3_draws(5)[0] + "SetRandomizerSeed 20\n"
          + _so3_draws(5, first="fF")[0] + 'SetFunctionValue (sv_Construct "%g %g %g %g %g | %g %g %g %g %g | %g" '
          "fA fB fC fD fE fF fG fH fI fJ (GetRandomizerSeed))", True),
     _so3_seq("seed.20.nextframe", "SetRandomizerSeed 20\n"),
     _so3_seq("seed.0", "SetRandomizerSeed 0\n"),
     _so3_seq("seed.noarg", "SetRandomizerSeed\n"),
     _so3_seq("seed.5489", "SetRandomizerSeed 5489\n"),
     _so3_seq("seed.alias", "SetSeed 20\n", 2, "GenerateSeededRandomNumber 1 100"),
     _so3_seq("seed.percent", "SetRandomizerSeed 20\n", 5, "GetRandomPercentSeeded"),
     # AdvanceSeed 5 then one draw is the sixth of the sequence [documented: the page].
     _so3("seed.advance", "SetRandomizerSeed 101\nlet fRes := AdvanceSeed 5\nlet fA := RandSeeded 1 100\n"
          "SetRandomizerSeed 101\n" + _so3_draws(6, first="fB")[0]
          + 'SetFunctionValue (sv_Construct "%g %g | %g %g %g %g %g %g" fRes fA fB fC fD fE fF fG)', True),
     _so3("seed.advance.zero", "SetRandomizerSeed 101\nlet fRes := AdvanceSeed 0\nlet fA := RandSeeded 1 100\n"
          'SetFunctionValue (sv_Construct "%g %g" fRes fA)', True),
     # How a draw maps onto the range: a wide range shows the raw numbers (MT19937's
     # first outputs for 5489 are 3499211612, 581869302, 3890346734).
     _so3_seq("seed.range.wide", "SetRandomizerSeed 5489\n", 3, "RandSeeded 0 1000000000"),
     _so3_seq("seed.range.uint", "SetRandomizerSeed 5489\n", 3, "RandSeeded 0 4294967294"),
     _so3_seq("seed.range.bits", "SetRandomizerSeed 5489\n", 8, "RandSeeded 0 1"),
     _so3_seq("seed.range.same", "SetRandomizerSeed 5489\n", 3, "RandSeeded 7 7"),
     _so3_seq("seed.range.fraction", "SetRandomizerSeed 5489\n", 5, "RandSeeded 1.7 3.2"),
     # Seeds from forms [documented: the form's FormID, the default with none; the page's
     # example calls it on a reference; hypothesis: the calling reference's FormID].
     _so3("seed.form.caps", _form("rObj", SO3F["caps"]) + "SetSeedUsingForm rObj\nlet fA := GetRandomizerSeed\n"
          "let fB := RandSeeded 1 100\nSetRandomizerSeed 15\nlet fC := RandSeeded 1 100\n"
          'SetFunctionValue (sv_Construct "%g %g %g" fA fB fC)', True),
     _so3("seed.form.player", "SetSeedUsingForm rP\nSetFunctionValue (GetRandomizerSeed)"),
     _so3("seed.form.ref", "rP.SetSeedUsingForm\nSetFunctionValue (GetRandomizerSeed)"),
     _so3("seed.form.none", "SetRandomizerSeed 20\nSetSeedUsingForm\nSetFunctionValue (GetRandomizerSeed)"),
     _so3("seed.form.alias", _form("rObj", SO3F["stimpak"]) + "SetFormSeed rObj\nSetFunctionValue (GetSeed)"),
     _so3("seed.get.ref", "SetRandomizerSeed 33\nSetFunctionValue (rP.GetRandomizerSeed)"),
     # A seed above the int range [documented: an unsigned seed, returned as an int].
     _so3("seed.big", "SetRandomizerSeed 4294967295\nlet fA := GetRandomizerSeed\nlet fB := RandSeeded 1 100\n"
          "SetRandomizerSeed 3000000000\n" 'SetFunctionValue (sv_Construct "%.0f %g %.0f" fA fB (GetRandomizerSeed))', True)]
    # (RandSeeded with min above max is not probed: its page says the check is not in
    # place and the game crashes.)
    # ---- equipped items ----
    # [documented: playable items with no mask; the slots' items with slot bits; no
    # duplicates; hypothesis: bits 20 and up ignored, ammo not counted].
    + [_so3_items("items.nomask", "rP.GetEquippedItems"),
       _so3_items("items.0", "rP.GetEquippedItems 0"),
       _so3_items("items.weapon", "rP.GetEquippedItems 32"),
       _so3_items("items.pipboy", "rP.GetEquippedItems 64"),
       _so3_items("items.allslots", "rP.GetEquippedItems 1048575"),
       _so3_items("items.bit20", "rP.GetEquippedItems 1048576"),
       _so3_items("items.bit20.weapon", "rP.GetEquippedItems 1048608"),
       _so3_items("refs.nomask", "rP.GetEquippedItemRefs", refs=True),
       _so3_items("refs.allslots", "rP.GetEquippedItemRefs 1048575", refs=True),
       _so3v("bitmask", "rP.GetEquippedItemsAsBitMask"),
       _so3v("bitmask.alias", "rP.GetOccupiedEquipSlots"),
       _so3v("broken", 'sv_Construct "%g %g %g %g %g %g" (rP.GetNumBrokenEquippedItems) (rP.GetNumBrokenEquippedItems 100) '
             "(rP.GetNumBrokenEquippedItems 100 32) (rP.GetNumBrokenEquippedItems 0 32) (rP.GetNumBrokenEquippedItems -1) "
             "(rP.GetNumBrokenEq 100 1048575)", True),
       _so3v("weight.base", 'sv_Construct "%.3f %.3f %.3f %.3f %.3f" (rP.GetBaseEquippedWeight) (rP.GetBaseEquippedWeight 0 32) '
             "(rP.GetBaseEquippedWeight 0 1048575) (rP.GetBaseEquippedWeight 1000) (rP.GetBaseEquippedWeight 2)", True),
       _so3v("weight.calc", 'sv_Construct "%.3f %.3f %.3f %.3f %.3f" (rP.GetCalculatedEquippedWeight) (rP.GetCalculatedEquippedWeight 0 32) '
             "(rP.GetCalculatedEquippedWeight 0 1048575) (rP.GetCalculatedEquippedWeight 1000) (rP.GetCalculatedEquippedWeight 2)", True),
       _so3v("carry", 'sv_Construct "%.3f %.3f" (rP.GetCalculatedMaxCarryWeight) (rP.GetAV CarryWeight)', True),
       # The weapon as an inventory reference [documented: 0 with none].
       _so3("weaponref", "let rOther := rP.GetEquippedObject 5\nlet rItem := rP.GetEquippedWeaponRef\nif rItem\n"
            "let rObj := rItem.GetBaseObject\nSetFunctionValue (sv_Construct \"%z %g\" (GetFormIDString rObj) (rObj == rOther))\n"
            "else\nSetFunctionValue (sv_Construct \"0 %g\" (rOther == 0))\nendif", True),
       _so3("refforitem.weapon", "let rOther := rP.GetEquippedObject 5\nif rOther == 0\nSetFunctionValue (sv_Construct \"no weapon\")\n"
            "return\nendif\nlet rItem := rP.GetEquippedItemRefForItem rOther\nif rItem\nlet rObj := rItem.GetBaseObject\n"
            "SetFunctionValue (sv_Construct \"%z %g %g\" (GetFormIDString rObj) (rObj == rOther) (rItem.IsEquipped))\nelse\n"
            "SetFunctionValue (sv_Construct \"0\")\nendif", True),
       _so3("refforitem.notworn", _form("rObj", SO3F["stimpak"]) + "let rItem := rP.GetEquippedItemRefForItem rObj\n"
            "SetFunctionValue (rItem == 0)"),
       # A form list: the ammo list with the worn weapon added at its end, then taken out.
       _so3("refforitem.list", _form("rObj", SO3F["ammolist"]) + "let rOther := rP.GetEquippedObject 5\nif rOther == 0\n"
            "SetFunctionValue (sv_Construct \"no weapon\")\nreturn\nendif\nlet fA := ListGetCount rObj\n"
            "ListAddForm rObj rOther\nlet rItem := rP.GetEquippedItemRefForItem rObj\nlet fB := 0\nif rItem\n"
            "let fB := (rItem.GetBaseObject) == rOther\nendif\nListRemoveForm rObj rOther\n"
            'SetFunctionValue (sv_Construct "%g %g %g" fA fB (ListGetCount rObj))', True),
       _so3("refforitem.list.none", _form("rObj", SO3F["ammolist"]) + "let rItem := rP.GetEquippedItemRefForItem rObj\n"
            "SetFunctionValue (rItem == 0)"),
       # UnequipItems on the weapon slot [documented: as UnequipItem], then re-equipped.
       _so3("unequip.weapon", "let rOther := rP.GetEquippedObject 5\nif rOther == 0\nSetFunctionValue (sv_Construct \"no weapon\")\n"
            "return\nendif\nrP.AuxVarSetRef \"*nvfSO3W\" rOther\nlet fA := rP.GetEquippedItemsAsBitMask\n"
            "rP.UnequipItems 32 0 1\nlet fB := rP.GetEquippedItemsAsBitMask\nlet fC := rP.GetEquipped rOther\n"
            'SetFunctionValue (sv_Construct "%g %g %g" fA fB fC)', True),
       _so3("unequip.weapon.next", "let rOther := rP.AuxVarGetRef \"*nvfSO3W\"\n"
            'SetFunctionValue (sv_Construct "%g %g" (rP.GetEquippedItemsAsBitMask) (rOther != 0))', True),
       _so3("unequip.reequip", "let rOther := rP.AuxVarGetRef \"*nvfSO3W\"\nif rOther\nrP.EquipItem rOther 0 1\nendif\n"
            "SetFunctionValue 1"),
       _so3("unequip.reequip.next", "let rOther := rP.AuxVarGetRef \"*nvfSO3W\"\nrP.AuxVarErase \"*nvfSO3W\"\n"
            'SetFunctionValue (sv_Construct "%g %z" (rP.GetEquippedItemsAsBitMask) (GetFormIDString (rP.GetEquippedObject 5)))', True)]
    # ---- small getters ----
    # ApplyEasingAlt [documented: codes 0 sine … 9 bounce, modes 0/1/2, -1 for a bad code;
    # hypothesis: a bad mode, an input outside 0-1].
    + [_so3v(f"ease.{c}", f'sv_Construct "%.6f %.6f %.6f" (ApplyEasingAlt 0.25 {c} 0) (ApplyEasingAlt 0.25 {c} 1) (ApplyEasingAlt 0.25 {c} 2)', True)
       for c in _SO3_EASE]
    + [_so3v("ease.badcode", 'sv_Construct "%g %g %g" (ApplyEasingAlt 0.5 10 0) (ApplyEasingAlt 0.5 -1 0) (ApplyEasingAlt 0.5 1.5 0)', True),
       _so3v("ease.badmode", 'sv_Construct "%.6f %.6f" (ApplyEasingAlt 0.25 1 3) (ApplyEasingAlt 0.25 1 -1)', True),
       _so3v("ease.outside", 'sv_Construct "%.6f %.6f %.6f" (ApplyEasingAlt 1.5 1 0) (ApplyEasingAlt -0.5 1 0) (ApplyEasingAlt 1 9 2)', True),
       _so3v("ease.alias", 'sv_Construct "%.6f" (EaseAlt 0.25 2 1)', True),
       # GetPosArray [documented: X, Y, Z]: printed less GetPos.
       _so3("posarray", "let aList := rP.GetPosArray\n"
            'SetFunctionValue (sv_Construct "%g %.3f %.3f %.3f" (ar_Size aList) ((aList[0]) - (rP.GetPos X)) '
            "((aList[1]) - (rP.GetPos Y)) ((aList[2]) - (rP.GetPos Z)))", True),
       _so3v("mainthread", "IsOutsideMainThread"),
       _so3v("conditionprint", "rP.ConditionPrint"),
       _so3v("animcond", 'sv_Construct "%g %g %g %g %g" (rP.IsAnimPlayingExCond 1) (rP.IsAnimPlayingExCond 1 1) '
             "(rP.IsAnimPlayingExCond 2) (rP.IsAnimPlayingExCond 3) (rP.IsAnimPlayingEx 1)", True),
       _so3v("jumping", 'sv_Construct "%g %g" (rP.IsJumping) (rP.IsInAir)', True),
       # GetNumActorsInRangeFromRef [documented: flags 1, 2, 4; hypothesis: the caller not
       # counted (distance 0 from itself), the player counted from another reference].
       _so3v("inrange.player", 'sv_Construct "%g %g %g %g" (rP.GetNumActorsInRangeFromRef 0) (rP.GetNumActorsInRangeFromRef 500) '
             "(rP.GetNumActorsInRangeFromRef 5000) (rP.GetNumActorsInRangeFromRef 100000)", True),
       _so3v("inrange.flags", 'sv_Construct "%g %g %g %g %g" (rP.GetNumActorsInRangeFromRef 100000 0) (rP.GetNumActorsInRangeFromRef 100000 1) '
             "(rP.GetNumActorsInRangeFromRef 100000 2) (rP.GetNumActorsInRangeFromRef 100000 4) (rP.GetNumActorsInRangeFromRef 100000 7)", True),
       _so3("inrange.static", _form("rObj", SO3F["runway"])
            + 'SetFunctionValue (sv_Construct "%g %g %.0f" (rObj.GetNumActorsInRangeFromRef 0) (rObj.GetNumActorsInRangeFromRef 100000) (rObj.GetDistance rP))', True)]
    # ---- the player ----
    # SetIsPCAMurderer [documented: the flag IsPCAMurderer reads].
    + [_so3("murderer", "let fSave := IsPCAMurderer\nSetIsPCAMurderer 1\nlet fA := IsPCAMurderer\nSetIsPCAMurderer 0\n"
            "let fB := IsPCAMurderer\nSetIsPCAMurderer fSave\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fA fB (IsPCAMurderer))', True),
       # Get/SetPCHasSleepWaitOverride [documented: EnableFastTravel's can-wait flag].
       _so3("sleepwait", "let fSave := GetPCHasSleepWaitOverride\nSetPCHasSleepWaitOverride 1\nlet fA := GetPCHasSleepWaitOverride\n"
            "SetPCHasSleepWaitOverride 0\nlet fB := GetPCHasSleepWaitOverride\nSetPCHasSleepWaitOverride fSave\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fA fB (GetPCHasSleepWaitOverride))', True)]
    # GetChallengeProgress [documented: done non-recurring = threshold, recurring from 0;
    # hypothesis: 0 for a form that is not a challenge].
    + [_so3f(f"challenge.{k}", k, "GetChallengeProgress rObj") for k in ("explore", "simpak", "challenge", "vault3", "oldtyme", "stimpak")]
    # Level-up points [documented: the next level outside the menu, levelOverride before the
    # +1; perk points 1 or 0 by iLevelsPerPerk].
    + [_so3v("points.skill", 'sv_Construct "%g %g %g %g %g %g" (GetCalculatedSkillPoints) (GetCalculatedSkillPoints 1) '
             "(GetCalculatedSkillPoints 2) (GetCalculatedSkillPoints 10) (GetCalculatedSkillPoints 49) (GetCalculatedSkillPointsEarnedPerLevel)", True),
       _so3v("points.perk", 'sv_Construct "%g %g %g %g %g %g" (GetCalculatedPerkPoints) (GetCalculatedPerkPoints 1) '
             "(GetCalculatedPerkPoints 2) (GetCalculatedPerkPoints 3) (GetCalculatedPerkPoints 49) (GetCalculatedPerkPointsEarnedPerLevel)", True),
       _so3v("points.level", "rP.GetLevel")]
    # ---- factions: temporary reactions, both directions, left neutral as the records have
    # them [documented: SetAllyTemp 0 ally 1 friend; SetEnemyTemp 0 enemy 1 neutral].
    + [_so3("factions.temp", _form("rObj", SO3F["boonefac"]) + _form("rOther", SO3F["mercfac"])
            + "let fA := GetFactionCombatReaction rObj rOther\nlet fB := GetFactionCombatReaction rOther rObj\n"
            "SetAllyTemp rObj rOther\nlet fC := GetFactionCombatReaction rObj rOther\nlet fD := GetFactionCombatReaction rOther rObj\n"
            "SetAllyTemp rObj rOther 0 1\nlet fE := GetFactionCombatReaction rObj rOther\nlet fF := GetFactionCombatReaction rOther rObj\n"
            "SetEnemyTemp rObj rOther\nlet fG := GetFactionCombatReaction rObj rOther\nlet fH := GetFactionCombatReaction rOther rObj\n"
            "SetEnemyTemp rObj rOther 1 0\nlet fI := GetFactionCombatReaction rObj rOther\nlet fJ := GetFactionCombatReaction rOther rObj\n"
            "SetEnemyTemp rObj rOther 1 1\n"
            'SetFunctionValue (sv_Construct "%g %g | %g %g | %g %g | %g %g | %g %g | %g %g" fA fB fC fD fE fF fG fH fI fJ '
            "(GetFactionCombatReaction rObj rOther) (GetFactionCombatReaction rOther rObj))", True),
       _so3("factions.alias", _form("rObj", SO3F["boonefac"]) + _form("rOther", SO3F["mercfac"])
            + "SetAllyShowOff rObj rOther\nlet fA := GetFactionCombatReaction rObj rOther\nSetEnemyShowOff rObj rOther\n"
            "let fB := GetFactionCombatReaction rObj rOther\nSetEnemyTemp rObj rOther 1 1\n"
            'SetFunctionValue (sv_Construct "%g %g %g" fA fB (GetFactionCombatReaction rObj rOther))', True),
       # [hypothesis: a temporary reaction wins until SetAlly/SetEnemy is called on the pair]
       _so3("factions.saved.after.temp", _form("rObj", SO3F["boonefac"]) + _form("rOther", SO3F["mercfac"])
            + "SetAllyTemp rObj rOther\nSetEnemy rObj rOther 1 1\nlet fA := GetFactionCombatReaction rObj rOther\n"
            "SetEnemyTemp rObj rOther 1 1\nSetFunctionValue fA")]
    # ---- a reference's radiation [documented: 0 without; set adds it] ----
    + [_so3("radiation.player", "let fSave := rP.GetRadiationExtraData\nlet fRes := rP.SetRadiationExtraData 20\n"
            "let fA := rP.GetRadiationExtraData\nrP.SetRadiationExtraData fSave\n"
            'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fRes fA (rP.GetRadiationExtraData))', True),
       _so3f("radiation.static", "runway", "rObj.GetRadiationExtraData")]
    # ---- ModNumericINISetting [documented: 1 changed, 0 not found] ----
    + [_so3("ini.mod", 'let fSave := GetNumericINISetting "fDefaultFOV:Display"\nlet fA := ModNumericINISetting "fDefaultFOV:Display" 5\n'
            'let fB := GetNumericINISetting "fDefaultFOV:Display"\nlet fC := ModNumericINISetting "fDefaultFOV:Display" -5\n'
            'SetFunctionValue (sv_Construct "%g %g %g %g %g" fSave fA fB fC (GetNumericINISetting "fDefaultFOV:Display"))', True),
       _so3v("ini.mod.missing", 'ModNumericINISetting "nvfNoSuchSetting:General" 1')]
    # ---- TopicInfoGetResponseStrings [documented: the texts in order]: count and lengths ----
    + [_so3(f"responses.{k}", _form("rObj", SO3F[k]) + "let aList := TopicInfoGetResponseStrings rObj\nlet fA := ar_Size aList\n"
            'let sText := sv_Construct "%g:" fA\nlet fB := 0\nwhile fB < fA\nlet sSave := aList[fB]\n'
            'let sText := sv_Construct "%z %g" sText (sv_Length sSave)\nlet fB += 1\nloop\nSetFunctionValue sText', True)
       for k in ("caravan", "bye", "stimpak")]
    # ---- aux timers and their handlers; one step per frame, in order ----
    # A lambda kept in a ref variable and called [diagnostic for the rows below].
    + [_so3("aux.lambda.call", "let rH := (Begin Function { }\nSetFunctionValue 7\nEnd)\nlet fA := Call rH\nSetFunctionValue fA"),
       # AuxTimerTimeToCountdown [documented: the time AuxTimerStart gave, -1 when not
       # running; hypothesis: names are case-insensitive].
       _so3v("aux.countdown.none", 'AuxTimerTimeToCountdown "*nvfSO3T"'),
       # S1: the start handler on the player's "*nvfSO3T", a second one on the runway
       # piece's; the player's timer started [hypothesis: the handler runs inside
       # AuxTimerStart, so the count is 1 in the same step].
       _so3("aux.start.reg", _form("rObj", SO3F["runway"]) + f"let rH := {_SO3_START}\nlet rH2 := {_SO3_START}\n"
            'rP.AuxVarSetRef "*nvfSO3H1" rH\nrP.AuxVarSetRef "*nvfSO3H2" rH2\n'
            'SetOnAuxTimerStartHandler 1 rH "*nvfSO3T"\nSetOnAuxTimerStartHandler 1 rH2 "*nvfSO3T" rObj\n'
            'AuxTimerStart "*nvfSO3T" 50\n'
            f'SetFunctionValue (sv_Construct "%g %g %g" {_so3_count("S", "SL")} {_so3_count("S", owner="rObj")})', True),
       # S2: the counts a frame later; the timer restarted (a restart counts too), the
       # runway piece's own timer started.
       _so3("aux.start.restart", _form("rObj", SO3F["runway"])
            + f'let sText := sv_Construct "%g %g |" {_so3_count("S")} {_so3_count("S", owner="rObj")}\n'
            'AuxTimerStart "*nvfSO3T" 60\nrObj.AuxTimerStart "*nvfSO3T" 30\n'
            f'SetFunctionValue (sv_Construct "%z %g %g" sText {_so3_count("S")} {_so3_count("S", owner="rObj")})', True),
       _so3("aux.countdown", _form("rObj", SO3F["runway"])
            + 'SetFunctionValue (sv_Construct "%g %g %g %g" (AuxTimerTimeToCountdown "*nvfSO3T") (rObj.AuxTimerTimeToCountdown "*nvfSO3T") '
            '(AuxTimerTimeToCountdown "*nvfSO3T" rObj) (AuxTimerTimeToCountdown "*NVFSO3T"))', True),
       # S3: the counts; the player's handler removed, the timer started again.
       _so3("aux.start.remove", f'let sText := sv_Construct "%g |" {_so3_count("S")}\nlet rH := rP.AuxVarGetRef "*nvfSO3H1"\n'
            'SetOnAuxTimerStartHandler 0 rH "*nvfSO3T"\nAuxTimerStart "*nvfSO3T" 70\n'
            f'SetFunctionValue (sv_Construct "%z %g %g" sText (rH != 0) {_so3_count("S")})', True),
       # S4: a frame later the count has not moved; the timers stopped.
       _so3("aux.start.after", _form("rObj", SO3F["runway"]) + f'let sText := sv_Construct "%g" {_so3_count("S")}\n'
            'AuxTimerStop "*nvfSO3T"\nrObj.AuxTimerStop "*nvfSO3T"\n'
            'SetFunctionValue (sv_Construct "%z %g %g" sText (AuxTimerTimeToCountdown "*nvfSO3T") (rObj.AuxTimerTimeToCountdown "*nvfSO3T"))', True),
       # U1: update handlers on a seconds timer with flag 256 (U), a frames timer with flag
       # 256 (F) and a seconds timer without it (V) [documented: only flag 256; the time
       # since the last update, 1 for a frames timer].
       _so3("aux.update.reg", "let rH := " + _SO3_UPD.replace("{c}", "U") + "\nlet rH2 := " + _SO3_UPD.replace("{c}", "V") + "\n"
            'rP.AuxVarSetRef "*nvfSO3H3" rH\nrP.AuxVarSetRef "*nvfSO3H4" rH2\n'
            'SetOnAuxTimerUpdateHandler 1 rH "*nvfSO3U"\nSetOnAuxTimerUpdateHandler 1 rH "*nvfSO3F"\n'
            'SetOnAuxTimerUpdateHandler 1 rH2 "*nvfSO3V"\n'
            'AuxTimerStart "*nvfSO3U" 100 263\nAuxTimerStart "*nvfSO3V" 100 7\n'
            f'SetFunctionValue (sv_Construct "%g %g" {_so3_count("U", "V")})', True),
       # The seconds between updates as "above 0 and below 0.1", not the frame time itself
       # (rewritten 2026-11-08).
       _so3("aux.update.counts", 'let fA := rP.AuxVarGetFlt "*nvfSO3UD"\n'
            f'SetFunctionValue (sv_Construct "%g %g %g" {_so3_count("U")} ((fA > 0) && (fA < 0.1)) {_so3_count("V")})', True),
       _so3("aux.update.frames.start", 'rP.AuxVarSetFlt "*nvfSO3U" 0\nAuxTimerStop "*nvfSO3U"\nAuxTimerStart "*nvfSO3F" 100000 259\n'
            f'SetFunctionValue (sv_Construct "%g" (AuxTimerTimeToCountdown "*nvfSO3F"))', True),
       _so3("aux.update.frames", f'SetFunctionValue (sv_Construct "%g %.4f %g" {_so3_count("U", "UD", "V")})', True),
       # U4: the handlers removed; a frame later the counts have not moved.
       _so3("aux.update.remove", 'let rH := rP.AuxVarGetRef "*nvfSO3H3"\nlet rH2 := rP.AuxVarGetRef "*nvfSO3H4"\n'
            'SetOnAuxTimerUpdateHandler 0 rH "*nvfSO3U"\nSetOnAuxTimerUpdateHandler 0 rH "*nvfSO3F"\n'
            'SetOnAuxTimerUpdateHandler 0 rH2 "*nvfSO3V"\n'
            f'SetFunctionValue (sv_Construct "%g %g %g" (rH != 0) (rH2 != 0) {_so3_count("U")})', True),
       _so3("aux.update.after", f'SetFunctionValue (sv_Construct "%g %g" {_so3_count("U", "V")})', True),
       # Cleanup: every handler removed again (by the refs kept), every timer stopped,
       # every aux variable erased.
       _so3("aux.cleanup", _form("rObj", SO3F["runway"])
            + 'let rH := rP.AuxVarGetRef "*nvfSO3H1"\nlet rH2 := rP.AuxVarGetRef "*nvfSO3H2"\n'
            'if rH\nSetOnAuxTimerStartHandler 0 rH "*nvfSO3T"\nendif\nif rH2\nSetOnAuxTimerStartHandler 0 rH2 "*nvfSO3T" rObj\nendif\n'
            'let rH := rP.AuxVarGetRef "*nvfSO3H3"\nlet rH2 := rP.AuxVarGetRef "*nvfSO3H4"\n'
            'if rH\nSetOnAuxTimerUpdateHandler 0 rH "*nvfSO3U"\nSetOnAuxTimerUpdateHandler 0 rH "*nvfSO3F"\nendif\n'
            'if rH2\nSetOnAuxTimerUpdateHandler 0 rH2 "*nvfSO3V"\nendif\n'
            'AuxTimerStop "*nvfSO3T"\nrObj.AuxTimerStop "*nvfSO3T"\nAuxTimerStop "*nvfSO3U"\nAuxTimerStop "*nvfSO3F"\n'
            'AuxTimerStop "*nvfSO3V"\n'
            + "".join(f'rP.AuxVarErase "*nvfSO3{n}"\n' for n in ("S", "SL", "H1", "H2", "H3", "H4", "U", "UD", "V", "VD"))
            + 'rObj.AuxVarErase "*nvfSO3S"\nrObj.AuxVarErase "*nvfSO3SL"\n'
            'SetFunctionValue (sv_Construct "%g %g %g" (AuxTimerTimeToCountdown "*nvfSO3U") (AuxTimerTimeToCountdown "*nvfSO3F") '
            '(AuxTimerTimeToCountdown "*nvfSO3T"))', True)]
)


# showoff4 (writes, session only, each change put back; #1010): ShowOff batch 4
# (docs/spec-notes/ext-showoff.md §7): the matrix and quaternion functions (the
# quaternion's convention and normalising are hypotheses), SayTo_GetTopic on Doc Mitchell,
# the switches' defaults and their return values, the INI cache and ReadFromJSON on files
# that do not exist (nothing is written to a file), DisableAlt / EnableAlt on Doc's
# reference and the hotkey icon override of the 10mm pistol.
_SO4_HEAD = ("ref rP\nref rDoc\nref rObj\narray_var aM\narray_var aR\nfloat fRes\nBegin Function { }\n")


def _so4(pid, body, text=False):
    return (f"showoff4.{pid}", _SO4_HEAD + _form("rP", NF["player"]) + _form("rDoc", NF["docref"]) + body, text, None, "")


def _so4v(pid, expr, text=False):
    return _so4(pid, f"SetFunctionValue ({expr})", text)


def _so4m(pid, make, cells):
    """A matrix made by `make` (into aR), its cells as text "a b c ..." or its ar_Size."""
    shown = " ".join("%g" for _ in cells)
    args = " ".join(f"(aR{c})" for c in cells)
    return _so4(pid, f"let aR := {make}\nif eval (ar_Size aR) < 1\nSetFunctionValue \"none\"\nreturn\nendif\n"
                     f"SetFunctionValue (sv_Construct \"{shown}\" {args})", True)


_M2 = "(ar_List (ar_List 3, 4) (ar_List 5, 6))"
SHOWOFF4_PROBES = (
    [_so4v("ismat.text", 'Matrix_IsMatrix (ar_List "test")'),
     _so4v("ismat.one", "Matrix_IsMatrix (ar_List (ar_List 1))"),
     _so4v("ismat.ragged", "Matrix_IsMatrix (ar_List (ar_List 1), (ar_List 2, 3))"),
     _so4v("ismat.mixed", "Matrix_IsMatrix (ar_List 5, (ar_List 2, 3))"),
     _so4v("ismat.flat", "Matrix_IsMatrix (ar_List 1, 2, 3)"),
     _so4v("ismat.empty", "Matrix_IsMatrix (ar_List)"),
     _so4m("mul.2x2", f'Mat_ApplyOpWithMat {_M2} "*" (ar_List (ar_List 7, 8) (ar_List 9, 10))', ["[0][0]", "[0][1]", "[1][0]", "[1][1]"]),
     _so4v("mul.undefined.size", 'ar_Size (Mat_ApplyOpWithMat (ar_List 1, 2, 3) "*" (ar_List 1, 2, 3))'),
     # A 1-D input: the result is 1-D (ar_Size 3) or a list of one row (1) [hypothesis: 1-D].
     _so4v("schur.flat.size", 'ar_Size (Mat_ApplyOpWithMat (ar_List 1, 2, 3) "%" (ar_List 1, 2, 3))'),
     _so4m("schur.flat", 'Mat_ApplyOpWithMat (ar_List 1, 2, 3) "%" (ar_List 1, 2, 3)', ["[0]", "[1]", "[2]"]),
     _so4m("div.2x2", f'Mat_ApplyOpWithMat {_M2} "/" {_M2}', ["[0][0]", "[1][1]"]),
     _so4m("scal.div", f'Mat_ApplyOpWithScal {_M2} "/" 2', ["[0][0]", "[0][1]", "[1][0]", "[1][1]"]),
     _so4v("scal.mod.size", f'ar_Size (Mat_ApplyOpWithScal {_M2} "%" 2)'),
     # Division by zero: inf or something else [hypothesis: the floating-point infinity].
     _so4m("scal.div0", f'Mat_ApplyOpWithScal {_M2} "/" 0', ["[0][0]"]),
     _so4m("transpose.2x2", f"Mat_T {_M2}", ["[0][0]", "[0][1]", "[1][0]", "[1][1]"]),
     # A 1-D row transposed: three rows of one [hypothesis].
     _so4v("transpose.flat.size", "ar_Size (Mat_T (ar_List 1, 2, 3))"),
     # The quaternion's convention: 90 degrees about Z gives m[0][1] -1 and m[1][0] 1 in
     # the usual right-handed form [hypothesis].
     _so4m("quat.z90", "Quat_GetMat (ar_List 0.70710678, 0, 0, 0.70710678)", ["[0][0]", "[0][1]", "[1][0]", "[2][2]"]),
     # Not of unit length: normalised first, the identity, or something else [hypothesis].
     _so4m("quat.unnormal", "Quat_GetMat (ar_List 2, 0, 0, 0)", ["[0][0]", "[1][1]", "[2][2]"]),
     _so4m("quat.back", "Mat_GetQuat (ar_List (ar_List 0, -1, 0) (ar_List 1, 0, 0) (ar_List 0, 0, 1))", ["[0]", "[1]", "[2]", "[3]"]),
     # The sign: w kept not negative [hypothesis]; a rotation of 270 degrees about Z.
     _so4m("quat.back.270", "Mat_GetQuat (ar_List (ar_List 0, 1, 0) (ar_List -1, 0, 0) (ar_List 0, 0, 1))", ["[0]", "[1]", "[2]", "[3]"]),
     _so4v("sayto.topic.doc", "GetFormIDString (rDoc.SayTo_GetTopic)", True),
     _so4v("sayto.info.doc", "GetFormIDString (rDoc.SayTo_GetTopicInfo)", True),
     _so4v("pickpocket.default", "GetPlayerCanPickpocketEquippedItems"),
     # The read: 1 while quest messages are on [hypothesis].
     _so4v("questmsgs.read", "ToggleQuestMessages"),
     _so4("questmsgs.off", "let fRes := ToggleQuestMessages 0\nSetFunctionValue (sv_Construct \"%g %g %g\" fRes (ToggleQuestMessages 0) (ToggleQuestMessages))", True),
     _so4("questmsgs.restore", "let fRes := ToggleQuestMessages 1\nSetFunctionValue (sv_Construct \"%g %g\" fRes (ToggleQuestMessages))", True),
     _so4v("freezeammo.read", "FreezeAmmoRegen"),
     _so4("freezeammo.on", "let fRes := FreezeAmmoRegen 1\nSetFunctionValue (sv_Construct \"%g %g\" fRes (FreezeAmmoRegen))", True),
     _so4("freezeammo.restore", "let fRes := FreezeAmmoRegen 0\nSetFunctionValue (sv_Construct \"%g %g\" fRes (FreezeAmmoRegen))", True),
     _so4("hotkeyicon.pistol", _form("rObj", NF["pistol10"]) + "SetFunctionValue (sv_Construct \"[%z] [%z]\" (GetItemHotkeyIconPath rObj 1) (GetItemHotkeyIconPath rObj))", True),
     _so4("hotkeyicon.set", _form("rObj", NF["pistol10"]) + "SetItemHotkeyIconPath rObj \"interface\\icons\\nvfprobe.dds\"\nSetFunctionValue (GetItemHotkeyIconPath rObj 1)", True),
     _so4("hotkeyicon.restore", _form("rObj", NF["pistol10"]) + "SetItemHotkeyIconPath rObj \"\"\nSetFunctionValue (sv_Construct \"[%z]\" (GetItemHotkeyIconPath rObj 1))", True),
     _so4v("hasini.missing", 'HasINISetting_Cached "Main:fNone" "NVFExtProbe\\nvfso4missing"'),
     _so4v("reloadini.missing", 'ReloadIniCache "NVFExtProbe\\nvfso4missing"'),
     _so4v("readjson.missing", 'TestExpr (ReadFromJSON "nvfso4missing.json")'),
     _so4("disablealt.doc", "rDoc.DisableAlt\nlet fRes := rDoc.GetDisabled\nrDoc.EnableAlt 0\nSetFunctionValue (sv_Construct \"%g %g\" fRes (rDoc.GetDisabled))", True),
     _so4v("patchfreezetime", "PatchFreezeTime"),
     _so4v("fadestate.doc", "rDoc.GetActorFadeState")]
)


# sup5 (writes files; #1008): SUP batch 5 (docs/spec-notes/ext-sup5.md): Excel workbooks,
# JSON files, file sizes and times, the load-order dump, screenshots and QR codes. The rows
# write under the game folder's NVFProbe\sup5 (Excel, JSON through ppNVSE's pWriteFile, the
# QR code), SUPNVSE.log in the game folder, and one screenshot in
# Data\textures\SUPScreenshots\NVFProbe, which the screenshot rows delete again. No script
# can remove the rest: after the run, delete the game folder's NVFProbe folder and
# SUPNVSE.log by hand. Nothing touches the save. The network and dialog functions are not
# probed (decisions: no network, no dialogs; a dialog would halt the game).
def _sup5(pid, body, text=True):
    return (f"sup5.{pid}", "float fA\nfloat fB\nstring_var sText\narray_var aList\narray_var aIn\nref rP\n"
            "Begin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


_SUP5_X = "NVFProbe\\sup5\\book"
_SUP5_A = "NVFProbe\\sup5\\arr"
_SUP5_N = "NVFProbe\\sup5\\none"
_SUP5_JSON = "NVFProbe\\sup5\\probe.json"


def _sup5_list(expr):
    return (f"let aList := {expr}\nlet fA := ar_Size aList\nif fA > 0\n"
            "SetFunctionValue (sv_Construct \"%g|%z|%z\" fA (aList[0]) (aList[fA - 1]))\nelse\n"
            "SetFunctionValue (sv_Construct \"%g\" fA)\nendif")


SUP5_PROBES = (
    # A missing workbook: -1 [documented: every Excel page's note]; ReadString's text is
    # NVForever's hypothesis "".
    [_sup5("xl.missing", f"SetFunctionValue (sv_Construct \"%g %g %g %g\" (ExcelReadFloat \"{_SUP5_N}\" 0 0 0) "
           f"(ExcelGetWorkSheetCount \"{_SUP5_N}\") (ExcelGetRowCount \"{_SUP5_N}\" 0) (ExcelGetCellValueType \"{_SUP5_N}\" 0 0 0))"),
     _sup5("xl.missing.string", f"SetFunctionValue (sv_Construct \"[%z]\" (ExcelReadString \"{_SUP5_N}\" 0 0 0))"),
     # Writes: the returns, then the values' types (hypothesis: a written number is a
     # float, 2, a whole one too).
     _sup5("xl.write", f"let fA := ExcelWriteFloat \"{_SUP5_X}\" 0 0 0 10\nlet fB := ExcelWriteString \"{_SUP5_X}\" 0 4 1 \"MyText\"\n"
           "SetFunctionValue (sv_Construct \"%g %g\" fA fB)"),
     _sup5("xl.read", f"SetFunctionValue (sv_Construct \"%g [%z] %g %g %g\" (ExcelReadFloat \"{_SUP5_X}\" 0 0 0) "
           f"(ExcelReadString \"{_SUP5_X}\" 0 4 1) (ExcelGetCellValueType \"{_SUP5_X}\" 0 0 0) "
           f"(ExcelGetCellValueType \"{_SUP5_X}\" 0 4 1) (ExcelGetCellValueType \"{_SUP5_X}\" 0 1 1))"),
     _sup5("xl.types", f"ExcelWriteFloat \"{_SUP5_X}\" 0 1 0 3.25\nExcelWriteString \"{_SUP5_X}\" 0 2 0 \"\"\n"
           f"SetFunctionValue (sv_Construct \"%g %g\" (ExcelGetCellValueType \"{_SUP5_X}\" 0 1 0) (ExcelGetCellValueType \"{_SUP5_X}\" 0 2 0))"),
     # Across kinds (hypothesis: a text cell's number 0, a number cell's text "").
     _sup5("xl.cross", f"SetFunctionValue (sv_Construct \"%g [%z]\" (ExcelReadFloat \"{_SUP5_X}\" 0 4 1) (ExcelReadString \"{_SUP5_X}\" 0 0 0))"),
     # Rows and columns in use (hypothesis: the last used + 1, here 5 and 2).
     _sup5("xl.counts", f"SetFunctionValue (sv_Construct \"%g %g %g\" (ExcelGetRowCount \"{_SUP5_X}\" 0) (ExcelGetColCount \"{_SUP5_X}\" 0) "
           f"(ExcelGetWorkSheetCount \"{_SUP5_X}\"))"),
     # A sheet past the last (hypothesis: the sheets between made, named Sheet1, Sheet2, Sheet3).
     _sup5("xl.sheet3", f"let fA := ExcelWriteFloat \"{_SUP5_X}\" 2 0 0 1.5\n"
           f"SetFunctionValue (sv_Construct \"%g %g [%z] [%z] [%z]\" fA (ExcelGetWorkSheetCount \"{_SUP5_X}\") "
           f"(ExcelSetSheetName \"{_SUP5_X}\" 0) (ExcelSetSheetName \"{_SUP5_X}\" 1) (ExcelSetSheetName \"{_SUP5_X}\" 2))"),
     # A missing sheet (hypothesis: -1, -1 and "").
     _sup5("xl.missing.sheet", f"SetFunctionValue (sv_Construct \"%g %g [%z]\" (ExcelGetRowCount \"{_SUP5_X}\" 7) "
           f"(ExcelReadFloat \"{_SUP5_X}\" 7 0 0) (ExcelSetSheetName \"{_SUP5_X}\" 7))"),
     # Renaming: cut to 31 [documented]; the return (hypothesis: the new name).
     _sup5("xl.rename", f"SetFunctionValue (sv_Construct \"[%z] [%z]\" (ExcelSetSheetName \"{_SUP5_X}\" 1 \"A name far longer than thirty-one letters\") "
           f"(ExcelSetSheetName \"{_SUP5_X}\" 1))"),
     # Erasing (hypothesis: 1 each; the erased sheet kept, so 3 sheets).
     _sup5("xl.erase", f"let fA := ExcelEraseCell \"{_SUP5_X}\" 0 4 1\nlet fB := ExcelEraseSheet \"{_SUP5_X}\" 2\n"
           f"SetFunctionValue (sv_Construct \"%g %g %g %g %g\" fA fB (ExcelGetCellValueType \"{_SUP5_X}\" 0 4 1) "
           f"(ExcelGetRowCount \"{_SUP5_X}\" 2) (ExcelGetWorkSheetCount \"{_SUP5_X}\"))"),
     # An array down a column, the player as a name [documented: "Courier"].
     _sup5("xl.arr.to", "let aIn := ar_Construct \"array\"\nar_Append aIn 2\nar_Append aIn \"MyString\"\nar_Append aIn 3.5\nar_Append aIn rP\n"
           f"let fA := ExcelArrayToFile \"{_SUP5_A}\" 0 0 0 aIn\n"
           f"SetFunctionValue (sv_Construct \"%g %g [%z] %g [%z] %g\" fA (ExcelReadFloat \"{_SUP5_A}\" 0 0 0) "
           f"(ExcelReadString \"{_SUP5_A}\" 0 1 0) (ExcelReadFloat \"{_SUP5_A}\" 0 2 0) "
           f"(ExcelReadString \"{_SUP5_A}\" 0 3 0) (ExcelGetCellValueType \"{_SUP5_A}\" 0 3 0))"),
     # xl.arr.row (ExcelArrayToFile along a row onto sheet 1 of a one-sheet workbook)
     # crashed the original [observed 2026-11-08]: left out.
     # Back as an array, a gap at rows 4-5 and a form at row 6 (hypothesis: gaps passed over).
     _sup5("xl.arr.from", f"ExcelWriteString \"{_SUP5_A}\" 0 6 0 \"*FalloutNV.esm:000014\"\n"
           f"let aList := ExcelArrayFromFile \"{_SUP5_A}\" 0 0 0\n"
           "SetFunctionValue (sv_Construct \"%g %g %z %g %z %i\" (ar_Size aList) (aList[0]) (aList[1]) (aList[2]) (aList[3]) "
           "(aList[(ar_Size aList) - 1]))"),
     _sup5("xl.arr.missing", f"let aList := ExcelArrayFromFile \"{_SUP5_N}\" 0 0 0\nSetFunctionValue (ar_Size aList)", False),
     # JSON: the file written by pWriteFile as one line; %q is a double quote.
     _sup5("json.write", "let aIn := ar_Construct \"array\"\n"
           "ar_Append aIn (sv_Construct \"{%qname%q:%qJoe%q,%qlevel%q:7,%qinv%q:[1,%qtwo%q],%qpos%q:{%qx%q:1.5}}\")\n"
           f"SetFunctionValue (pWriteFile \"{_SUP5_JSON}\" aIn)", False),
     # [documented: JSONIsFileValid's table] valid, missing, not .json, outside.
     _sup5("json.valid", f"SetFunctionValue (sv_Construct \"%g %g %g %g\" (JSONIsFileValid \"{_SUP5_JSON}\") "
           f"(JSONIsFileValid \"{_SUP5_N}.json\") (JSONIsFileValid \"{_SUP5_X}.xls\") (JSONIsFileValid \"..\\NVFProbe\\probe.json\"))"),
     # The values (hypothesis: a value in a one-element list, a list as itself, an object
     # as a map; a dotted path; no array for a missing key).
     _sup5("json.name", _sup5_list(f"JSONGetValue \"{_SUP5_JSON}\" \"name\" 0")),
     _sup5("json.level", _sup5_list(f"JSONGetValue \"{_SUP5_JSON}\" \"level\" 0")),
     _sup5("json.inv", _sup5_list(f"JSONGetValue \"{_SUP5_JSON}\" \"inv\" 1")),
     _sup5("json.pos", f"let aList := JSONGetValue \"{_SUP5_JSON}\" \"pos\" 0\nSetFunctionValue (sv_Construct \"%g %g\" (ar_Size aList) (aList[\"x\"]))"),
     _sup5("json.path", _sup5_list(f"JSONGetValue \"{_SUP5_JSON}\" \"pos.x\" 0")),
     _sup5("json.missing", f"let aList := JSONGetValue \"{_SUP5_JSON}\" \"nope\" 0\nSetFunctionValue (ar_Size aList)", False),
     # Sizes and times (hypothesis: missing -1; the time as year … second).
     _sup5("file.size", f"SetFunctionValue (sv_Construct \"%g %g\" (GetFileSize \"{_SUP5_JSON}\") (GetFileSize \"{_SUP5_N}.txt\"))"),
     _sup5("file.time", f"let aList := GetFileTime \"{_SUP5_JSON}\" 2\nlet fA := ar_Size aList\n"
           "SetFunctionValue (sv_Construct \"%g|%g|%g\" fA (aList[0]) (aList[1]))"),
     _sup5("file.time.missing", f"let aList := GetFileTime \"{_SUP5_N}.txt\" 2\nSetFunctionValue (ar_Size aList)", False),
     _sup5("file.timestr", f"let sText := GetFileTimeSTR \"{_SUP5_JSON}\" 2 0\n"
           f"SetFunctionValue (sv_Construct \"%g %g [%z]\" (sv_Length sText) (sv_Find \"/\" sText) (GetFileTimeSTR \"{_SUP5_N}.txt\" 2 0))"),
     # The load-order dump's line count, first and last lines (hypothesis: "00 FalloutNV.esm").
     _sup5("dump.lo", "DumpLoadOrder\nlet aList := pReadFile \"SUPNVSE.log\"\nlet fA := ar_Size aList\n"
           "SetFunctionValue (sv_Construct \"%g|%z|%z\" fA (aList[0]) (aList[fA - 1]))"),
     # A part of the screen as a PNG, its size, then deleted [documented: DeleteScreenshot,
     # -1 missing, -2 outside]; the success value is NVForever's hypothesis 1.
     _sup5("shot.take", "CaptureScreenshotAlt \"NVFProbe\" \"shot\" 0 100 0 50 4 1 0\nSetFunctionValue 1", False),
     _sup5("shot.size", "SetFunctionValue (GetFileSize \"Data\\textures\\SUPScreenshots\\NVFProbe\\shot.png\")", False),
     _sup5("shot.delete", "SetFunctionValue (sv_Construct \"%g %g %g\" (DeleteScreenshot \"NVFProbe\" \"shot.png\") "
           "(DeleteScreenshot \"NVFProbe\" \"shot.png\") (DeleteScreenshot \"..\\..\" \"shot.png\"))"),
     # The QR code: where it goes (the game folder or Data) and its size (hypothesis: the
     # game folder, 10 pixels a module).
     _sup5("qr.make", "TextToQRCode \"NVFProbe\\sup5\\qr\" \"I love bacon\" 0\nSetFunctionValue 1", False),
     _sup5("qr.where", "SetFunctionValue (sv_Construct \"%g %g\" (GetFileSize \"NVFProbe\\sup5\\qr.png\") (GetFileSize \"Data\\NVFProbe\\sup5\\qr.png\"))"),
     _sup5("nothing.program", "SetFunctionValue (IsProgramRunning \"nvf_no_such_program.exe\")", False)]
)


# jip6 and jip7 (writes, session only, each change put back where a getter allows; #1006):
# JIP LN batches 6 and 7 (docs/spec-notes/jip-records-6.md, jip-records-7.md) on
# FalloutNV.esm forms [observed: fnvtool esm inventory and decode]: FusionGenerator01 (DEST,
# VATS targetable, four stages; stage 2 Cap Damage + Destroy, explosion 0x7015E and a
# replacement model), the 10mm round (no destruction data; clip 12, consumed 0x140AA7 at 20%),
# VStreetCrBrahmin2 (damage 8, reach 32, sounds from 0x28047, one model),
# CrSuperMutantDeadMinigunRubyHill (two models, weapon list 0x3A15E), Craig Boone (an NPC),
# VCGTutorialJump (MESG, flags 0, 10 s), 10mmBulletProjectile, GrenadeFragExplosion,
# BaseBallBatMetalImpact (IPCT), the 10mm pistol (impact data set 0x19083),
# ThrownImpactDataSet, NVWastelandClear and Hazy, NVDefaultClimate (two weathers),
# AmmoEffectMagnumDTReduction, ArmorLeather (no class flags).
JP = {"fusion": "81932", "ammo": "4241", "brahmin": "17B587", "mutant": "17AF62", "boone": "92BD2",
      "msg": "16768C", "proj": "2CD5F", "expl": "179DA", "ipct": "6C666", "pistol": "434F", "ipds": "176A12",
      "ipdsold": "19083", "clear": "FFC88", "hazy": "100A4E", "climate": "8809B", "amef": "17BA46",
      "leather": "20423", "stageexpl": "7015E", "casing": "140AA7", "brsounds": "28047", "light": "1E879"}
JP_DECLS = ("ref rP\nref rObj\nref rOther\nref rItem\nfloat fRes\nfloat fSave\nfloat fA\nfloat fB\nfloat fC\n"
            "string_var sText\narray_var aList\narray_var aKeys\nBegin Function { }\n")


def _jp(set_, pid, body, text=False):
    return (f"{set_}.{pid}", JP_DECLS + _form("rP", NF["player"]) + body, text, None, "")


def _jpv(set_, pid, key, expr, text=False):
    rd = f"SetFunctionValue (GetFormIDString ({expr}))" if text == "form" else f"SetFunctionValue ({expr})"
    return _jp(set_, pid, _form("rObj", JP[key]) + rd, bool(text))


# A setter's result and the value read back, the old value put back, and the read again.
def _jpset(set_, pid, key, save, setter, read, restore, other=None):
    pre = _form("rObj", JP[key]) + (_form("rOther", JP[other]) if other else "")
    return _jp(set_, pid, pre + f"let fSave := {save}\nlet fRes := {setter}\nlet fA := {read}\n{restore}\n"
               'SetFunctionValue (sv_Construct "%g %g %g %g" fSave fRes fA (' + read + "))", True)


# A setter with no restore read (a form or text field): result, then the read as text.
def _jpset_form(set_, pid, key, setter, read, restore, other=None):
    pre = _form("rObj", JP[key]) + (_form("rOther", JP[other]) if other else "")
    return _jp(set_, pid, pre + f"let fRes := {setter}\nlet rItem := {read}\n{restore}\n"
               'SetFunctionValue (sv_Construct "%g %z %z" fRes (GetFormIDString rItem) (GetFormIDString (' + read + ")))", True)


# An array of strings or numbers, "size: v v ..." in key order.
def _jparr(set_, pid, key, expr):
    return _jp(set_, pid, _form("rObj", JP[key]) + f"let aList := {expr}\nlet aKeys := ar_Keys aList\nlet fA := ar_Size aKeys\n"
               'let sText := sv_Construct "%g:" fA\nlet fB := 0\nwhile fB < fA\nlet fC := aKeys[fB]\n'
               'let sText := sv_Construct "%z %z" sText (aList[fC])\nlet fB += 1\nloop\nSetFunctionValue sText', True)


J6, J7 = "jip6", "jip7"
JIP6_PROBES = (
    # ---- destruction data: reads of every stage and trait, a trait past 4 and a stage past the end
    [_jpv(J6, f"dest.target.{k}", k, "GetDestructionDataTargetable rObj") for k in ("fusion", "ammo")]
    + [_jpv(J6, f"dest.stages.{k}", k, "GetDestructionDataNumStages rObj") for k in ("fusion", "ammo")]
    + [_jpv(J6, f"dest.trait.{i}.{t}", "fusion", f"GetNthDestructionStageTrait rObj {i} {t}") for i in range(0, 5) for t in range(0, 6)]
    + [_jpv(J6, f"dest.expl.{i}", "fusion", f"GetNthDestructionStageExplosion rObj {i}", "form") for i in (1, 2, 4)]
    + [_jpv(J6, f"dest.debris.{i}", "fusion", f"GetNthDestructionStageDebris rObj {i}", "form") for i in (2,)]
    + [_jpv(J6, f"dest.repl.{i}", "fusion", f"GetNthDestructionStageReplacement rObj {i}", True) for i in (1, 2, 5)]
    + [_jpv(J6, "dest.trait.ammo", "ammo", "GetNthDestructionStageTrait rObj 0 0")]
    # setters: targetable, a trait in and out of range, flags past the table, NaN-free extremes
    + [_jpset(J6, "dest.settarget.0", "fusion", "GetDestructionDataTargetable rObj", "SetDestructionDataTargetable rObj 0",
              "GetDestructionDataTargetable rObj", "SetDestructionDataTargetable rObj 1")]
    + [_jpset(J6, f"dest.settrait.{i}.{t}.{v}", "fusion", f"GetNthDestructionStageTrait rObj {i} {t}",
              f"SetNthDestructionStageTrait rObj {i} {t} {v}", f"GetNthDestructionStageTrait rObj {i} {t}",
              f"SetNthDestructionStageTrait rObj {i} {t} fSave")
       for i, t, v in ((1, 0, 50), (1, 0, 150), (1, 0, -5), (1, 1, 3.5), (1, 2, 300), (1, 3, 8), (1, 3, 7), (1, 4, 3), (1, 5, 9))]
    + [_jpset_form(J6, "dest.setexpl.none", "fusion", "SetNthDestructionStageExplosion rObj 2",
                   "GetNthDestructionStageExplosion rObj 2", "SetNthDestructionStageExplosion rObj 2 rOther", "stageexpl")]
    + [_jp(J6, "dest.setrepl", _form("rObj", JP["fusion"]) + 'let fRes := SetNthDestructionStageReplacement rObj 1 "nvf\\probe.nif"\n'
           'let sText := GetNthDestructionStageReplacement rObj 1\n'
           'SetFunctionValue (sv_Construct "%g [%z]" fRes sText)', True)]
    # add and remove stages: at an index inside, past the end, and removing past the end
    + [_jp(J6, "dest.add.1", _form("rObj", JP["fusion"]) + "let fRes := AddDestructionStage rObj 1 60 9\n"
           "let fA := GetDestructionDataNumStages rObj\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g %g %g" fRes fA (GetNthDestructionStageTrait rObj 1 0) '
           "(GetNthDestructionStageTrait rObj 1 2) (GetNthDestructionStageTrait rObj 2 0) (GetNthDestructionStageTrait rObj 2 2))", True),
       _jp(J6, "dest.remove.1", _form("rObj", JP["fusion"]) + "let fRes := RemoveDestructionStage rObj 1\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g" fRes (GetDestructionDataNumStages rObj) '
           "(GetNthDestructionStageTrait rObj 1 0) (GetNthDestructionStageTrait rObj 1 2))", True),
       _jp(J6, "dest.add.99", _form("rObj", JP["fusion"]) + "let fRes := AddDestructionStage rObj 99 10 7\n"
           "let fA := GetDestructionDataNumStages rObj\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g" fRes fA (GetNthDestructionStageTrait rObj 4 0) '
           "(GetNthDestructionStageTrait rObj 3 0))", True),
       _jp(J6, "dest.remove.last", _form("rObj", JP["fusion"]) + "let fA := GetDestructionDataNumStages rObj\n"
           "let fRes := RemoveDestructionStage rObj (fA - 1)\n"
           'SetFunctionValue (sv_Construct "%g %g" fRes (GetDestructionDataNumStages rObj))', True),
       _jpv(J6, "dest.remove.99", "fusion", "RemoveDestructionStage rObj 99"),
       _jpv(J6, "dest.add.none", "ammo", "AddDestructionStage rObj 0 50 1")]
    # ---- creatures
    + [_jpv(J6, f"cr.damage.{k}", k, "GetCreatureDamage rObj") for k in ("brahmin", "mutant", "boone")]
    + [_jp(J6, "cr.damage.ref", "SetFunctionValue (rP.GetCreatureDamage)")]
    + [_jpv(J6, f"cr.reach.{k}", k, "GetCreatureReach rObj") for k in ("brahmin", "mutant", "boone")]
    + [_jpv(J6, f"cr.weaplist.{k}", k, "GetCreatureWeaponList rObj", "form") for k in ("brahmin", "mutant")]
    + [_jpv(J6, f"cr.sounds.{k}", k, "GetCreatureSoundsTemplate rObj", "form") for k in ("brahmin", "mutant", "boone")]
    + [_jparr(J6, f"cr.models.{k}", k, "GetCreatureModels rObj") for k in ("brahmin", "mutant")]
    + [_jpv(J6, f"cr.hasmodel.{n}", "mutant", f'CreatureHasModel "{p}" rObj')
       for n, p in (("exact", "NV_SM_v2_1_base.NIF"), ("case", "nv_sm_v2_1_base.nif"), ("none", "brahmin.nif"))]
    + [_jpset(J6, f"cr.setdamage.{v}", "brahmin", "GetCreatureDamage rObj", f"SetCreatureDamage {v} rObj",
              "GetCreatureDamage rObj", "SetCreatureDamage fSave rObj") for v in (99, 40000, -3)]
    + [_jpset_form(J6, "cr.setsounds.none", "brahmin", "SetCreatureSoundsTemplate rObj", "GetCreatureSoundsTemplate rObj",
                   "SetCreatureSoundsTemplate rObj rOther", "brsounds")]
    + [_jp(J6, "cr.toggle.off", _form("rObj", JP["mutant"]) + 'let fRes := ToggleCreatureModel "smspinebreakerheavy.nif" 0 rObj\n'
           'let fA := CreatureHasModel "smspinebreakerheavy.nif" rObj\nlet fB := ar_Size (GetCreatureModels rObj)\n'
           'let fC := ToggleCreatureModel "smspinebreakerheavy.nif" 1 rObj\n'
           'SetFunctionValue (sv_Construct "%g %g %g %g %g" fRes fA fB fC (ar_Size (GetCreatureModels rObj)))', True),
       _jpv(J6, "cr.toggle.notlisted", "mutant", 'ToggleCreatureModel "brahmin.nif" 1 rObj')]
    # ---- messages
    + [_jpv(J6, "msg.flags", "msg", "GetMessageFlags rObj"), _jpv(J6, "msg.disabled", "msg", "GetMessageDisabled rObj")]
    + [_jpset(J6, f"msg.setflags.{v}", "msg", "GetMessageFlags rObj", f"SetMessageFlags rObj {v}", "GetMessageFlags rObj",
              "SetMessageFlags rObj fSave") for v in (3, 1, 8)]
    + [_jpset(J6, "msg.setdisabled", "msg", "GetMessageDisabled rObj", "SetMessageDisabled rObj 1", "GetMessageDisabled rObj",
              "SetMessageDisabled rObj fSave")]
    + [_jp(J6, f"msg.settime.{n}", _form("rObj", JP["msg"]) + f"let fRes := SetMessageDisplayTime rObj {v}\n"
           "SetMessageDisplayTime rObj 10\nSetFunctionValue fRes") for n, v in (("7", 7), ("neg", -1))]
    + [_jp(J6, "msg.crafting", "let fA := ToggleCraftingMessages 0\nlet fB := ToggleCraftingMessages 1\n"
           'SetFunctionValue (sv_Construct "%g %g" fA fB)', True)]
)

JIP7_PROBES = (
    # ---- ammunition
    [_jpset(J7, f"ammo.trait.{t}.{v}", "ammo", f"GetAmmoTraitNumeric rObj {t}", f"SetAmmoTraitNumeric rObj {t} {v}",
            f"GetAmmoTraitNumeric rObj {t}", f"SetAmmoTraitNumeric rObj {t} fSave")
     for t, v in ((0, 2.5), (1, 3), (1, -5), (2, 150), (2, 35.5), (3, 300), (3, 7.6))]
    + [_jpv(J7, f"ammo.read.{n}", "ammo", e) for n, e in (("speed", "GetAmmoSpeed rObj"),
       ("consumed", "GetAmmoConsumedPercent rObj"), ("effects", "GetNumAmmoEffects rObj"))]
    + [_jpset_form(J7, "ammo.setproj", "ammo", "SetAmmoProjectile rObj rOther", "GetAmmoProjectile rObj",
                   "SetAmmoProjectile rObj", "proj"),
       _jpset_form(J7, "ammo.setcasing.none", "ammo", "SetAmmoCasing rObj", "GetAmmoCasing rObj",
                   "SetAmmoCasing rObj rOther", "casing")]
    + [_jp(J7, "ammo.effects.addremove", _form("rObj", JP["ammo"]) + _form("rOther", JP["amef"])
           + "let fA := AddAmmoEffect rObj rOther\nlet fB := AddAmmoEffect rObj rOther\nlet fC := GetNumAmmoEffects rObj\n"
           "let rItem := GetNthAmmoEffect rObj 0\nlet fRes := RemoveAmmoEffect rObj rOther\nlet fSave := RemoveAmmoEffect rObj rOther\n"
           'SetFunctionValue (sv_Construct "%g %g %g %z %g %g %g" fA fB fC (GetFormIDString rItem) fRes fSave (GetNumAmmoEffects rObj))', True)]
    + [_jpset(J7, f"amef.trait.{t}.{v}", "amef", f"GetAmmoEffectTraitNumeric rObj {t}",
              f"SetAmmoEffectTraitNumeric rObj {t} {v}", f"GetAmmoEffectTraitNumeric rObj {t}",
              f"SetAmmoEffectTraitNumeric rObj {t} fSave") for t, v in ((0, 9), (0, 3), (1, 5), (2, 0.5))]
    # ---- projectiles and explosions
    + [_jpv(J7, f"proj.trait.{t}", "proj", f"GetProjectileTraitNumeric rObj {t}") for t in range(0, 16)]
    + [_jpset(J7, f"proj.settrait.{t}.{v}", "proj", f"GetProjectileTraitNumeric rObj {t}",
              f"SetProjectileTraitNumeric rObj {t} {v}", f"GetProjectileTraitNumeric rObj {t}",
              f"SetProjectileTraitNumeric rObj {t} fSave")
       for t, v in ((0, 99), (0, 2), (1, 1.5), (2, 1000), (4, 2), (14, 5), (14, 2))]
    + [_jpset_form(J7, "proj.setlight", "proj", "SetProjectileLight rObj rOther", "GetProjectileLight rObj",
                   "SetProjectileLight rObj", "light"),
       _jp(J7, "proj.setflash", _form("rObj", JP["proj"]) + 'let fRes := SetProjectileMuzzleFlash rObj "nvf\\probe.nif"\n'
           'let sText := GetProjectileMuzzleFlash rObj\n'
           'SetProjectileMuzzleFlash rObj "Effects\\MuzzleFlashes\\HandGunMuzzleFlash01.NIF"\n'
           'SetFunctionValue (sv_Construct "%g [%z]" fRes sText)', True)]
    + [_jpv(J7, f"expl.trait.{t}", "expl", f"GetExplosionTraitNumeric rObj {t}") for t in range(0, 9)]
    + [_jpset(J7, f"expl.settrait.{t}.{v}", "expl", f"GetExplosionTraitNumeric rObj {t}",
              f"SetExplosionTraitNumeric rObj {t} {v}", f"GetExplosionTraitNumeric rObj {t}",
              f"SetExplosionTraitNumeric rObj {t} fSave") for t, v in ((1, 200), (7, 9), (7, 1), (2, -10))]
    # ---- impact data
    + [_jpv(J7, f"ipct.trait.{t}", "ipct", f"GetImpactDataTrait rObj {t}", "form" if t in (5, 6, 7) else False)
       for t in range(0, 19)]
    + [_jpset(J7, f"ipct.settrait.{t}.{v}", "ipct", f"GetImpactDataTrait rObj {r}", f"SetImpactDataTraitNumeric rObj {t} {v}",
              f"GetImpactDataTrait rObj {r}", f"SetImpactDataTraitNumeric rObj {t} fSave")
       for t, r, v in ((0, 0, 0.5), (1, 1, 5), (13, 16, 99), (13, 16, 6), (14, 17, 300300300), (14, 17, 10020030))]
    + [_jp(J7, "ipct.setform.sound1", _form("rObj", JP["ipct"]) + _form("rOther", JP["brsounds"])
           + "let rItem := GetImpactDataTrait rObj 6\nlet fRes := SetImpactDataTraitForm rObj 1 rOther\n"
           "let rP := GetImpactDataTrait rObj 6\nSetImpactDataTraitForm rObj 1 rItem\n"
           'SetFunctionValue (sv_Construct "%g %z %z %z" fRes (GetFormIDString rItem) (GetFormIDString rP) '
           "(GetFormIDString (GetImpactDataTrait rObj 6)))", True)]
    + [_jp(J7, "ipds.set", _form("rObj", JP["pistol"]) + _form("rOther", JP["ipds"]) + _form("rItem", JP["ipdsold"])
           + "let fRes := SetImpactDataSet rObj rOther\nSetImpactDataSet rObj rItem\nSetFunctionValue fRes")]
    + [_jpv(J7, f"mat.{k}", k, "GetImpactMaterialType rObj") for k in ("boone", "brahmin", "ammo")]
    + [_jpset(J7, f"mat.set.{v}", "boone", "GetImpactMaterialType rObj", f"SetImpactMaterialType {v} rObj",
              "GetImpactMaterialType rObj", "SetImpactMaterialType fSave rObj") for v in (5, 15, 11)]
    # ---- weathers and climates
    + [_jpv(J7, "wthr.precip", "clear", "GetWeatherPrecipitationModel rObj", True)]
    + [_jpv(J7, f"wthr.tex.{l}", "clear", f"GetWeatherTexture rObj {l}", True) for l in range(0, 5)]
    + [_jp(J7, "wthr.settex.0", _form("rObj", JP["clear"]) + 'let fRes := SetWeatherTexture rObj 0 "nvf\\probe.dds"\n'
           'let sText := GetWeatherTexture rObj 0\nSetWeatherTexture rObj 0 "sky\\alpha.dds"\n'
           'SetFunctionValue (sv_Construct "%g [%z]" fRes sText)', True),
       _jp(J7, "wthr.override", "let fA := SetWeatherTransitionTimeOverride 5\nlet fB := SetWeatherTransitionTimeOverride 0\n"
           'SetFunctionValue (sv_Construct "%g %g" fA fB)', True)]
    + [_jpv(J7, "clmt.count", "climate", "GetClimateNumWeatherTypes rObj")]
    + [_jp(J7, "clmt.addremove", _form("rObj", JP["climate"]) + _form("rOther", JP["hazy"])
           + "let fA := ClimateAddWeatherType rObj rOther 50\nlet fB := GetClimateNumWeatherTypes rObj\n"
           "let fC := ClimateAddWeatherType rObj rOther 70\nlet fSave := GetClimateNumWeatherTypes rObj\n"
           "let fRes := ClimateRemoveWeatherType rObj rOther\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g %g %g %g" fA fB fC fSave (GetClimateNthWeatherChance rObj (fSave - 1)) fRes '
           "(GetClimateNumWeatherTypes rObj))", True),
       _jp(J7, "clmt.remove.absent", _form("rObj", JP["climate"]) + _form("rOther", JP["hazy"])
           + "SetFunctionValue (ClimateRemoveWeatherType rObj rOther)"),
       _jp(J7, "clmt.add.chance", _form("rObj", JP["climate"]) + _form("rOther", JP["hazy"])
           + "let fA := ClimateAddWeatherType rObj rOther 150\nlet fB := GetClimateNumWeatherTypes rObj\n"
           "let fC := GetClimateNthWeatherChance rObj (fB - 1)\nClimateRemoveWeatherType rObj rOther\n"
           'SetFunctionValue (sv_Construct "%g %g %g" fA fB fC)', True)]
    # ---- armour
    + [_jpv(J7, f"armor.class.{k}", k, "GetArmorClass rObj") for k in ("leather",)]
    + [_jpset(J7, f"armor.setclass.{v}", "leather", "GetArmorClass rObj", f"SetArmorClass rObj {v}", "GetArmorClass rObj",
              "SetArmorClass rObj fSave") for v in (3, 2, 0, 4)]
    + [_jp(J7, "armor.setrct", _form("rObj", JP["leather"]) + 'let fRes := SetArmorRCT rObj "nvf\\probe.rct"\n'
           'SetFunctionValue (sv_Construct "%g [%z]" fRes (GetArmorRCT rObj))', True),
       _jp(J7, "armor.condpenalty", "let fA := SetArmorConditionPenalty\nlet fB := SetArmorConditionPenalty 1.5\n"
           "let fC := SetArmorConditionPenalty 3\nlet fRes := SetArmorConditionPenalty -1\nSetArmorConditionPenalty 1\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g %g" fA fB fC fRes (SetArmorConditionPenalty))', True),
       _jp(J7, "armor.maxpercent", "let fA := SetDamageToArmorMaxPercent\nlet fB := SetDamageToArmorMaxPercent 0.5\n"
           "let fC := SetDamageToArmorMaxPercent 2\nSetDamageToArmorMaxPercent fA\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g" fA fB fC (SetDamageToArmorMaxPercent))', True)]
)


# jip6b and jip7b: a second round (2026-11-08) on the rows jip6/jip7 found to differ: the
# rule behind out-of-range values, the default decal data of an impact without DODT
# against RipperImpactOrganic's own (min/max 8/24, colour (63, 63, 63)), and what a turned-on
# model not in the list, an added stage on a form without destruction data and the caps leave.
JP.update({"ripper": "177874"})
J6B, J7B = "jip6b", "jip7b"
JIP6B_PROBES = (
    [_jpset(J6B, f"dest.hp.{n}", "fusion", "GetNthDestructionStageTrait rObj 1 0", f"SetNthDestructionStageTrait rObj 1 0 {v}",
            "GetNthDestructionStageTrait rObj 1 0", "SetNthDestructionStageTrait rObj 1 0 fSave")
     for n, v in (("0", 0), ("100", 100), ("101", 101), ("120", 120), ("200", 200), ("255", 255), ("256", 256),
                  ("m1", -1), ("m5", -5), ("m100", -100), ("1000", 1000))]
    + [_jpset(J6B, f"dest.stage.{n}", "fusion", "GetNthDestructionStageTrait rObj 1 2", f"SetNthDestructionStageTrait rObj 1 2 {v}",
              "GetNthDestructionStageTrait rObj 1 2", "SetNthDestructionStageTrait rObj 1 2 fSave")
       for n, v in (("2", 2), ("3", 3), ("4", 4), ("10", 10), ("255", 255), ("256", 256), ("300", 300), ("m1", -1))]
    + [_jpset(J6B, f"dest.selfdmg.{n}", "fusion", "GetNthDestructionStageTrait rObj 1 1", f"SetNthDestructionStageTrait rObj 1 1 {v}",
              "GetNthDestructionStageTrait rObj 1 1", "SetNthDestructionStageTrait rObj 1 1 fSave")
       for n, v in (("7", 7), ("m2", -2), ("70000", 70000))]
    + [_jpset(J6B, f"dest.debris.{n}", "fusion", "GetNthDestructionStageTrait rObj 1 4", f"SetNthDestructionStageTrait rObj 1 4 {v}",
              "GetNthDestructionStageTrait rObj 1 4", "SetNthDestructionStageTrait rObj 1 4 fSave")
       for n, v in (("300", 300), ("m1", -1))]
    + [_jp(J6B, "dest.add.none", _form("rObj", JP["ammo"]) + "let fRes := AddDestructionStage rObj 0 50 1\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g" fRes (GetDestructionDataNumStages rObj) '
           "(GetNthDestructionStageTrait rObj 0 0) (GetDestructionDataTargetable rObj))", True),
       _jp(J6B, "dest.add.end", _form("rObj", JP["fusion"]) + "let fA := GetDestructionDataNumStages rObj\n"
           "let fRes := AddDestructionStage rObj fA 10 3\nlet fB := GetDestructionDataNumStages rObj\n"
           "let fC := RemoveDestructionStage rObj fA\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g %g" fA fRes fB fC (GetDestructionDataNumStages rObj))', True),
       _jp(J6B, "dest.add.0", _form("rObj", JP["fusion"]) + "let fRes := AddDestructionStage rObj 0 99 0\n"
           "let fB := GetNthDestructionStageTrait rObj 0 0\nlet fC := RemoveDestructionStage rObj 0\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g" fRes fB fC (GetNthDestructionStageTrait rObj 0 0))', True)]
    + [_jpset(J6B, f"cr.dmg.{n}", "brahmin", "GetCreatureDamage rObj", f"SetCreatureDamage {v} rObj",
              "GetCreatureDamage rObj", "SetCreatureDamage fSave rObj")
       for n, v in (("32767", 32767), ("65535", 65535), ("65536", 65536), ("70000", 70000), ("m1", -1))]
    + [_jp(J6B, "cr.toggle.add", _form("rObj", JP["mutant"]) + 'let fRes := ToggleCreatureModel "brahmin.nif" 1 rObj\n'
           'let fA := ar_Size (GetCreatureModels rObj)\nlet fB := CreatureHasModel "brahmin.nif" rObj\n'
           'let fC := ToggleCreatureModel "brahmin.nif" 0 rObj\n'
           'SetFunctionValue (sv_Construct "%g %g %g %g %g" fRes fA fB fC (ar_Size (GetCreatureModels rObj)))', True),
       _jp(J6B, "cr.toggle.offagain", _form("rObj", JP["mutant"]) + 'let fA := ToggleCreatureModel "smspinebreakerheavy.nif" 0 rObj\n'
           'let fB := ToggleCreatureModel "smspinebreakerheavy.nif" 0 rObj\nlet fC := ar_Size (GetCreatureModels rObj)\n'
           'let fRes := ToggleCreatureModel "smspinebreakerheavy.nif" 1 rObj\n'
           'SetFunctionValue (sv_Construct "%g %g %g %g" fA fB fC fRes)', True),
       _jp(J6B, "msg.crafting.twice", "let fA := ToggleCraftingMessages 1\nlet fB := ToggleCraftingMessages 0\n"
           "let fC := ToggleCraftingMessages 0\nlet fRes := ToggleCraftingMessages 1\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g" fA fB fC fRes)', True),
       _jp(J6B, "dest.setexpl.result", _form("rObj", JP["fusion"]) + _form("rOther", JP["stageexpl"])
           + "let fRes := SetNthDestructionStageExplosion rObj 2 rOther\nSetFunctionValue fRes"),
       _jp(J6B, "dest.settarget.result", _form("rObj", JP["fusion"])
           + "let fRes := SetDestructionDataTargetable rObj 1\nSetFunctionValue fRes")]
)
JIP7B_PROBES = (
    [_jpset(J7B, f"ammo.clip.{n}", "ammo", "GetAmmoTraitNumeric rObj 3", f"SetAmmoTraitNumeric rObj 3 {v}",
            "GetAmmoTraitNumeric rObj 3", "SetAmmoTraitNumeric rObj 3 fSave") for n, v in (("255", 255), ("256", 256), ("m1", -1), ("511", 511))]
    + [_jpset(J7B, f"ammo.pps.{n}", "ammo", "GetAmmoTraitNumeric rObj 1", f"SetAmmoTraitNumeric rObj 1 {v}",
              "GetAmmoTraitNumeric rObj 1", "SetAmmoTraitNumeric rObj 1 fSave") for n, v in (("1.5", 1.5), ("m2.5", -2.5))]
    + [_jpset(J7B, f"amef.{t}.{n}", "amef", f"GetAmmoEffectTraitNumeric rObj {t}", f"SetAmmoEffectTraitNumeric rObj {t} {v}",
              f"GetAmmoEffectTraitNumeric rObj {t}", f"SetAmmoEffectTraitNumeric rObj {t} fSave")
       for t, n, v in ((0, "5", 5), (0, "6", 6), (0, "m1", -1), (1, "2", 2), (1, "3", 3), (2, "m4", -4))]
    + [_jpset(J7B, f"proj.{t}.{n}", "proj", f"GetProjectileTraitNumeric rObj {t}", f"SetProjectileTraitNumeric rObj {t} {v}",
              f"GetProjectileTraitNumeric rObj {t}", f"SetProjectileTraitNumeric rObj {t} fSave")
       for t, n, v in ((0, "16", 16), (0, "17", 17), (0, "0", 0), (0, "4", 4), (0, "3", 3), (14, "3", 3), (14, "m1", -1), (14, "0", 0))]
    + [_jpset(J7B, f"expl.7.{n}", "expl", "GetExplosionTraitNumeric rObj 7", f"SetExplosionTraitNumeric rObj 7 {v}",
              "GetExplosionTraitNumeric rObj 7", "SetExplosionTraitNumeric rObj 7 fSave") for n, v in (("m1", -1), ("3", 3), ("300", 300))]
    + [_jp(J7B, f"ipct.colour.{k}", _form("rObj", JP[k]) + 'SetFunctionValue (sv_Construct "%.0f" (GetImpactDataTrait rObj 17))', True)
       for k in ("ipct", "ripper")]
    + [_jpv(J7B, f"ipct.ripper.{t}", "ripper", f"GetImpactDataTrait rObj {t}") for t in (8, 9, 10, 11, 12, 13, 14, 15, 16)]
    + [_jpset(J7B, f"ipct.flags.{n}", "ipct", "GetImpactDataTrait rObj 16", f"SetImpactDataTraitNumeric rObj 13 {v}",
              "GetImpactDataTrait rObj 16", "SetImpactDataTraitNumeric rObj 13 fSave") for n, v in (("16", 16), ("255", 255), ("256", 256), ("m1", -1))]
    + [_jp(J7B, f"ipct.setcolour.{n}", _form("rObj", JP["ripper"]) + "let fSave := GetImpactDataTrait rObj 17\n"
           f"let fRes := SetImpactDataTraitNumeric rObj 14 {v}\nlet fA := GetImpactDataTrait rObj 17\n"
           "SetImpactDataTraitNumeric rObj 14 fSave\n"
           'SetFunctionValue (sv_Construct "%.0f %g %.0f %.0f" fSave fRes fA (GetImpactDataTrait rObj 17))', True)
       for n, v in (("255255255", 255255255), ("255255256", 255255256), ("300", 300), ("m1", -1), ("16777215", 16777215))]
    + [_jp(J7B, "ipct.setform.sound", _form("rObj", JP["ipct"]) + _form("rOther", "5FD18")
           + "let fRes := SetImpactDataTraitForm rObj 2 rOther\nlet rItem := GetImpactDataTrait rObj 7\n"
           "SetImpactDataTraitForm rObj 2\n"
           'SetFunctionValue (sv_Construct "%g %z %z" fRes (GetFormIDString rItem) (GetFormIDString (GetImpactDataTrait rObj 7)))', True)]
    + [_jp(J7B, "armor.maxpercent.range", "let fA := SetDamageToArmorMaxPercent 2\nlet fB := SetDamageToArmorMaxPercent -1\n"
           "let fC := SetDamageToArmorMaxPercent\nSetDamageToArmorMaxPercent 1\n"
           'SetFunctionValue (sv_Construct "%g %g %g %g" fA fB fC (SetDamageToArmorMaxPercent))', True),
       _jp(J7B, "armor.condpenalty.neg", "let fA := SetArmorConditionPenalty -1\nlet fB := SetArmorConditionPenalty\n"
           "SetArmorConditionPenalty 1\n"
           'SetFunctionValue (sv_Construct "%g %g %g" fA fB (SetArmorConditionPenalty))', True)]
)


SETS = {"jip-getters": PROBES, "jip-forms": FORM_PROBES, "jip-stores": STORE_PROBES, "jip-events": EVENT_PROBES,
        "jip-hits": HIT_PROBES, "jip-records": RECORD_PROBES, "nvse-forms": NVSE_FORM_PROBES, "nvse-vars": VAR_PROBES,
        "nvse-refs": REF_PROBES, "nvse-controls": CONTROL_PROBES, "nvse-actors": ACTOR_PROBES,
        "nvse-setters": SETTER_PROBES, "nvse-misc": MISC_PROBES, "nvse-math": MATH_PROBES,
        "nvse-data": DATA_PROBES, "nvse-inv": INV_PROBES, "nvse-second": SECOND_PROBES,
        "nvse-keynames": KEYNAME_PROBES, "nvse-third": THIRD_PROBES, "nvse-fourth": FOURTH_PROBES,
        "jip-fifth": FIFTH_PROBES, "nvse-fifth": NVSE_FIFTH_PROBES, "nvse-sixth": SIXTH_PROBES,
        "nvse-seventh": SEVENTH_PROBES,
        "nvse-eighth": EIGHTH_PROBES,
        "lu-leveled": LEVELED_PROBES,
        "nvse-flags": FLAG_PROBES,
        "ini-files": INI_FILE_PROBES,
        "jip-n7": N7_PROBES,
        "player-state": PLAYER_STATE_PROBES,
        "mcm": MCM_PROBES,
        "plugin-odds": N9_PROBES,
        "lu-leveled2": LEVELED2_PROBES,
        "projectile": PROJECTILE_PROBES,
        "sky-sound": SKY_SOUND_PROBES,
        "knvse-anim": KNVSE_PROBES,
        "knvse2": KNVSE2_PROBES,
        "jipppln": JIPPPLN_PROBES,
        "nif-blocks": NIF_PROBES,
        "nvse-events": NVSE_EVENT_PROBES,
        "jip-events2": JIP_EVENT2_PROBES,
        "nvse-leftovers": N13_PROBES,
        "save-set": SAVE_SET_PROBES,
        "save-read": SAVE_READ_PROBES,
        "rgb-ui": RGB_UI_PROBES,
        "gaps1": GAPS1_PROBES,
        "gaps2": GAPS2_PROBES,
        "gaps3": GAPS3_PROBES,
        "gaps4": GAPS4_PROBES,
        "gaps5": GAPS5_PROBES,
        "gaps6": GAPS6_PROBES,
        "gaps7": GAPS7_PROBES,
        "gaps8": GAPS8_PROBES,
        "gaps9": GAPS9_PROBES,
        "gaps10": GAPS10_PROBES,
        "gaps11": GAPS11_PROBES,
        "plugins1": PLUGINS1_PROBES,
        "plugins2": PLUGINS2_PROBES,
        "plugins3": PLUGINS3_PROBES,
        "engine1": ENGINE1_PROBES,
        "engine2": ENGINE2_PROBES,
        "engine3": ENGINE3_PROBES,
        "glta-order": GLTA_ORDER_PROBES,
        "data1": DATA1_PROBES,
        "data2": DATA2_PROBES,
        "data3": DATA3_PROBES,
        "anim1": ANIM1_PROBES,
        "anim2": ANIM2_PROBES,
        "reload1": RELOAD1_PROBES,
        "sound1": SOUND1_PROBES,
        "proj1": PROJ1_PROBES,
        "controls1": CONTROLS1_PROBES,
        "hud1": HUD1_PROBES,
        "showoff1": SHOWOFF1_PROBES,
        "atk1": ATK1_PROBES,
        "jip4": JIP4_PROBES,
        "rules1": RULES1_PROBES,
        "rules2": RULES2_PROBES,
        "showoff2": SHOWOFF2_PROBES,
        "lu2": LU2_PROBES,
        "showoff5": SHOWOFF5_PROBES,
        "showoff4": SHOWOFF4_PROBES,
        "tmpl1": TMPL1_PROBES,
        "jg1": JG1_PROBES,
        "jg2": JG2_PROBES,
        "jg5": JG5_PROBES,
        "jg6": JG6_PROBES,
        "one111": ONE111_PROBES,
        "jip5": JIP5_PROBES,
        "jip8": JIP8_PROBES,
        "jip9": JIP9_PROBES,
        "pp": PP_PROBES,
        "format1": FORMAT1_PROBES,
        "tmpl2": TMPL2_PROBES,
        "tmpl3": TMPL3_PROBES,
        "itr": ITR_PROBES,
        "sup2": SUP2_PROBES,
        "sup3": SUP3_PROBES,
        "sup6": SUP6_PROBES,
        "sup5": SUP5_PROBES}
# Sets whose probes write (into the session only, see STORE_PROBES, EVENT_PROBES, HIT_PROBES
# and VAR_PROBES).
WRITING_SETS = {"jip-stores", "jip-events", "jip-hits", "nvse-vars", "nvse-setters", "nvse-data", "nvse-inv",
                "nvse-second", "nvse-fourth", "jip-fifth", "nvse-fifth", "nvse-sixth", "nvse-seventh", "nvse-eighth", "lu-leveled", "nvse-flags", "ini-files", "jip-n7", "player-state", "mcm", "plugin-odds", "lu-leveled2", "projectile", "sky-sound", "knvse-anim", "nif-blocks", "nvse-events", "jip-events2", "nvse-leftovers", "save-set", "save-read", "rgb-ui", "gaps1", "gaps2", "gaps3", "gaps4", "gaps5", "gaps6", "gaps7", "gaps8", "gaps9", "gaps10", "gaps11", "plugins1", "plugins2", "plugins3", "engine1", "engine2", "engine3", "glta-order", "data1", "data2", "data3", "anim1", "anim2", "reload1", "sound1", "proj1", "controls1", "hud1", "showoff1", "atk1", "jip4", "rules1", "rules2", "showoff2", "jg1", "jg2", "jg5", "jip5", "jip8", "tmpl3", "knvse2", "itr", "one111", "pp", "jipppln", "sup2", "sup3", "showoff4", "sup5", "jip9", "jg6", "showoff5", "sup6", "lu2"}
SETS.update({"lutana1": LUTANA1_PROBES})
WRITING_SETS |= {"lutana1"}
SETS.update({"jg3": JG3_PROBES, "jg4": JG4_PROBES})
WRITING_SETS |= {"jg3", "jg4"}
SETS.update({"sup1": SUP1_PROBES, "showoff3": SHOWOFF3_PROBES})
WRITING_SETS |= {"showoff3"}
SETS.update({"jip6": JIP6_PROBES, "jip7": JIP7_PROBES, "jip6b": JIP6B_PROBES, "jip7b": JIP7B_PROBES})  # JIP 6/7 probes (#1006)
WRITING_SETS |= {"jip6", "jip7", "jip6b", "jip7b"}


# lu3 (writes, session only, each change put back; #1009): Lutana batch 3
# (docs/spec-notes/ext-lutana.md §10): the activator prompt, the Anim Object idle and the
# loading screen type's colour on the pages' example forms, perk ranks and perk entries on
# the player, the regions and cell references at Doc Mitchell's, the deprecated
# SetVariableFloat and the event names called as functions. Every value set is read
# back, then set back to what was read.
_LU3_HEAD = "ref rP\nref rObj\narray_var aR\nstring_var sOld\nfloat fA\nfloat fB\nfloat fC\nfloat fD\nfloat fE\nfloat fF\nBegin Function { }\n"


def _lu3(pid, body, text=True):
    return (f"lu3.{pid}", _LU3_HEAD + body, text, None, "")


LU3_PROBES = (
    # The prompt of the page's example table, read only: the setter with $sOld to put it
    # back did not compile in the original [observed 2026-11-09], and a literal would
    # guess the value (hypothesis: the record's XATO, else "").
    [_lu3("prompt", "let sOld := GetActivatorPrompt BlackjackTableGomorrah\nSetFunctionValue (sv_Construct \"[%z]\" sOld)"),
     # The page's Anim Object's idle (hypothesis: its ANIO DATA), its Editor ID.
     _lu3("anio", "let rObj := GetAnimObjectIdle RepairWall01Wrench\nSetFunctionValue (GetFormIDString rObj)"),
     # The page's example: r 255, g 180, b 53 [documented]; set and read, set back. The
     # elements go into floats first: as sv_Construct's arguments they did not compile in
     # the original [observed 2026-11-09].
     _lu3("lsct", "let aR := GetLoadScreenTypeTextRGB StandardLoadingTip\nlet fA := aR[\"r\"]\nlet fB := aR[\"g\"]\nlet fC := aR[\"b\"]\n"
          "SetLoadScreenTypeTextRGB StandardLoadingTip 59.3 181 300\nlet aR := GetLoadScreenTypeTextRGB StandardLoadingTip\n"
          "let fD := aR[\"r\"]\nlet fE := aR[\"g\"]\nlet fF := aR[\"b\"]\nSetLoadScreenTypeTextRGB StandardLoadingTip fA fB fC\n"
          "SetFunctionValue (sv_Construct \"%g %g %g / %.1f %g %g\" fA fB fC fD fE fF)"),
     # A rank above the perk's ranks, then 0: "0 1 1 1" [observed 2026-11-09], cut to its one
     # rank, 0 changes nothing. Educated is put back as it was (SetPerkRank 0 does not
     # take it away: a player without it keeps it for the session).
     _lu3("perkrank", "let fA := PlayerREF.GetPerkRank Educated\nPlayerREF.SetPerkRank Educated 9\nlet fB := PlayerREF.GetPerkRank Educated\n"
          "PlayerREF.SetPerkRank Educated 0\nlet fC := PlayerREF.GetPerkRank Educated\nPlayerREF.SetPerkRank Educated fA\n"
          "SetFunctionValue (sv_Construct \"%g %g %g %g\" fA fB fC (PlayerREF.GetPerkRank Educated))"),
     # The page's example: Fast Shot's 2nd entry from Calculate Hit Chance (8) to Calculate
     # Weapon Range (7), read back, set back: "8 7 8" [observed 2026-11-09].
     _lu3("perkentry", "let fA := GetNthPerkEntryFunction FastShot 1\nSetNthPerkEntryFunction FastShot 1 7\nlet fB := GetNthPerkEntryFunction FastShot 1\n"
          "SetNthPerkEntryFunction FastShot 1 fA\nSetFunctionValue (sv_Construct \"%g %g %g\" fA fB (GetNthPerkEntryFunction FastShot 1))"),
     # Indoors: no regions (hypothesis: an empty array, 0).
     _lu3("regions", "let aR := GetPlayerRegions\nSetFunctionValue (ar_Size aR)", False),
     # GetCellRefs against GetRefs in the player's cell, every type (hypothesis: equal);
     # the cell given, the page lists it as not optional (without it the call did not
     # compile in the original [observed 2026-11-09]).
     _lu3("cellrefs", "let rObj := PlayerREF.GetParentCell\nlet aR := GetCellRefs 0 0 0 rObj\nlet fA := ar_Size aR\nlet aR := GetRefs 0 0 0\n"
          "SetFunctionValue (sv_Construct \"%g\" (fA == (ar_Size aR)))"),
     # SetVariableFloat on a FalloutNV.esm quest's variable, nGeckosKilled of 10A214 (the
     # eighth set's has.vanilla row; NVFExtProbe.esp is not active in the probe game), its
     # result and the value read back, set back (hypothesis: "0 7").
     _lu3("setvar", _form("rObj", "10A214") + "let fA := GetVariable \"nGeckosKilled\" rObj\nlet fB := SetVariableFloat \"nGeckosKilled\" 7 rObj\n"
          "let fC := GetVariable \"nGeckosKilled\" rObj\nSetVariable \"nGeckosKilled\" fA rObj\nSetFunctionValue (sv_Construct \"%g %g\" fB fC)"),
     # The event names called as functions: "2 3" [observed 2026-11-09].
     _lu3("events", "SetFunctionValue (sv_Construct \"%g %g\" (OnCellEnter) (OnCellExit))")]
)
SETS.update({"lu3": LU3_PROBES})
WRITING_SETS |= {"lu3"}
# jg7 (writes, session only, each change put back; #1007): JohnnyGuitar batch 7
# (docs/spec-notes/ext-jg7.md): the music, level-up, yield, viewmodel clip and LOD switches
# read back, barter hiding on the caps (F) with Doc Mitchell (104C0F) as the vendor, the
# reputation change icon on RepNVGoodsprings (104C22), the Goodsprings map marker's icon
# (CDF51), GetJohnnyPatch, the heap size, IsMenuPaused, the load
# screen switch and RefAddr [observed: forms from fnvtool esm inventory, as FX above].
# ToggleDisableSaves, the queued texts, the camera and HUD shakes, TogglePipBoy,
# ExitGameAlt and RollCredits change the screen or the game and are not probed.
JG7 = {"caps": "F", "pistol10": "434F", "docref": "104C0F", "repgs": "104C22", "gsmarker": "CDF51"}


def _jg7(pid, body, text=False):
    return (f"jg7.{pid}", "ref rP\nref rObj\nref rOther\nfloat fRes\nfloat fSave\nfloat fA\nfloat fB\nfloat fC\n"
            "float fH\nfloat fS\nfloat fV\nstring_var sText\nBegin Function { }\n"
            + _form("rP", NF["player"]) + body, text, None, "")


JG7_PROBES = (
    # 1. switches read back and put back
    [_jg7("music", "let fA := (IsCombatMusicEnabled)\nToggleCombatMusic 0\nlet fB := (IsCombatMusicEnabled)\nToggleCombatMusic 1\n"
          'SetFunctionValue (sv_Construct "%g %g %g" fA fB (IsCombatMusicEnabled))', True),
     _jg7("levelup", "let fA := (IsLevelUpMenuEnabled)\nToggleLevelUpMenu 0\nlet fB := (IsLevelUpMenuEnabled)\nToggleLevelUpMenu 1\n"
          'SetFunctionValue (sv_Construct "%g %g %g" fA fB (IsLevelUpMenuEnabled))', True),
     _jg7("yield", "let fA := (GetYieldTimer)\nSetYieldTimer 5\nlet fB := (GetYieldTimer)\nSetYieldTimer 0\n"
          'SetFunctionValue (sv_Construct "%g %g %g" fA fB (GetYieldTimer))', True),
     _jg7("clip", "let fSave := (GetViewmodelClipDistance)\nSetViewmodelClipDistance 2.5\nlet fA := (GetViewmodelClipDistance)\n"
          "SetViewmodelClipDistance 0\nlet fB := (GetViewmodelClipDistance)\nSetViewmodelClipDistance fSave\n"
          'SetFunctionValue (sv_Construct "%g %g %g" fSave fA fB)', True),
     _jg7("lod", "let fSave := (GetNiLODLevel)\nSetNiLODLevel 2\nlet fA := (GetNiLODLevel)\nSetNiLODLevel fSave\n"
          'SetFunctionValue (sv_Construct "%g %g %g" fSave fA (GetNiLODLevel))', True),
     # 2. barter hiding flags: player side kept (1), both hidden (none), shown again (0); a
     # vendor's own entry read with and without the vendor; the caps (all 0 in the first
     # run) and the 10mm pistol (434F)
     *[_jg7(pid, _form("rObj", JG7[key]) + _form("rOther", JG7["docref"])
          + "let fA := IsItemBarterHiddenEx rObj\nHideItemBarterEx rObj 1 1\nlet fB := IsItemBarterHiddenEx rObj\n"
          "HideItemBarterEx rObj 1\nlet fC := IsItemBarterHiddenEx rObj\nHideItemBarterEx rObj 0\n"
          'let sText := sv_Construct "%g %g %g %g" fA fB fC (IsItemBarterHiddenEx rObj)\n'
          "HideItemBarterEx rObj 1 2 rOther\nlet fA := IsItemBarterHiddenEx rObj rOther\nlet fB := IsItemBarterHiddenEx rObj\n"
          'HideItemBarterEx rObj 0 0 rOther\nSetFunctionValue (sv_Construct "%z %g %g %g" sText fA fB (IsItemBarterHiddenEx rObj rOther))', True)
       for pid, key in (("barter", "caps"), ("barter.pistol", "pistol10"))],
     # 3. the reputation icon's return for tier 2 and tier 5 (session only), the map
     # marker's icon before any SetCustomMapMarkerIcon
     _jg7("repicon", _form("rObj", JG7["repgs"])
          + 'let fA := SetCustomReputationChangeIcon rObj 2 "interface\\icons\\nvf.dds"\n'
          + 'SetFunctionValue (sv_Construct "%g %g" fA (SetCustomReputationChangeIcon rObj 5 "interface\\icons\\nvf.dds"))', True),
     _jg7("markericon", _form("rObj", JG7["gsmarker"]) + "SetFunctionValue (rObj.GetCustomMapMarkerIcon)", True),
     # 4. HSVtoRGB and RGBtoHSV compiled at run time in none of seven forms in the original
     # (observer run 2026-11-08), so they are not in this set (docs/spec-notes/ext-jg7.md)
     # 5. the extender's own files, menus, the load screen switch, RefAddr
     _jg7("patches", 'SetFunctionValue (sv_Construct "%g %g %g %g %g %g %g %g" (GetJohnnyPatch 1) (GetJohnnyPatch 2) '
          "(GetJohnnyPatch 3) (GetJohnnyPatch 4) (GetJohnnyPatch 5) (GetJohnnyPatch 6) (GetJohnnyPatch 7) (GetJohnnyPatch 8))", True),
     _jg7("heap", "SetFunctionValue (GetDefaultHeapSize)"),
     _jg7("menupaused", 'SetFunctionValue (sv_Construct "%g %g %g" (IsMenuPaused) (IsMenuPaused 1002) (IsMenuPaused 1009))', True),
     _jg7("lsonly", "SetFunctionValue (GetLocationSpecificLoadScreensOnly)"),
     _jg7("refaddr", "SetFunctionValue ((rP.RefAddr) != 0)")]
)
SETS.update({"jg7": JG7_PROBES})  # JohnnyGuitar batch 7 (#1007)
WRITING_SETS |= {"jg7"}




# jip10 (writes, session only, each change put back; #1006): JIP batch 10
# (docs/spec-notes/jip-scripts-10.md) on FalloutNV.esm forms: VMS01 (Ain't That a Kick in
# the Head, done in the probe save), MS05's script for added variables (MS05 is not
# started by script here; the quest's script instance takes the variable), Doc Mitchell
# (factions, linked children), the player (perks), and a text file written and read
# under the game's folder (NVForever writes into its own layer). The questions: which
# elements GetQuests, GetQuestObjectives and GetActiveObjectives give (forms or indices),
# AddScriptVariable's result for a name added twice, SetStageAlt's GetStageDone of a
# higher stage, ReadStringFromFile's line cuts, FakeScriptEvent's result.
J10 = {"vms01": "104C1C", "doc": "104C0C", "docref": "104C0F", "ms05": "1023A5"}


def _j10(pid, body, text=False):
    return (f"jip10.{pid}", "ref rP\nref rObj\nref rItem\nfloat fRes\nfloat fA\nfloat fB\nstring_var sText\narray_var aList\n"
            "Begin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


def _j10arr(pid, setup, expr):
    return _j10(pid, setup + f"let aList := {expr}\nlet sText := \"\"\nlet fA := ar_Size aList\nif fA > 0\n"
                "let sText := sv_Construct \"%g %z\" fA (ToString aList[0])\nelse\nlet sText := sv_Construct \"%g\" fA\nendif\n"
                "SetFunctionValue sText", True)


JIP10_PROBES = (
    [_j10arr("quests.active", "", "GetQuests"),
     _j10arr("quests.done", "", "GetQuests 1"),
     _j10arr("objectives.active", "", "GetActiveObjectives"),
     _j10arr("objectives.vms01.0", _form("rObj", J10["vms01"]), "GetQuestObjectives rObj 0"),
     _j10arr("objectives.vms01.1", _form("rObj", J10["vms01"]), "GetQuestObjectives rObj 1"),
     _j10arr("targets.vms01.10", _form("rObj", J10["vms01"]), "GetObjectiveTargets rObj 10")]
    + [_j10arr("perks.player", "", "rP.GetAllPerks"),
       _j10arr("perks.player.hidden", "", "rP.GetAllPerks 0 1"),
       _j10arr("perks.player.mates", "", "rP.GetAllPerks 1"),
       _j10arr("factions.docref", _form("rObj", J10["docref"]), "rObj.GetFactions"),
       _j10arr("factions.doc", _form("rObj", J10["doc"]), "GetFactions rObj"),
       _j10arr("linked.docref", _form("rObj", J10["docref"]), "rObj.GetLinkedChildren"),
       _j10arr("enable.docref", _form("rObj", J10["docref"]), "rObj.GetEnableChildren")]
    + [_j10("vars.add", _form("rObj", J10["docref"])
            + 'let fRes := rObj.AddScriptVariable "nvfProbeVar"\nlet fA := rObj.AddScriptVariable "nvfProbeVar"\n'
            'rObj.SetVariable "nvfProbeVar" 2.5\nlet fB := rObj.GetVariable "nvfProbeVar"\n'
            'let sText := sv_Construct "%g %g %g %g" fRes fA fB (rObj.HasVariableAdded "nvfProbeVar")\n'
            'rObj.RemoveScriptVariable "nvfProbeVar"\n'
            'SetFunctionValue (sv_Construct "%z %g" sText (rObj.HasVariableAdded "nvfProbeVar"))', True)]
    + [_j10("events.disabled", _form("rObj", J10["docref"])
            + 'let fA := GetScriptEventDisabled rObj "OnHit"\nSetScriptEventDisabled rObj "OnHit OnLoad" 1\n'
            'let fB := GetScriptEventDisabled rObj "onload"\nSetScriptEventDisabled rObj "OnHit OnLoad" 0\n'
            'SetFunctionValue (sv_Construct "%g %g %g" fA fB (GetScriptEventDisabled rObj "OnHit"))', True),
       _j10("fake.onadd", _form("rObj", J10["docref"]) + 'SetFunctionValue (rObj.FakeScriptEvent "OnTriggerEnter")'),
       _j10("fake.onactivate", _form("rObj", J10["docref"]) + 'SetFunctionValue (rObj.FakeScriptEvent "OnActivate")')]
    + [_j10("extra.data", "rP.SetRefExtraData 2 -903.5\nrP.SetRefExtraData 19 \"Girly\"\nrP.SetRefExtraData 18 7\n"
            "let fA := rP.GetRefExtraData 2\nlet sText := rP.GetRefExtraData 19\nlet fB := rP.RefHasExtraData\n"
            "let fRes := rP.RefHasExtraData 200\nrP.SetRefExtraData\n"
            'SetFunctionValue (sv_Construct "%g [%z] %g %g %g" fA sText fB fRes (rP.RefHasExtraData))', True),
       _j10("mapmarker", _form("rObj", J10["docref"]) + "rObj.AddRefMapMarker\nlet fA := rObj.RefHasMapMarker\n"
            "rObj.RemoveRefMapMarker\n" 'SetFunctionValue (sv_Construct "%g %g" fA (rObj.RefHasMapMarker))', True)]
    + [_j10("file.roundtrip", 'let fRes := WriteStringToFile "nvf_probe_jip10.txt" 0 "one%rtwo%rthree"\n'
            'let fA := WriteStringToFile "nvf_probe_jip10.txt" 1 "%rfour"\n'
            'let sText := ReadStringFromFile "nvf_probe_jip10.txt" 2 2\n'
            'SetFunctionValue (sv_Construct "%g %g [%z] [%z]" fRes fA sText (ReadStringFromFile "nvf_probe_jip10.txt"))', True),
       _j10("file.missing", 'SetFunctionValue (ReadStringFromFile "nvf_probe_no_such_file.txt")', True)]
)


SETS.update({"jip10": JIP10_PROBES})  # JIP batch 10 (#1006)
WRITING_SETS |= {"jip10"}


# sup7 (writes, session only; #1008): SUP batch 7 (docs/spec-notes/ext-sup7.md): the NIF
# block interpolations on the player's third-person "Bip01 Head" (which type number is
# which, the searches' defaults, the trait, the finish event and what a stopped block
# keeps), the vertices of a node and a missing block, and the HUD bars' script-variable
# and UDF sources (their results only: no getter reads a bar's value). Writes restored:
# every interpolation stopped, the head's scale set back to 1, the handler removed, the
# bar destroyed. One step per frame.
def _sup7(pid, body, text=True):
    return (f"sup7.{pid}", "float fA\nfloat fB\nfloat fC\nfloat fD\nref rH\narray_var aR\nBegin Function { }\n" + body, text, None, "")


SUP7_PROBES = (
    # A scale from 1 to 2 over 2 s: its result and the searches. "1 1 0 0 0 0" [observed
    # 2026-11-08]: type 2 is not the scale, no arguments finds none.
    [_sup7("scale.start", "let fA := PlayerREF.NifBlockInterpolateScale \"Bip01 Head\" 1 2 2 0 0 0\n"
           "SetFunctionValue (sv_Construct \"%g %g %g %g %g %g\" fA (PlayerREF.IsNifBlockInterpolated \"Bip01 Head\") "
           "(PlayerREF.IsNifBlockInterpolated \"Bip01 Head\" 2) (PlayerREF.IsNifBlockInterpolated \"Bip01 Head\" 1) "
           "(PlayerREF.IsNifBlockInterpolated \"Bip01 Head\" -1 1) (PlayerREF.IsNifBlockInterpolated))"),
     # A frame on: the scale between the ends; Stop of types 1 and 2 (neither the scale's),
     # the search after, the scale. "0 -1 -1 1 0" [observed 2026-11-08]: a stop that finds
     # none gives -1; the scale did not read above 1 a frame on.
     _sup7("scale.next", "let fA := PlayerREF.GetNifBlockScale \"Bip01 Head\"\nlet fB := PlayerREF.NifBlockInterpolateStop \"Bip01 Head\" 1\n"
           "let fC := PlayerREF.NifBlockInterpolateStop \"Bip01 Head\" 2\n"
           "SetFunctionValue (sv_Construct \"%g %g %g %g %g\" ((fA > 1) && (fA < 2)) fB fC (PlayerREF.IsNifBlockInterpolated \"Bip01 Head\") "
           "((PlayerREF.GetNifBlockScale \"Bip01 Head\") > 1))"),
     _sup7("scale.kept", "SetFunctionValue (sv_Construct \"%g\" ((PlayerREF.GetNifBlockScale \"Bip01 Head\") > 1))\nPlayerREF.SetNifBlockScale \"Bip01 Head\" 1"),
     # Which type is the translation and the rotation [observed 2026-11-08: the scale is
     # type 0, a rotation found by type 1]: each started from the block as it is to the
     # same place (calcStart, no visible change), the three types searched as digits (type
     # 0 hundreds, 1 tens, 2 units), then Stop without arguments and the block searched.
     # Array elements go into floats first: as a command's arguments they do not compile
     # [observed]. "101 0 1" and "111 0 1" [observed 2026-11-08, second run]: the scale of
     # scale.start still runs, the translation is type 2, a bare Stop stops nothing.
     _sup7("types.translation", "let aR := PlayerREF.GetNifBlockTranslation \"Bip01 Head\"\nlet fA := aR[0]\nlet fB := aR[1]\nlet fC := aR[2]\n"
           "PlayerREF.NifBlockInterpolateTranslation \"Bip01 Head\" 0 0 0 fA fB fC 5 0 0 0 1\n"
           "let fD := (PlayerREF.IsNifBlockInterpolated \"Bip01 Head\" 0) * 100 + (PlayerREF.IsNifBlockInterpolated \"Bip01 Head\" 1) * 10 + (PlayerREF.IsNifBlockInterpolated \"Bip01 Head\" 2)\n"
           "let fA := PlayerREF.NifBlockInterpolateStop\n"
           "SetFunctionValue (sv_Construct \"%g %g %g\" fD fA (PlayerREF.IsNifBlockInterpolated \"Bip01 Head\"))"),
     _sup7("types.rotation", "let aR := PlayerREF.GetNifBlockRotation \"Bip01 Head\"\nlet fA := aR[0]\nlet fB := aR[1]\nlet fC := aR[2]\n"
           "PlayerREF.NifBlockInterpolateRotation \"Bip01 Head\" 0 0 0 fA fB fC 5 0 0 1\n"
           "let fD := (PlayerREF.IsNifBlockInterpolated \"Bip01 Head\" 0) * 100 + (PlayerREF.IsNifBlockInterpolated \"Bip01 Head\" 1) * 10 + (PlayerREF.IsNifBlockInterpolated \"Bip01 Head\" 2)\n"
           "let fA := PlayerREF.NifBlockInterpolateStop\n"
           "SetFunctionValue (sv_Construct \"%g %g %g\" fD fA (PlayerREF.IsNifBlockInterpolated \"Bip01 Head\"))"),
     # Stop on the block (the earlier steps' interpolations still running), the trait with
     # none; the trait on a running one, trait 0 and an unknown trait 5: "1 0 0 0"
     # [observed 2026-11-08, both runs].
     _sup7("trait", "let fA := PlayerREF.NifBlockInterpolateStop \"Bip01 Head\"\nlet fB := PlayerREF.NifBlockInterpolateSetTrait 0 1\n"
           "PlayerREF.NifBlockInterpolateScale \"Bip01 Head\" 1 1 5 0 0 0\n"
           "let fC := PlayerREF.NifBlockInterpolateSetTrait 0 1 \"Bip01 Head\"\n"
           "SetFunctionValue (sv_Construct \"%g %g %g %g\" fA fB fC (PlayerREF.NifBlockInterpolateSetTrait 5 1 \"Bip01 Head\"))\n"
           "PlayerREF.NifBlockInterpolateStop"),
     # The finish event: a handler writes its arguments into aux variables; a scale of
     # 1 to 1 over a thousandth of a second ends by the next frame.
     _sup7("event.set", "PlayerREF.AuxVarErase \"*sup7\"\nlet rH := (Begin Function {ref rRef, string_var sBlock, int iType, int iLoop, int iPC}\n"
           "PlayerREF.AuxVarSetFlt \"*sup7\" ((rRef == PlayerREF) * 1000 + (sBlock == \"Bip01 Head\") * 100 + iType * 10 + iLoop) 0\n"
           "PlayerREF.AuxVarSetFlt \"*sup7\" iPC 1\nEnd)\nSetSUPOnInterpolateFinishEvent rH\nPlayerREF.AuxVarSetRef \"*sup7h\" rH\n"
           "SetFunctionValue (PlayerREF.NifBlockInterpolateScale \"Bip01 Head\" 1 1 0.001 0 0 0)", False),
     _sup7("event.wait", "SetFunctionValue (PlayerREF.IsNifBlockInterpolated)", False),
     # "1100 1 0" [observed 2026-11-08]: the player, the block, type 0 (the scale), not
     # from a loop; iPCNode 1 on the player's third-person model; then the handler removed.
     _sup7("event.read", "SetFunctionValue (sv_Construct \"%g %g %g\" (PlayerREF.AuxVarGetFlt \"*sup7\" 0) (PlayerREF.AuxVarGetFlt \"*sup7\" 1) "
           "(PlayerREF.IsNifBlockInterpolated))\nlet rH := PlayerREF.AuxVarGetRef \"*sup7h\"\nRemoveSUPOnInterpolateFinishEvent rH\n"
           "PlayerREF.AuxVarErase \"*sup7\"\nPlayerREF.AuxVarErase \"*sup7h\"\nPlayerREF.SetNifBlockScale \"Bip01 Head\" 1"),
     # Vertices of a node and of a missing block: "-2 -1 0 0" [observed 2026-11-08].
     _sup7("verts", "let aR := PlayerREF.NifBlockGetVerticles \"Bip01 Head\"\nlet fA := ar_Size aR\nlet aR := PlayerREF.NifBlockGetVerticles \"NVFNoSuchBlock\"\n"
           "SetFunctionValue (sv_Construct \"%g %g %g %g\" (PlayerREF.GetNifBlockNumVerticles \"Bip01 Head\") "
           "(PlayerREF.GetNifBlockNumVerticles \"NVFNoSuchBlock\") fA (ar_Size aR))"),
     # The bars' UDF source on a made bar and on a missing one (hypothesis: "1 -1"; the
     # first 1 [observed 2026-11-08]); then the bar destroyed. HudBarSetValueScriptVar did
     # not compile at run time in any form tried [observed], so it is not probed here.
     _sup7("bars", "HudBarCreate \"NVFProbeBar\" 100 100 0\n"
           "let fA := HudBarSetValueUDF \"NVFProbeBar\" (Begin Function { }\nSetFunctionValue 0.5\nEnd) 0\n"
           "let fB := HudBarSetValueUDF \"NVFNoSuchBar\" (Begin Function { }\nSetFunctionValue 0.5\nEnd) 0\n"
           "HudBarDestroy \"NVFProbeBar\"\nSetFunctionValue (sv_Construct \"%g %g\" fA fB)")]
)
SETS.update({"sup7": SUP7_PROBES})
WRITING_SETS |= {"sup7"}


# jip11 (reads; the one write, ForceActorDetectionValue, is put back; #1006): JIP batch 11
# (docs/spec-notes/jip-actors-11.md) on Doc Mitchell (reference and base) and the player
# in the probe save. The questions: GetActorLightAmount's range on an actor indoors,
# GetActorMovementFlags of a standing actor, GetDetectionValue's value for a pair (and one
# never measured), GetActiveEffects' elements (the magic items or their base effects),
# GetBaseActorValueAlt on a base form, GetKillXP, GetRadiationLevelAlt on an NPC, the
# stable and interrupt packages, list getters out of combat (no array?).
def _j11(pid, body, text=False):
    return (f"jip11.{pid}", "ref rP\nref rObj\nfloat fA\nfloat fB\nstring_var sText\narray_var aList\n"
            "Begin Function { }\n" + _form("rP", NF["player"]) + body, text, None, "")


def _j11v(pid, setup, expr, text=False):
    return _j11(pid, setup + f"SetFunctionValue ({expr})", text)


def _j11arr(pid, setup, expr):
    return _j11(pid, setup + f"let aList := {expr}\nlet fA := ar_Size aList\nif fA > 0\n"
                "let sText := sv_Construct \"%g %z\" fA (ToString aList[0])\nelse\nlet sText := sv_Construct \"%g\" fA\nendif\n"
                "SetFunctionValue sText", True)


_J11DOC = _form("rObj", NF["docref"])
JIP11_PROBES = (
    [_j11v("alpha.doc", _J11DOC, "rObj.GetActorAlpha"),
     _j11v("gravity.doc", _J11DOC, "rObj.GetActorGravityMult"),
     _j11v("leveled.doc", _J11DOC, "GetFormIDString (rObj.GetActorLeveledList)", True),
     _j11v("light.doc", _J11DOC, "rObj.GetActorLightAmount"),
     _j11v("light.player", "", "rP.GetActorLightAmount"),
     _j11v("movement.doc", _J11DOC, "rObj.GetActorMovementFlags"),
     _j11v("killxp.doc", _J11DOC, "rObj.GetKillXP"),
     _j11v("rads.doc", _J11DOC, "rObj.GetRadiationLevelAlt"),
     _j11v("rads.player", "", "rP.GetRadiationLevelAlt"),
     _j11v("baseav.doc", _J11DOC, "rObj.GetBaseActorValueAlt Strength"),
     _j11v("baseav.docbase", _form("rObj", NF["doc"]), "GetBaseActorValueAlt Strength rObj"),
     _j11v("detection.doc.player", _J11DOC, "rObj.GetDetectionValue rP"),
     _j11v("detection.player.doc", _J11DOC, "rP.GetDetectionValue rObj"),
     _j11v("stable.doc", _J11DOC, "GetFormIDString (rObj.GetCurrentStablePackage)", True),
     _j11v("interrupt.doc", _J11DOC, "GetFormIDString (rObj.GetInterruptPackage)", True),
     _j11v("unreachable.doc", _J11DOC, "rObj.GetTargetUnreachable"),
     _j11v("unarmed.docbase", _form("rObj", NF["doc"]), "GetFormIDString (GetActorUnarmedEffect rObj)", True),
     _j11v("scoped.player", "", "rP.IsEquippedWeaponScoped"),
     _j11v("silenced.player", "", "rP.IsEquippedWeaponSilenced"),
     _j11v("breath.player", "", "rP.GetActorDiveBreath"),
     _j11v("falltime.player", "", "rP.GetFallTimeRemaining")]
    + [_j11arr("allies.doc", _J11DOC, "rObj.GetCombatAllies"),
       _j11arr("targets.doc", _J11DOC, "rObj.GetCombatTargets"),
       _j11arr("detected.doc", _J11DOC, "rObj.GetDetectedActors"),
       _j11arr("effects.player", "", "rP.GetActiveEffects"),
       _j11arr("effects.player.actor", "", "rP.GetActiveEffects 2"),
       _j11arr("effects.player.ingestible", "", "rP.GetActiveEffects 4")]
    + [_j11("force.detection", _J11DOC + "rP.ForceActorDetectionValue -100\nlet fA := rObj.GetDetectionValue rP\n"
            "rP.ForceActorDetectionValue\nSetFunctionValue (sv_Construct \"%g %g\" fA (rObj.GetDetectionValue rP))", True)]
)

SETS.update({"jip11": JIP11_PROBES})  # JIP batch 11 (#1006)
WRITING_SETS |= {"jip11"}

# sup4 (writes, session only, each change put back; #1008): SUP batch 4
# (docs/spec-notes/ext-sup4.md): random array keys, invalid references, floats from an
# array, a UDF frames later, the last save and load, the cells and map markers near the
# player at Doc Mitchell's, the scripts marked on load, the caught-pickpocketing flag on
# Doc Mitchell (set back), transliteration. One step per frame.
# The string variable is sOut: `string_var sA` did not compile in the original
# [observed 2026-11-09, sup4 diagnostic runs; other names, sOld, did].
def _sup4(pid, body, text=True):
    return (f"sup4.{pid}", "ref rD\nref rNone\nref rH\narray_var aR\narray_var aIn\nfloat fA\nfloat fB\nfloat fC\nstring_var sOut\nBegin Function { }\n"
            + _form("rD", NF["docref"]) + body, text, None, "")


SUP4_PROBES = (
    # A key of a five-element list in 0-4, a key of a two-key map, an empty list's
    # (hypothesis: "1 1 -1").
    [_sup4("randomkey", "let aR := Ar_List 10, 20, 30, 40, 50\nlet fA := Ar_GetRandomKey aR\n"
           "let aIn := Ar_Map \"LALALA\"::1, \"TROLOLO\"::2\nlet sOut := Ar_GetRandomKeyMap aIn\nlet aR := Ar_List\n"
           "SetFunctionValue (sv_Construct \"%g %g %g\" ((fA >= 0) && (fA <= 4) && (fA == Floor fA)) ((sOut == \"LALALA\") || (sOut == \"TROLOLO\")) (Ar_GetRandomKey aR))"),
     # Valid references, an empty one, the empty one inside an inner array searched with
     # and without its dimension (hypothesis: "0 1 0 1").
     _sup4("invalidrefs", "let aR := Ar_List PlayerREF, rD\nlet fA := Ar_HasInvalidRefs aR\nlet aR := Ar_List PlayerREF, rNone\nlet fB := Ar_HasInvalidRefs aR\n"
           "let aIn := Ar_List rNone\nlet aR := Ar_List PlayerREF, aIn\n"
           "SetFunctionValue (sv_Construct \"%g %g %g %g\" fA fB (Ar_HasInvalidRefs aR) (Ar_HasInvalidRefs aR 1))"),
     # The page's example: 3 8 5 [documented].
     _sup4("setfloats", "let aR := Ar_List 3, 8, 5, 1\nAr_SetFloatsFrom aR fA fB fC\nSetFunctionValue (sv_Construct \"%g %g %g\" fA fB fC)"),
     # A UDF two frames later writes an aux variable; read at once and two steps later
     # (hypothesis: "0", then "1").
     _sup4("nextframe.set", "PlayerREF.AuxVarErase \"*sup4\"\nCallFunctionNextFrame (Begin Function { }\nPlayerREF.AuxVarSetFlt \"*sup4\" 1\nEnd) 2\n"
           "SetFunctionValue (PlayerREF.AuxVarGetFlt \"*sup4\")", False),
     _sup4("nextframe.wait", "SetFunctionValue 0", False),
     _sup4("nextframe.read", "SetFunctionValue (PlayerREF.AuxVarGetFlt \"*sup4\")\nPlayerREF.AuxVarErase \"*sup4\"", False),
     # The probe game loads a save before the steps: its size, the co-save's, its name
     # given, the last save's name before any save (hypothesis: "1 1 1 0"; NVForever
     # writes no co-save and gives 0 for type 1 [decision]).
     _sup4("saves", "let sOut := GetSaveName 1\nSetFunctionValue (sv_Construct \"%g %g %g %g\" ((GetLoadedSaveSize 0) > 0) ((GetLoadedSaveSize 1) > 0) "
           "((Sv_Length sOut) > 0) ((Sv_Length (GetSaveName 0)) > 0))"),
     # Indoors: the player's cell alone at depth 0, 2 and -1, no map markers (hypothesis:
     # "1 1 1 0").
     _sup4("near", "let aR := GetNearCells 0\nlet fA := Ar_Size aR\nlet aR := GetNearCells 2\nlet fB := Ar_Size aR\nlet aR := GetNearCells -1\nlet fC := Ar_Size aR\n"
           "let aR := GetNearMapMarkers 2\nSetFunctionValue (sv_Construct \"%g %g %g %g\" fA fB fC (Ar_Size aR))"),
     # A probe query is no script form: nothing marked (hypothesis: "0 0"; the original may
     # refuse the call outside a script).
     _sup4("markonload", "let fA := MarkScriptOnLoad\nSetFunctionValue (sv_Construct \"%g %g\" fA (IsScriptMarkedOnLoad GSDocMitchellScript))"),
     # Doc Mitchell's flag read, set, read, set back (hypothesis: "0 1 1").
     _sup4("caught", "let fA := rD.SetCaughtPCPickpocketting\nlet fB := rD.SetCaughtPCPickpocketting 1\nlet fC := rD.SetCaughtPCPickpocketting\n"
           "rD.SetCaughtPCPickpocketting fA\nSetFunctionValue (sv_Construct \"%g %g %g\" fA fB fC)"),
     # The page names no return value: the text back (hypothesis: "Privet"), or nothing.
     # The literal is "Привет" as cp1251 bytes, written with the cp1252 characters of those
     # bytes: the probe text reaches the game as cp1252 [observed 2026-11-09: UTF-8 Cyrillic
     # arrived as cp1252 bytes].
     _sup4("translit", "let sOut := TranslitText \"Ïðèâåò\"\nSetFunctionValue sOut")]
)
SETS.update({"sup4": SUP4_PROBES})
WRITING_SETS |= {"sup4"}


# jg8 (reads only, nothing changed; #1007): JohnnyGuitar batch 8 (docs/spec-notes/ext-jg8.md):
# file hashes of Data\Credits.txt, loaded archives, a world map icon's DDS header, the
# system colours, the sleep/wait menu's state outside it, Ar_SortEditor on Stimpak
# (15169), Caps001 (F) and the 10mm pistol (434F), Ar_Shuffle [observed: forms from
# fnvtool esm inventory, as NF above]. The event handlers, SetCustomMapMarker,
# ClearCustomMapMarker and TuneRadioRef change the session or need an event the probe
# cannot cause and put back; IsRadioRefPlaying crashes the original (its page).


# The original compiles neither `$var` for a literal nor array elements as call arguments
# [observed: probe set jg8, 2026-11-09], so the icon is spelled out and the sorted forms go
# through ref variables.
JG8_ICON = r"interface\icons\world map\icon_map_encampment.dds"


def _jg8_firsts():
    return "let rA := aOut[0]\nlet rB := aOut[1]\nlet rC := aOut[2]\n"


def _jg8(pid, body, text=False):
    return (f"jg8.{pid}", "ref rA\nref rB\nref rC\nfloat fA\nfloat fB\nfloat fC\nint iR\nint iG\nint iB\n"
            "string_var sText\narray_var aList\narray_var aOut\nBegin Function { }\n" + body, text, None, "")


JG8_PROBES = (
    [_jg8("md5", 'SetFunctionValue (sv_Construct "%z|%z" (MD5File "Data\\Credits.txt") (MD5File "Data\\NoSuchFile.txt"))', True),
     _jg8("sha1", 'SetFunctionValue (SHA1File "Data\\Credits.txt")', True),
     _jg8("bsa", 'SetFunctionValue (sv_Construct "%g %g %g %g" (IsBSALoaded "Fallout - Meshes.bsa") '
          '(IsBSALoaded "fallout - meshes.bsa") (IsBSALoaded "Data\\Fallout - Meshes.bsa") (IsBSALoaded "NoSuch.bsa"))', True),
     _jg8("texture", 'SetFunctionValue (sv_Construct "%g %g %g %g" ' + " ".join(
          f'({f} "{JG8_ICON}")' for f in ("GetTextureWidth", "GetTextureHeight", "GetTextureMipMapCount", "GetTextureFormat")) + ")", True),
     _jg8("colour", 'SetFunctionValue (sv_Construct "%.0f %.0f %.0f %.0f %.0f %.0f" (GetSystemColor 1) (GetSystemColor 2) '
          "(GetSystemColor 3) (GetSystemColor 4) (GetSystemColor 5) (GetSystemColor 6))", True),
     _jg8("colour.alt", 'let fA := GetSystemColorAlt 1 iR iG iB\nSetFunctionValue (sv_Construct "%g %g %g" iR iG iB)', True),
     _jg8("sleepwait", "SetFunctionValue (GetSleepWaitMenuState)"),
     _jg8("sorteditor", _form("rA", NF["stimpak"]) + _form("rB", NF["caps"]) + _form("rC", NF["pistol10"])
          + "let aList := Ar_List rA, rB, rC\nlet aOut := Ar_SortEditor aList\n"
          + _jg8_firsts()
          + 'let sText := sv_Construct "%z %z %z" (GetFormIDString rA) (GetFormIDString rB) (GetFormIDString rC)\n'
          "let aOut := Ar_SortEditor aList 1\n"
          + _jg8_firsts()
          + 'SetFunctionValue (sv_Construct "%z | %z %z %z" sText (GetFormIDString rA) (GetFormIDString rB) (GetFormIDString rC))', True),
     _jg8("shuffle", "let aOut := Ar_Shuffle (Ar_List 1, 2, 3, 4, 5)\n"
          "let fA := Ar_Size aOut\nlet fB := aOut[0] + aOut[1] + aOut[2] + aOut[3] + aOut[4]\n"
          'SetFunctionValue (sv_Construct "%g %g" fA fB)', True)]
)
SETS.update({"jg8": JG8_PROBES})  # JohnnyGuitar batch 8 (#1007)


def script(body):
    if body.startswith(("ref ", "float ")):
        return body + "\nEnd\n"
    return "Begin Function { }\n" + body + "\nEnd\n"


STEPS_DIR = os.path.join(HERE, "jip-probes")


def steps_text(set_name):
    """The job file of a set as committed in STEPS_DIR (the lab's candidate,
    `fnvtool ext queries`, runs these)."""
    return json.dumps(job(set_name), indent=1) + "\n"


def job_name(set_name):
    return set_name.replace("-", "_")


def job(set_name="jip-getters"):
    steps = [{"query": script(b), "label": pid, "field": "value", **({"text": True} if text else {})}
             for pid, b, text, _, _ in SETS[set_name]]
    return {"format": cs.STEPS_FORMAT, "name": job_name(set_name),
            "title": f"{set_name}, runtime-compiled" + ("" if set_name in WRITING_SETS else " (read-only)"),
            "start": "any save (read-only)", "steps": steps}


def _fmt(v):
    if isinstance(v, float):
        return f"{v:.6g}"
    return "" if v is None else str(v)


def collect(out_jsonl, set_name="jip-getters"):
    rows, errors = [], []
    with open(out_jsonl, encoding="utf-8") as f:
        for line in f:
            r = json.loads(line)
            if r.get("kind") != "result":
                continue
            if "error" in r:
                errors.append({"id": r["label"], "error": r["error"]})
            else:
                rows.append({"id": r["label"], "value": _fmt(r.get("value"))})
    return {"format": ep.FORMAT, "set": set_name, "version": "JIP 57.30, xNVSE 6.4.9", "sources": [os.path.basename(out_jsonl)],
            "rows": rows, "files": [], "duplicates": [], "errors": errors}


def expected(set_name="jip-getters"):
    return {pid: exp for pid, _, _, exp, _ in SETS[set_name]}


def compare(golden):
    return ep.compare(golden, expected(golden.get("set", "jip-getters")))


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    j = sub.add_parser("job")
    j.add_argument("--set", default="jip-getters", choices=sorted(SETS))
    j.add_argument("-o", "--out")
    c = sub.add_parser("collect")
    c.add_argument("output")
    c.add_argument("--set", default="jip-getters", choices=sorted(SETS))
    c.add_argument("-o", "--out")
    k = sub.add_parser("compare")
    k.add_argument("golden")
    st = sub.add_parser("steps")
    st.add_argument("--dir", default=STEPS_DIR)
    a = ap.parse_args(argv)
    if a.cmd == "steps":
        os.makedirs(a.dir, exist_ok=True)
        for name in SETS:
            with open(os.path.join(a.dir, f"{job_name(name)}.steps.json"), "w", encoding="utf-8", newline="\n") as f:
                f.write(steps_text(name))
        print(json.dumps({"dir": a.dir, "sets": list(SETS)}))
        return 0
    if a.cmd == "job":
        a.out = a.out or f"{job_name(a.set)}.steps.json"
        with open(a.out, "w", encoding="utf-8", newline="\n") as f:
            json.dump(job(a.set), f, indent=1)
            f.write("\n")
        print(a.out)
        return 0
    if a.cmd == "collect":
        doc = collect(a.output, a.set)
        out = a.out or os.path.join(str(ep.default_goldens()), "jip", f"{a.set}.json")
        os.makedirs(os.path.dirname(out), exist_ok=True)
        with open(out, "w", encoding="utf-8") as f:
            json.dump(doc, f, indent=1, ensure_ascii=False)
            f.write("\n")
        print(json.dumps({"out": out, "rows": len(doc["rows"]), "errors": len(doc["errors"])}))
        return 0
    with open(a.golden, encoding="utf-8") as f:
        res = compare(json.load(f))
    print(json.dumps(res, indent=1))
    return 1 if res["mismatches"] or res["missing"] else 0


if __name__ == "__main__":
    sys.exit(main())
