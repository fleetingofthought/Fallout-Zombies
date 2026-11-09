//! The compiler without game data: source → bytes → the decoder's IR
//! (docs/spec-notes/script-compiler.md). The parity over the shipped units is the lab
//! scenario L5.script.compile.vanilla.

use fnv_script::compile::{compile, Compiled, Env, FormInfo, Seed, PLAYER};
use fnv_script::unit::RefSlot;
use fnv_script::{decode, Arg, Expr, FunctionTable, Op, Program, Stmt, Token, VarDecl, VarType};

fn table() -> FunctionTable {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    FunctionTable::load(&p).expect("functions.json loads")
}

const QUEST: u32 = 0x0100_1000;
const CAPS: u32 = 0x0000_000F;
const GLOBAL: u32 = 0x0100_2000;
const DOOR: u32 = 0x0100_3000;

struct World;

impl Env for World {
    fn form(&self, editor_id: &str) -> Option<FormInfo> {
        let (form, sig) = match editor_id.to_ascii_lowercase().as_str() {
            "testquest" => (QUEST, *b"QUST"),
            "caps001" => (CAPS, *b"MISC"),
            "testglobal" => (GLOBAL, *b"GLOB"),
            "testdoorref" => (DOOR, *b"REFR"),
            _ => return None,
        };
        Some(FormInfo { form, sig })
    }
    fn info(&self, form: u32) -> Option<FormInfo> {
        [("testquest", QUEST), ("caps001", CAPS), ("testglobal", GLOBAL), ("testdoorref", DOOR)]
            .iter()
            .find(|(_, f)| *f == form)
            .and_then(|(n, _)| self.form(n))
    }
    fn editor_id(&self, form: u32) -> Option<String> {
        [("TestQuest", QUEST), ("Caps001", CAPS), ("TestGlobal", GLOBAL), ("TestDoorREF", DOOR)]
            .iter()
            .find(|(_, f)| *f == form)
            .map(|(n, _)| n.to_string())
    }
    fn local_form(&self, local: u32) -> Option<u32> {
        Some(local)
    }
    fn script_vars(&self, form: u32) -> Option<Vec<VarDecl>> {
        (form == QUEST).then(|| {
            vec![
                VarDecl { index: 1, name: "bDone".into(), ty: VarType::Int },
                VarDecl { index: 2, name: "fTimer".into(), ty: VarType::FloatOrRef },
            ]
        })
    }
}

fn build(src: &str) -> (Compiled, Program) {
    let t = table();
    let c = compile(src, &t, &World, None);
    let errors: Vec<_> = c.errors().collect();
    assert!(errors.is_empty(), "compile errors: {errors:?}");
    let d = decode(&c.bytes, &t);
    assert!(d.report.issues.is_empty(), "decoder issues: {:?}", d.report.issues);
    assert_eq!(d.report.raw_total(), 0);
    (c, d.program)
}

fn body(p: &Program) -> &[Stmt] {
    &p.blocks[0].body
}

fn ops(e: &Expr) -> Vec<String> {
    e.0.iter()
        .map(|t| match t {
            Token::Num { text, .. } => text.clone(),
            Token::Op { op } => op.symbol().to_string(),
            Token::Var { .. } => "var".into(),
            Token::Ref { .. } => "ref".into(),
            Token::Call { call } => format!("call{:04X}", call.opcode),
            Token::X { .. } => "x".into(),
            Token::Raw { .. } => "raw".into(),
        })
        .collect()
}

#[test]
fn a_small_script_round_trips_through_the_decoder() {
    let (c, p) = build(
        "scn TestScript\nshort a\nref r\n\nbegin GameMode\n  set a to 1 + 2 * 3\n  if a == 7 && GetStage TestQuest >= 10\n    player.AddItem Caps001 5\n  elseif a\n    r.Activate player 1\n  else\n    set TestQuest.bDone to 1\n  endif\nend\n",
    );
    assert!(p.script_name);
    assert_eq!(p.blocks.len(), 1);
    assert_eq!(p.blocks[0].params, None, "GameMode has no parameter block");
    let Stmt::Set { expr, .. } = &body(&p)[0] else { panic!("set first") };
    assert_eq!(ops(expr), ["1", "2", "3", "*", "+"]);
    let Stmt::If { arms, otherwise } = &body(&p)[1] else { panic!("if second") };
    assert_eq!(arms.len(), 2);
    assert!(otherwise.is_some());
    assert_eq!(c.variables.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["a", "r"]);
    // The quest (GetStage), the player, the caps, the local `r`, in order of first use.
    assert_eq!(c.refs, [RefSlot::Form(QUEST), RefSlot::Form(PLAYER), RefSlot::Form(CAPS), RefSlot::Local(2)]);
}

#[test]
fn or_binds_tighter_than_and_and_minus_is_unary() {
    let (_, p) = build("short a\nshort b\nshort c\nbegin GameMode\nif a && b || c\nset a to -1\nendif\nend");
    let Stmt::If { arms, .. } = &body(&p)[0] else { panic!() };
    assert_eq!(ops(&arms[0].cond), ["var", "var", "var", "||", "&&"]);
    let Stmt::Set { expr, .. } = &arms[0].body[0] else { panic!() };
    assert_eq!(ops(expr), ["1", "~"]);
    assert!(matches!(expr.0[1], Token::Op { op: Op::Neg }));
}

#[test]
fn parameter_types_decide_how_values_are_stored() {
    let (c, p) = build(
        "ref r\nfloat f\nbegin GameMode\nSetQuestDelay TestQuest 3\nset f to GetGlobalValue TestGlobal\nset f to TestGlobal\nplayer.GetDistance r\nend",
    );
    let b = body(&p);
    // A number in a float parameter is a float, even written without a point.
    let Stmt::Call { call } = &b[0] else { panic!() };
    assert_eq!(call.args[1], Arg::Float(3.0));
    // A global in a form parameter is the form; in an expression it is the value.
    let Stmt::Set { expr, .. } = &b[1] else { panic!() };
    let Token::Call { call } = &expr.0[0] else { panic!() };
    assert!(matches!(call.args[0], Arg::Ref(_)));
    let Stmt::Set { expr, .. } = &b[2] else { panic!() };
    assert!(matches!(expr.0[0], Token::Var { .. }));
    // A `ref` local as a parameter is its reference list entry.
    let Stmt::Call { call } = &b[3] else { panic!() };
    let Arg::Ref(r) = call.args[0] else { panic!("ref local passed as a reference: {:?}", call.args) };
    assert_eq!(c.refs[r.0 as usize - 1], RefSlot::Local(1));
}

#[test]
fn player_spellings_get_their_own_entries() {
    let (c, _) = build("begin GameMode\nplayer.Disable\nPlayerREF.Enable\nplayer.Enable\nend");
    assert_eq!(c.refs, [RefSlot::Form(PLAYER), RefSlot::Form(PLAYER)]);
}

#[test]
fn surplus_arguments_are_dropped() {
    let (_, p) = build("begin GameMode\nStopCombat player\nend");
    let Stmt::Call { call } = &body(&p)[0] else { panic!() };
    assert!(!call.has_params, "StopCombat takes no parameters: no parameter block");
}

#[test]
fn a_seed_keeps_the_units_indices() {
    let t = table();
    let vars = vec![VarDecl { index: 7, name: "a".into(), ty: VarType::Int }];
    let refs = vec![RefSlot::Form(CAPS)];
    let c = compile("short a\nbegin GameMode\nset a to 1\nplayer.AddItem Caps001 a\nend", &t, &World, Some(Seed { variables: &vars, refs: &refs, unknown_calls: false }));
    assert_eq!(c.variables, vars);
    assert_eq!(c.refs, [RefSlot::Form(CAPS), RefSlot::Form(PLAYER)]);
    assert!(c.diags.iter().any(|d| d.kind == "reference_added"));
    let p = decode(&c.bytes, &t).program;
    let Stmt::Call { call } = &body(&p)[1] else { panic!() };
    assert_eq!(call.on.map(|r| r.0), Some(2));
    assert_eq!(call.args[0], Arg::Ref(fnv_script::RefIndex(1)));
}

#[test]
fn unknown_names_are_errors() {
    let t = table();
    let c = compile("begin GameMode\nNoSuchFunction 1\nset x to 2\nend", &t, &World, None);
    let kinds: Vec<&str> = c.errors().map(|d| d.kind).collect();
    assert_eq!(kinds, ["unknown_function", "bad_set_target"]);
    assert_eq!(c.errors().next().map(|d| d.line), Some(2));
}

#[test]
fn begin_blocks_with_parameters() {
    let (c, p) = build("begin OnTrigger player\nend\nbegin OnLoad\nend");
    assert_eq!(p.blocks[0].params.as_ref().map(Vec::len), Some(1));
    assert_eq!(p.blocks[1].params, None);
    assert_eq!(c.refs, [RefSlot::Form(PLAYER)]);
}

// ---- the extender's expressions (task A7) --------------------------------------

use fnv_script::ir::{FuncId, VarSlot, XAssign, XExpr, XOp};
use fnv_script::vm::{ArgValue, Host, RunOpts, Value, Vm};

/// The tree as text, every operation in parentheses.
fn x(e: &XExpr) -> String {
    match e {
        XExpr::Num { text, .. } => text.clone(),
        XExpr::Str { s } => format!("{s:?}"),
        XExpr::Var { var, .. } => match var.var {
            VarSlot::Local { index, .. } => format!("v{index}"),
            VarSlot::Global { global } => format!("g{}", global.0),
        },
        XExpr::Ref { r } => format!("r{}", r.0),
        XExpr::Enum { code } => format!("#{code}"),
        XExpr::Axis { c } => c.to_string(),
        XExpr::Call { call } => {
            let f = match &call.func {
                FuncId::Opcode(o) => format!("{o:04X}"),
                FuncId::Name(n) => n.clone(),
            };
            let on = call.on.as_ref().map(|o| format!("{}.", x(o))).unwrap_or_default();
            format!("{on}{f}({})", call.args.iter().map(x).collect::<Vec<_>>().join(" "))
        }
        XExpr::Unary { op, e } => format!("({op:?} {})", x(e)),
        XExpr::Binary { op, l, r } => format!("({} {op:?} {})", x(l), x(r)),
        XExpr::Member { l, key } => format!("{}->{key}", x(l)),
        XExpr::Assign { op, target, value } => format!("({} {op:?}= {})", x(target), x(value)),
        XExpr::Lambda { params, body, value, .. } => format!(
            "lambda[{}]({})",
            params.len(),
            value.as_ref().map(|v| x(v)).unwrap_or_else(|| format!("{} statements", body.len()))
        ),
    }
}

fn lets(src: &str) -> Vec<String> {
    let (_, p) = build(src);
    let mut out = Vec::new();
    p.walk(&mut |s| match s {
        Stmt::Let { expr } => out.push(x(expr)),
        Stmt::If { arms, .. } => {
            for a in arms {
                if let [Token::X { e }] = &a.cond.0[..] {
                    out.push(format!("if {}", x(e)));
                }
            }
        }
        _ => {}
    });
    out
}

#[test]
fn extender_precedence_follows_the_operator_table() {
    let l = lets("int a\nint b\nbegin GameMode\nlet a := 1 + 2 * 3 ^ 2\nlet a := b := 2\na += 1\nlet a := 1 || 0 && 2\nlet a := -!b\nend");
    assert_eq!(l[0], "(v1 Set= (1 Add (2 Mul (3 Pow 2))))");
    assert_eq!(l[1], "(v1 Set= (v2 Set= 2))", ":= groups right to left");
    assert_eq!(l[2], "(v1 Add= 1)", "the Let macro");
    assert_eq!(l[3], "(v1 Set= (1 Or (0 And 2)))", "&& binds tighter than ||");
    assert_eq!(l[4], "(v1 Set= (Neg (Not v2)))");
}

#[test]
fn a_functions_argument_is_an_operand() {
    // [documented: Inline Expression] `Player.GetActorValue Health + 50` is
    // (GetAV Health) + 50.
    let l = lets("int a\nbegin GameMode\nif eval GetStage TestQuest >= 10\nendif\nlet a := player.GetAV Luck\nlet a := GetRandomPercent * 2\nlet a := Abs -3 + 1\nend");
    assert_eq!(l[0], "if (103A(r1) Ge 10)");
    assert_eq!(l[3], "(v1 Set= (1221((Neg 3)) Add 1))", "a minus starts the first argument");
    assert!(l[1].starts_with("(v1 Set= r2.100E(#"), "an actor value name is its code: {}", l[1]);
    assert_eq!(l[2], "(v1 Set= (104D() Mul 2))", "a function without parameters takes no operand");
}

#[test]
fn extender_functions_are_named() {
    let t = table();
    let c = compile("int a\nbegin GameMode\nlet a := Ar_Size b\nend", &t, &World, None);
    // An undeclared word is its own text, with a warning [observed: nvse-fourth, the
    // editor's "Unquoted argument"].
    assert!(c.errors().next().is_none(), "{:?}", c.diags);
    assert!(c.diags.iter().any(|d| d.kind == "unquoted_argument" && !d.error), "b is not declared: {:?}", c.diags);
    let c = compile("int a\nint b\nbegin GameMode\nlet a := Ar_Size b\nend", &t, &World, None);
    assert!(c.errors().next().is_none(), "{:?}", c.diags);
    let p = decode(&c.bytes, &t).program;
    let Stmt::Let { expr: XExpr::Assign { op: XAssign::Set, value, .. } } = &body(&p)[0] else { panic!() };
    assert!(matches!(value.as_ref(), XExpr::Call { call } if call.func == FuncId::Name("Ar_Size".into())));
}

#[test]
fn bad_extended_lines_are_errors() {
    let t = table();
    for (src, kind) in [
        ("begin GameMode\nlet 5 := 1\nend", "bad_assignment_target"),
        ("int a\nbegin GameMode\nlet a + 1\nend", "let_without_assignment"),
        ("int a\nbegin GameMode\nlet a := (1 + 2\nend", "syntax"),
        ("int a\nbegin GameMode\nlet a := 1 2\nend", "syntax"),
    ] {
        let c = compile(src, &t, &World, None);
        assert_eq!(c.errors().map(|d| d.kind).collect::<Vec<_>>(), [kind], "{src}");
    }
}

#[derive(Default)]
struct Locals(
    std::collections::BTreeMap<u16, Value>,
    fnv_script::vm::ExtStore,
    std::collections::BTreeSet<u16>,
    std::collections::BTreeSet<u16>,
    Vec<String>,
    Option<u32>,
);

impl Host for Locals {
    fn var_kind(&mut self, var: &fnv_script::VarRef) -> fnv_script::vm::VarKind {
        match var.var {
            VarSlot::Local { index, .. } if self.2.contains(&index) => fnv_script::vm::VarKind::String,
            VarSlot::Local { index, .. } if self.3.contains(&index) => fnv_script::vm::VarKind::Array,
            _ => fnv_script::vm::VarKind::Value,
        }
    }
    fn ext(&mut self) -> Option<&mut fnv_script::vm::ExtStore> {
        Some(&mut self.1)
    }
    fn print(&mut self, text: &str) {
        self.4.push(text.to_string());
    }
    fn get_var(&mut self, var: &fnv_script::VarRef) -> Result<Value, String> {
        let VarSlot::Local { index, .. } = var.var else { return Err("global".into()) };
        Ok(self.0.get(&index).copied().unwrap_or_default())
    }
    fn set_var(&mut self, var: &fnv_script::VarRef, value: Value) -> Result<(), String> {
        let VarSlot::Local { index, .. } = var.var else { return Err("global".into()) };
        self.0.insert(index, value);
        Ok(())
    }
    fn ref_value(&mut self, _r: fnv_script::RefIndex) -> Result<Value, String> {
        Ok(Value::Ref(PLAYER))
    }
    fn call(&mut self, _opcode: u16, _on: Option<Value>, _args: &[ArgValue]) -> Result<Value, String> {
        Ok(Value::Num(4.0))
    }
    fn calling_ref(&mut self) -> Option<u32> {
        self.5
    }
    fn mod_name(&mut self, index: u32) -> Option<String> {
        (index == 0).then(|| "FalloutNV.esm".to_string())
    }
    fn mod_index(&mut self, name: &str) -> Option<u32> {
        name.eq_ignore_ascii_case("FalloutNV.esm").then_some(0)
    }
    fn form_sig(&mut self, form: u32) -> Option<[u8; 4]> {
        (form == 0x14 || form == 0x104C0C).then_some(*b"NPC_")
    }
    fn actor_value_code(&mut self, name: &str) -> Option<u16> {
        name.eq_ignore_ascii_case("Strength").then_some(5)
    }
}

/// A host whose string variables are the compiled unit's `string_var`s.
fn host_for(c: &Compiled) -> Locals {
    let of = |t: &str| c.variables.iter().filter(|v| v.ty == VarType::Other(t.into())).map(|v| v.index as u16).collect();
    Locals(Default::default(), Default::default(), of("string_var"), of("array_var"), Vec::new(), None)
}

fn run(src: &str) -> Locals {
    let t = table();
    let (c, p) = build(src);
    let mut h = host_for(&c);
    Vm::new(&t, &mut h, RunOpts { strict: true, ..RunOpts::default() }).run(&p.blocks[0].body).expect("runs");
    h
}

#[test]
fn let_runs_on_numbers() {
    let h = run("float a\nfloat b\nfloat c\nbegin GameMode\nlet a := 2 + 3 * 4\nlet b := c := 7\nb += 1\nlet c := 0 || 5\nlet a := (1 << 3) | 1\nend");
    assert_eq!(h.0[&1], Value::Num(9.0));
    assert_eq!(h.0[&2], Value::Num(8.0));
    // `||` returns the deciding operand's value [documented: NVSE Expressions].
    assert_eq!(h.0[&3], Value::Num(5.0));
}

#[test]
fn if_eval_tests_an_extended_condition() {
    let h = run("float a\nbegin GameMode\nif eval 2 ^ 3 == 8 && GetRandomPercent\nlet a := 1\nelseif eval 1\nlet a := 2\nendif\nend");
    assert_eq!(h.0[&1], Value::Num(1.0));
}

#[test]
fn not_built_yet_stops_the_statement() {
    let t = table();
    // GetPluginHeaderVersion is not built in the VM (the game answers it).
    let (_, p) = build("ref r\nbegin GameMode\nGetPluginHeaderVersion \"FalloutNV.esm\"\nend");
    let mut h = Locals::default();
    let r = Vm::new(&t, &mut h, RunOpts { strict: true, ..RunOpts::default() }).run(&p.blocks[0].body);
    assert!(matches!(r, Err(fnv_script::vm::VmError::NotBuilt(_))), "{r:?}");
    let _ = XOp::Add;
}

// ---- loops (task A8) ------------------------------------------------------------

#[test]
fn while_loops_run_with_break_and_continue() {
    // Sum 1..=10 skipping 5, stopping after 8: 1+2+3+4+6+7+8 = 31.
    let h = run(
        "float i\nfloat s\nbegin GameMode\nwhile i < 10\n  i += 1\n  if eval i == 5\n    continue\n  endif\n  if eval i > 8\n    break\n  endif\n  s += i\nloop\nend",
    );
    assert_eq!(h.0[&2], Value::Num(31.0));
    assert_eq!(h.0[&1], Value::Num(9.0));
}

#[test]
fn nested_loops_and_their_structure() {
    let (_, p) = build("float i\nfloat j\nfloat n\nbegin GameMode\nwhile i < 3\n  i += 1\n  let j := 0\n  while j < 2\n    j += 1\n    n += 1\n  loop\nloop\nend");
    let Stmt::While { body, .. } = &body(&p)[0] else { panic!("outer while") };
    assert!(matches!(body.last(), Some(Stmt::While { .. })), "the inner loop is in the outer body");
    let h = run("float i\nfloat j\nfloat n\nbegin GameMode\nwhile i < 3\n  i += 1\n  let j := 0\n  while j < 2\n    j += 1\n    n += 1\n  loop\nloop\nend");
    assert_eq!(h.0[&3], Value::Num(6.0));
}

#[test]
fn a_runaway_loop_stops_at_the_step_budget() {
    let t = table();
    let (_, p) = build("float i\nbegin GameMode\nwhile 1\n  i += 1\nloop\nend");
    let mut h = Locals::default();
    let r = Vm::new(&t, &mut h, RunOpts { strict: true, max_steps: 1000 }).run(&p.blocks[0].body);
    assert!(matches!(r, Err(fnv_script::vm::VmError::Budget(1000))), "{r:?}");
}

#[test]
fn foreach_compiles_and_waits_for_arrays() {
    let (_, p) = build("float e\nfloat a\nbegin GameMode\nforeach e <- a\n  break\nloop\nend");
    assert!(matches!(&body(&p)[0], Stmt::ForEach { body, .. } if body == &[Stmt::Break]));
}

#[test]
fn loop_mistakes_are_errors() {
    let t = table();
    for (src, kind) in [
        ("begin GameMode\nloop\nend", "loop_without_while"),
        ("begin GameMode\nbreak\nend", "outside_a_loop"),
        ("float i\nbegin GameMode\nwhile i\nend", "while_without_loop"),
        ("float e\nbegin GameMode\nforeach e a\nloop\nend", "syntax"),
    ] {
        let c = compile(src, &t, &World, None);
        assert_eq!(c.errors().map(|d| d.kind).collect::<Vec<_>>(), [kind], "{src}");
    }
}

// ---- inline expressions, new variable types (tasks A9, A10) -----------------------

#[test]
fn inline_expressions_as_arguments_in_an_extended_unit() {
    // [documented: Inline Expression] `Player.SetActorValue Health (Player.GetActorValue
    // Health + 50)`; the unit is extended (it has a let), so the call line is one too.
    let (_, p) = build("float f\nbegin GameMode\nlet f := 1\nplayer.SetActorValue Health (player.GetActorValue Health + 50)\nend");
    let Stmt::Eval { expr } = &body(&p)[1] else { panic!("an expression statement: {:?}", body(&p)[1]) };
    let s = x(expr);
    assert!(s.starts_with("r1.100F(#"), "{s}");
    assert!(s.contains("(r1.100E(#") && s.ends_with("Add 50))"), "{s}");
}

#[test]
fn a_vanilla_unit_keeps_its_parentheses_dropped() {
    // Without any extender syntax, parentheses among arguments are dropped as the editor
    // drops them (scripts.md §3.4): a plain call statement.
    let (_, p) = build("begin GameMode\nplayer.AddItem (Caps001) 5\nend");
    assert!(matches!(&body(&p)[0], Stmt::Call { .. }));
}

#[test]
fn extender_functions_as_statements_make_the_unit_extended() {
    let t = table();
    let c = compile("array_var a\nbegin GameMode\nAr_Append a 5\nend", &t, &World, None);
    assert!(c.errors().next().is_none(), "{:?}", c.diags);
    let p = decode(&c.bytes, &t).program;
    assert!(matches!(&body(&p)[0], Stmt::Eval { expr: XExpr::Call { call } } if call.func == FuncId::Name("Ar_Append".into())));
    assert_eq!(c.variables[0].ty, VarType::Other("array_var".into()));
}

#[test]
fn inline_declarations_hex_and_binary_and_block_comments() {
    let h = run("begin GameMode\nint i = 0x10 + 0b101\n/* a comment\nint j = 99\n*/\nfloat f = i * 2 ; 42\nend");
    assert_eq!(h.0[&1], Value::Num(21.0));
    assert_eq!(h.0[&2], Value::Num(42.0), "j was inside the comment, so f is variable 2");
}

#[test]
fn inline_declarations_in_let_eval_and_while() {
    // `let int i := 10`, a type in `if eval` and in a `while` condition [documented: Inline
    // variable declaration]; `Let array_var a = …` with the macro's `=` (#1460, the
    // Script Runner files of the owner's mod lists).
    let h = run("begin GameMode\nlet int i := 10\nif eval (int j = i + 1) > 10\nlet float f := j * 2\nendif\nwhile (int k = 0) > 1\nloop\nend");
    assert_eq!(h.0[&1], Value::Num(10.0));
    assert_eq!(h.0[&2], Value::Num(11.0));
    assert_eq!(h.0[&3], Value::Num(22.0));
    let (c, _) = build("begin GameMode\nLet array_var a = ar_List 1, 2\nforeach array_var e <- a\nloop\nend");
    assert!(c.errors().next().is_none(), "{:?}", c.diags);
    assert_eq!(c.variables.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["a", "e"]);
    // A lambda's typed parameters stay parameters.
    let (c, _) = build("begin GameMode\nSetEventHandler \"OnHit\" ({ref rT, int iX} => PrintC \"x\")\nend");
    assert!(c.errors().next().is_none(), "{:?}", c.diags);
}

#[test]
fn a_block_lambdas_body_declares_its_variables() {
    // `ref r = …` inside `(Begin Function {…} … End)`: declared before the body compiles,
    // as the unit's declarations are (#1460, gr_ files of the owner's mod lists).
    let (c, _) = build("begin GameMode\nSetOnControlUpEventHandler (Begin Function {int iCon}\nref rW = Player.GetEquippedObject 5\nint iN = 0\niN = 1\nEnd) 1\nend");
    assert!(c.errors().next().is_none(), "{:?}", c.diags);
    assert!(c.variables.iter().any(|v| v.name == "rW") && c.variables.iter().any(|v| v.name == "iN"));
}

#[test]
fn calls_on_results_chain() {
    let l = lets("ref r\nfloat f\nbegin GameMode\nlet f := (GetSelf).GetAV Luck\nend");
    assert!(l[0].starts_with("(v2 Set= "), "{}", l[0]);
    assert!(l[0].contains(".100E(#"), "a call on the result of GetSelf: {}", l[0]);
}

#[test]
fn compiler_override_blocks_read_every_line_with_the_extender_grammar() {
    // Under `Begin _GameMode` `if` and `set` take extended expressions [documented:
    // Script Compiler Override]; xNVSE 6.4.9 reads `Sin 90 > 0.5` as (Sin 90) > 0.5 there
    // [observed: probe expressions/09], not as the page's `Sin (14 == 1)`.
    let (_, p) = build("float f\nbegin _GameMode\nif Sin 14 == 1\nset f to f + 1\nendif\nend");
    assert_eq!(p.blocks[0].event, 0, "the block is GameMode");
    let Stmt::If { arms, .. } = &body(&p)[0] else { panic!() };
    let [Token::X { e }] = &arms[0].cond.0[..] else { panic!("an extended condition: {:?}", arms[0].cond) };
    assert_eq!(x(e), "(Sin(14) Eq 1)", "Sin is the extender's (no opcode)");
    assert!(matches!(&arms[0].body[0], Stmt::Let { .. }));
    // The same line outside an override block: (Sin 14) == 1.
    let l = lets("float f\nbegin GameMode\nif eval Sin 14 == 1\nendif\nend");
    assert_eq!(l[0], "if (Sin(14) Eq 1)");
}

// ---- user-defined functions and lambdas (task A11) --------------------------------

#[test]
fn a_function_script_has_a_function_block_with_its_parameters() {
    let (c, p) = build("scn TestUDF
int a
int b
Begin Function {a, b}
  SetFunctionValue a + b
End
");
    assert_eq!(p.blocks[0].event, fnv_script::ir::FUNCTION_BLOCK);
    assert_eq!(p.blocks[0].params.as_ref().map(Vec::len), Some(2));
    assert!(matches!(&p.blocks[0].body[0], Stmt::Eval { .. }), "SetFunctionValue is the extender's");
    assert_eq!(c.variables.len(), 2);
}

#[test]
fn call_takes_the_function_and_up_to_fifteen_arguments() {
    let l = lets("int r
ref f
begin GameMode
let r := Call f 1 2 3
let r := Call f, 1, (2 + 3)
end");
    // A `ref` local holding the function is the variable [documented: Lambda, `Call
    // rCallback 10`].
    assert_eq!(l[0], "(v1 Set= Call(v2 1 2 3))");
    assert_eq!(l[1], "(v1 Set= Call(v2 1 (2 Add 3)))");
}

#[test]
fn lambdas_in_both_forms() {
    let src = "int r
int x
ref f
begin GameMode
  let f := (begin function {x}
    SetFunctionValue x * 2
    let r := 1
  end)
  let f := ({x} => x + 1)
end";
    let l = lets(src);
    assert_eq!(l[0], "(v3 Set= lambda[1](2 statements))");
    assert_eq!(l[1], "(v3 Set= lambda[1]((v2 Add 1)))");
}

// ---- strings (task B1) ---------------------------------------------------------------

impl Locals {
    fn text(&self, index: u16) -> String {
        let id = self.0.get(&index).copied().unwrap_or_default().num() as u32;
        self.1.text(id).unwrap_or_default().to_string()
    }
}

#[test]
fn strings_join_copy_and_convert() {
    let h = run("string_var a\nstring_var b\nfloat n\nbegin GameMode\nlet a := \"Hello\"\nlet b := a\nlet a += \", world\"\nlet n := #\"0x10\" + #\" 2.5 \"\nlet b := b + $n + $3 + $1.25\nend");
    assert_eq!(h.text(1), "Hello, world");
    // Assignment copies: b kept "Hello" when a changed [documented: NVSE Expressions].
    assert_eq!(h.text(2), "Hello18.531.25");
    assert_eq!(h.0[&3], Value::Num(18.5));
}

#[test]
fn strings_compare_without_case_and_slice() {
    let h = run("string_var s\nstring_var t\nfloat a\nfloat b\nbegin GameMode\nlet s := \"Abcdef\"\nlet t := s[1:3] + s[-1] + s[0]\nif eval s == \"ABCDEF\" && \"apple\" < \"Banana\"\n  let a := 1\nendif\nlet b := s != \"x\"\nend");
    assert_eq!(h.text(2), "bcdfA");
    assert_eq!(h.0[&3], Value::Num(1.0));
    assert_eq!(h.0[&4], Value::Num(1.0));
}

#[test]
fn a_type_error_is_reported_and_the_script_goes_on() {
    // [documented: Let] a failing let reports and the script carries on.
    let t = table();
    let (c, p) = build("string_var s\nfloat a\nbegin GameMode\nlet a := \"x\" + 1\nlet a := 1 / 0\nlet a := 7\nend");
    let mut h = host_for(&c);
    Vm::new(&t, &mut h, RunOpts::default()).run(&p.blocks[0].body).expect("non-strict runs go on");
    assert_eq!(h.0[&2], Value::Num(7.0));
    let r = Vm::new(&t, &mut h, RunOpts { strict: true, ..RunOpts::default() }).run(&p.blocks[0].body);
    assert!(matches!(r, Err(fnv_script::vm::VmError::Type(_))), "{r:?}");
}

#[test]
fn tostring_and_tonumber() {
    let h = run("string_var s\nfloat n\nbegin GameMode\nlet s := ToString 42\nlet n := (ToNumber \"12\") * 2\nend");
    // ToNumber is NVSE-aware, so without the parentheses its argument would be "12" * 2.
    assert_eq!(h.text(1), "42");
    assert_eq!(h.0[&2], Value::Num(24.0));
}

// ---- arrays (task B3, the D2 functions) -------------------------------------------------

impl Locals {
    fn array_id(&self, index: u16) -> u32 {
        self.0.get(&index).copied().unwrap_or_default().num() as u32
    }
}

#[test]
fn arrays_share_on_assignment_and_index() {
    let h = run("array_var a\narray_var b\nfloat n\nfloat m\nbegin GameMode\nlet a := Ar_List 10, 20, 30\nlet b := a\nlet b[1] := 99\nlet n := a[1] + Ar_Size a\nlet a[3] := 5\nlet m := a[-0] + a[3]\nend");
    assert_eq!(h.array_id(1), h.array_id(2), "assignment shares [documented: NVSE Expressions]");
    assert_eq!(h.0[&3], Value::Num(102.0));
    assert_eq!(h.0[&4], Value::Num(15.0), "setting the key equal to the size appends");
}

#[test]
fn maps_keep_their_keys_sorted() {
    let h = run("array_var m\narray_var k\nstring_var s\nfloat f\nbegin GameMode\nlet m := Ar_Map \"b\"::2, \"A\"::1, \"c\"::3\nlet k := Ar_Keys m\nlet s := k[0] + k[1] + k[2]\nlet f := m->b + m[\"c\"]\nend");
    let text = { let id = h.0[&3].num() as u32; h.1.text(id).unwrap_or_default().to_string() };
    assert_eq!(text, "Abc");
    assert_eq!(h.0[&4], Value::Num(5.0));
}

#[test]
fn foreach_walks_an_array_and_a_string() {
    let h = run("array_var a\narray_var e\nfloat sum\nstring_var t\nstring_var c\nbegin GameMode\nlet a := Ar_Range 1, 5\nforeach e <- a\n  if eval *e == 4\n    break\n  endif\n  let sum += e[\"value\"] * 10 + e[\"key\"]\nloop\nforeach c <- \"abc\"\n  let t := c + t\nloop\nend");
    // 1..3 counted: values 10+20+30, keys 0+1+2.
    assert_eq!(h.0[&3], Value::Num(63.0));
    let id = h.0[&4].num() as u32;
    assert_eq!(h.1.text(id), Some("cba"));
}

#[test]
fn array_functions() {
    let h = run(
        "array_var a\narray_var b\nfloat f\nfloat g\nfloat k\nstring_var t\nbegin GameMode\nlet a := Ar_List 3, 1, 2\nlet b := Ar_Sort a\nlet f := b[0] * 100 + b[2]\nAr_Append a, 7\nAr_Insert a, 0, 9\nlet g := Ar_Erase a, 1:2\nlet k := Ar_Find 7, a\nlet t := TypeOf (Ar_Construct \"stringmap\")\nAr_Dump a\nend",
    );
    assert_eq!(h.0[&3], Value::Num(103.0));
    assert_eq!(h.0[&4], Value::Num(2.0), "two erased");
    assert_eq!(h.0[&5], Value::Num(2.0), "[9, 2, 7]: 7 is at key 2");
    let id = h.0[&6].num() as u32;
    assert_eq!(h.1.text(id), Some("StringMap"));
    assert_eq!(h.4, ["array #1 [0: 9, 1: 2, 2: 7]"]);
}

#[test]
fn temporary_arrays_are_freed() {
    let h = run("array_var a\nfloat n\nbegin GameMode\nlet n := Ar_Size (Ar_List 1, 2, 3)\nlet a := Ar_List (Ar_List 1), 2\nlet a := Ar_Null\nend");
    assert_eq!(h.0[&2], Value::Num(3.0));
    assert!(h.1.arrays.is_empty(), "nothing holds any array at the end: {:?}", h.1.arrays.keys().collect::<Vec<_>>());
}

// ---- string functions, formats, output, version reports (D1, C4, D4, C5) ------------------

#[test]
fn format_codes() {
    // [documented: String Formatting] "i = %g and f = %.2f" with 42 and 3.1415926535.
    let h = run("string_var s\nstring_var t\nbegin GameMode\nlet s := Sv_Construct \"i = %g and f = %.2f\", 42, 3.1415926535\nlet t := Sv_Construct \"%x4|%5.1f|%-4g|%+g|%05.1f|%%|%{%g%}\", 255, 2.25, 7, 3, -1.5, 0, 9\nend");
    assert_eq!(h.text(1), "i = 42 and f = 3.14");
    assert_eq!(h.text(2), "00FF|  2.2|7   |+3|-01.5|%|");
}

#[test]
fn big_numbers_in_g_are_scientific() {
    // [observed: probe format1 in the game: "1e+06", "1.230e+05", "0.5", "123.457",
    // "1.23457E+06", "1.2e+02"].
    let h = run("string_var s\nstring_var t\nbegin GameMode\nlet s := Sv_Construct \"%g %.3e\", 1000000, 123000\nlet t := Sv_Construct \"%g|%g|%G|%.2g|%g\", 0.5, 123.456789, 1234567, 123.456, 100000\nend");
    assert_eq!(h.text(1), "1e+06 1.230e+05");
    assert_eq!(h.text(2), "0.5|123.457|1.23457E+06|1.2e+02|100000");
}

#[test]
fn format_signs_and_huge_fields() {
    // %g writes -0 as 0 [hypothesis], %f keeps its sign [observed: "%.4f" of -0 reads
    // "-0.0000" in the original, #1919], + never doubles a minus, and a huge precision or
    // width is cut (#1145 review, #1134) [hypothesis: the C library's forms].
    let h = run("string_var s
string_var t
string_var u
begin GameMode
let s := Sv_Construct \"%g|%+.2f|%+g|%05.1f\", -0, -0, -2.5, -1.5
let t := Sv_Construct \"%.4000000000f|%4000000000g\", 1, 2
let u := Sv_Construct \"%.4f|%.4f|%+.1f\", -0, 0, 0
end");
    assert_eq!(h.text(1), "0|-0.00|-2.5|-01.5");
    assert_eq!(h.text(3), "-0.0000|0.0000|+0.0");
    assert!(h.text(2).len() < 1200, "{}", h.text(2).len());
}

#[test]
fn literal_codes_in_any_string() {
    let h = run("string_var s\nbegin GameMode\nlet s := \"say %qhi%q%rnext%e\"\nend");
    assert_eq!(h.text(1), "say \"hi\"\nnext");
}

#[test]
fn sv_functions() {
    let h = run(
        "string_var s\narray_var a\nfloat f\nfloat g\nfloat r\nfloat n\nstring_var x\nbegin GameMode\nlet s := \"One two ONE three\"\nlet f := Sv_Find \"one\", s, 1\nlet g := Sv_Length s\nlet r := Sv_Replace \"one|1\", s\nlet a := Sv_Split \"a,b:c\", \",:\"\nlet n := Sv_ToNumeric \"  -12.5e1xyz\" + Sv_GetChar \"A\", 0\nlet x := NumToHex 255, 4, 1\nend",
    );
    assert_eq!(h.0[&3], Value::Num(8.0), "found from position 1, without regard to case");
    assert_eq!(h.0[&4], Value::Num(17.0));
    assert_eq!(h.0[&5], Value::Num(2.0));
    assert_eq!(h.text(1), "1 two 1 three");
    assert_eq!(h.0[&6], Value::Num(-125.0 + 65.0));
    assert_eq!(h.text(7), "0x00FF");
}

#[test]
fn printc_and_version_reports() {
    let h = run("float v\nfloat p\nbegin GameMode\nPrintC \"v%g\", 1\nPrint \"x\" + $2\nlet v := GetNVSEVersion\nlet p := GetPluginVersion \"ShowOff xNVSE Plugin\"\nend");
    assert_eq!(h.4, ["v1", "x2"]);
    assert_eq!(h.0[&1], Value::Num(6.0));
    assert_eq!(h.0[&2], Value::Num(-1.0), "a plugin without a module here is absent");
}

#[test]
fn override_arguments_take_arithmetic_not_comparisons() {
    // [observed: probes expressions/08, 09 in the original game] under _GameMode,
    // `Abs -3 + 1` is 2 and `Sin 90 > 0.5` is (Sin 90) > 0.5.
    let l = lets("float r\nbegin _GameMode\nlet r := Abs -3 + 1\nlet r := Sin 90 > 0.5\nend");
    assert_eq!(l[0], "(v1 Set= 1221(((Neg 3) Add 1)))");
    assert_eq!(l[1], "(v1 Set= (Sin(90) Gt 0.5))");
}

#[test]
fn label_and_goto() {
    // [observed: probe loops/04 gives 3].
    let h = run("float r\nbegin GameMode\nlet r := 0\nLabel 1\nlet r += 1\nif r < 3\n  GoTo 1\nendif\nend");
    assert_eq!(h.0[&1], Value::Num(3.0));
}

#[test]
fn a_failing_condition_is_false_and_the_script_goes_on() {
    // [observed: probe loops/03] erasing forward runs past the shrunk list; the original
    // goes on and ends with 3 elements.
    let t = table();
    let src = "array_var a\nfloat i\nfloat n\nbegin GameMode\nlet a := Ar_List 1, 2, 3, 4, 5, 6\nlet i := -1\nlet n := Ar_Size a\nwhile n > (i += 1)\n  if eval a[i] % 2 == 0\n    Ar_Erase a, i\n  endif\nloop\nlet n := Ar_Size a\nend";
    let (c, p) = build(src);
    let mut h = host_for(&c);
    Vm::new(&t, &mut h, RunOpts::default()).run(&p.blocks[0].body).expect("goes on");
    assert_eq!(h.0[&3], Value::Num(3.0));
}

#[test]
fn math_and_rand() {
    let h = run("float s\nfloat r\nfloat f\nbegin GameMode\nlet s := (Sin 90) + Cos 0\nlet r := Rand 5, 5\nlet f := Floor 2.7 + Pow 2, 3\nend");
    assert_eq!(h.0[&1], Value::Num(2.0), "degrees [observed]");
    assert_eq!(h.0[&2], Value::Num(5.0));
    assert_eq!(h.0[&3], Value::Num(10.0));
}

#[test]
fn a_semicolon_in_an_extender_string_is_a_mismatched_quote() {
    // [observed: the GECK refused this line of probe strings/03, ext-bytecode.md §7].
    let t = table();
    let c = compile("scn X
array_var a
begin GameMode
let a := Sv_Split \"a,b;c\", \",;\"
end", &t, &World, None);
    let kinds: Vec<&str> = c.errors().map(|d| d.kind).collect();
    assert!(kinds.contains(&"mismatched_quotes"), "{kinds:?}");
}

#[test]
fn jip_regex_follows_the_probes() {
    // [observed: jip-forms regex.* rows, jip-strings.md §1].
    let src = [
        "string_var t\nstring_var r\nstring_var e\nfloat a\nfloat b\nfloat c\nfloat n\nfloat v\narray_var m\nbegin GameMode",
        "let t := \"Whatchamacallit\"",
        "let a := Sv_RegexMatch t, \"(What)(.*)\"",
        "let b := Sv_RegexMatch t, \"What\"",
        "let c := Sv_RegexMatch t, \"x(.*)\"",
        "let t := \"This subject has a submarine as a subsequence\"",
        "let m := Sv_RegexSearch t, \"\\b(sub)([^ ]*)\"",
        "let n := Ar_Size m",
        "let r := Sv_RegexReplace t, \"blip!|\\b(sub)([^ ]*)\"",
        "let v := ValidateRegex \"[\", e",
        "end",
    ]
    .join("\n");
    let h = run(&src);
    assert_eq!((h.0[&4], h.0[&5], h.0[&6]), (Value::Num(1.0), Value::Num(0.0), Value::Num(0.0)), "whole-string match");
    assert_eq!(h.0[&7], Value::Num(3.0));
    assert_eq!(h.text(2), "This blip! has a blip! as a blip!");
    assert_eq!(h.0[&8], Value::Num(5.0));
}

#[test]
fn typed_parameters_in_braces_declare_them() {
    // [observed: the original's runtime-compiled function scripts, jip-events.md §4].
    let t = table();
    let c = compile("Begin Function {float fD, ref rS}\nlet fD := ({int n} => n * 2)\nEnd\n", &t, &World, None);
    assert!(c.errors().next().is_none(), "{:?}", c.diags);
    let names: Vec<(&str, &VarType)> = c.variables.iter().map(|v| (v.name.as_str(), &v.ty)).collect();
    assert!(names.contains(&("fD", &VarType::FloatOrRef)) && names.contains(&("rS", &VarType::FloatOrRef)) && names.contains(&("n", &VarType::Int)), "{names:?}");
}

#[test]
fn a_signed_number_after_a_space_starts_the_next_argument() {
    // [observed: maxof.neg, aux.append.* with JIP in the original game].
    let h = run("float m\nfloat d\nfloat s\nbegin GameMode\nlet m := GetMaxOf -1 -5\nlet d := 7 -5\nlet s := GetMaxOf (3 -5) 0\nend");
    assert_eq!(h.0[&1], Value::Num(-1.0), "two arguments");
    assert_eq!(h.0[&2], Value::Num(2.0), "outside arguments a subtraction");
    assert_eq!(h.0[&3], Value::Num(0.0), "inside parentheses a subtraction");
}

#[test]
fn character_and_bit_functions() {
    let h = run("float d
float u
float a
float b
float s
float x
string_var t
string_var c
begin GameMode
let d := IsDigit 55
let u := ToUpper 97
let a := CharToAscii \"Az\"
let b := GetBit 5, 2
let s := SetBit 0, 3
let x := LogicalXor 6, 3
let t := IntToBin 5, 8, 1
let c := AsciiToChar 66
end");
    assert_eq!((h.0[&1], h.0[&2], h.0[&3]), (Value::Num(1.0), Value::Num(65.0), Value::Num(65.0)));
    assert_eq!((h.0[&4], h.0[&5], h.0[&6]), (Value::Num(1.0), Value::Num(8.0), Value::Num(5.0)));
    assert_eq!(h.text(7), "0b00000101");
    assert_eq!(h.text(8), "B");
}

#[test]
fn a_variable_named_like_an_extender_function_is_refused() {
    // [observed: `ref rK`, ReleaseKey's alias, does not compile in the original].
    let t = table();
    let c = compile("Begin Function { }
ref rK
ref rF
End
", &t, &World, None);
    let bad: Vec<&str> = c.errors().map(|d| d.detail.as_str()).collect();
    assert_eq!(bad, ["rK"]);
}

#[test]
fn quaternions_follow_the_probes() {
    // [observed: nvse-math rows in the original game]. Results go to the named variables.
    let t = table();
    let (c, p) = build("float w
float x
float y
float z
begin GameMode
QFromEuler \"w\" \"x\" \"y\" \"z\" 90 0 0
end");
    struct Named(Locals, Vec<(String, u16)>);
    impl Host for Named {
        fn var_kind(&mut self, v: &fnv_script::VarRef) -> fnv_script::vm::VarKind {
            self.0.var_kind(v)
        }
        fn get_var(&mut self, v: &fnv_script::VarRef) -> Result<Value, String> {
            self.0.get_var(v)
        }
        fn set_var(&mut self, v: &fnv_script::VarRef, x: Value) -> Result<(), String> {
            self.0.set_var(v, x)
        }
        fn ref_value(&mut self, r: fnv_script::RefIndex) -> Result<Value, String> {
            self.0.ref_value(r)
        }
        fn call(&mut self, o: u16, on: Option<Value>, a: &[ArgValue]) -> Result<Value, String> {
            self.0.call(o, on, a)
        }
        fn local_var(&mut self, name: &str) -> Option<fnv_script::VarRef> {
            let index = self.1.iter().find(|(n, _)| n == name)?.1;
            Some(fnv_script::VarRef { on: None, var: VarSlot::Local { index, letter: fnv_script::ir::VarLetter::Float } })
        }
    }
    let names = c.variables.iter().map(|v| (v.name.clone(), v.index as u16)).collect();
    let mut h = Named(host_for(&c), names);
    Vm::new(&t, &mut h, RunOpts { strict: true, ..RunOpts::default() }).run(&p.blocks[0].body).expect("runs");
    let g = |i: u16| h.0 .0[&i].num();
    assert!((g(1) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-4 && (g(2) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-4 && g(3).abs() < 1e-9 && g(4).abs() < 1e-9);
}

#[test]
fn jip_reports_and_pure_functions() {
    let h = run(
        "float i
float v
float a
float q
float m
float av
float ok
string_var n
string_var s
ref r
begin GameMode
let i := IsPluginInstalled \"JIP NVSE Plugin\"
let v := GetPluginVersion \"jip nvse plugin\"
let a := Fatan2 1, -1
let q := Fsqrt -1
let m := GetMaxOf 1, 7, 3
let av := StringToActorValue \"strength\"
let ok := ValidateModIndex 200
let n := GetModName 0
let r := StringToRef \"FalloutNV.esm:104C0C\"
let s := RefToString r
end",
    );
    assert_eq!(h.0[&1], Value::Num(1.0));
    assert_eq!(h.0[&2], Value::Num(57.3));
    // 135 at six digits; 135.00020525 at eight [observed: jip-probes rules1, #1025].
    assert!(matches!(h.0[&3], Value::Num(a) if (a - 135.000_205_25).abs() < 2e-5), "{:?}", h.0[&3]);
    assert_eq!(h.0[&4], Value::Num(0.0), "[observed]");
    assert_eq!(h.0[&5], Value::Num(7.0));
    assert_eq!(h.0[&6], Value::Num(5.0));
    assert_eq!(h.0[&7], Value::Num(-1.0));
    assert_eq!(h.text(8), "FalloutNV");
    assert_eq!(h.text(9), "FalloutNV.esm:104C0C");
}

/// Tale of Two Wastelands' start-up checks pass for the plugins built into the engine:
/// Stewie's Tweaks at TTW's minimum 941 and its own ttw_nvse (ext-version.md §5).
#[test]
fn ttws_plugin_checks_find_the_plugins_built_in() {
    let h = run(
        "float s
float t
float u
begin GameMode
let s := GetPluginVersion \"lStewieAl's Tweaks\"
let t := IsPluginInstalled \"ttw_nvse\"
let u := IsPluginInstalled \"Not A Plugin\"
end",
    );
    assert_eq!(h.0[&1], Value::Num(941.0));
    assert_eq!(h.0[&2], Value::Num(1.0));
    assert_eq!(h.0[&3], Value::Num(0.0));
}

#[test]
fn fatan_follows_the_games_approximation() {
    // [observed: jip-probes rules1 and rules2 (#1025), eight and ten decimals].
    let h = run(
        "float a
float b
float c
float d
float e
float f
begin GameMode
let a := Fatan 1
let b := Fatan 0.5
let c := Fatan 10
let d := Fatan2 -1, -1
let e := Fatan2 1, 0
let f := Fatan 0.3
end",
    );
    for (k, want) in [(1, 44.999_803_18), (2, 26.564_936_48), (3, 84.289_417_94), (4, -135.000_205_25), (5, 90.000_002_5), (6, 16.699_303_67)] {
        assert!(matches!(h.0[&k], Value::Num(v) if (v - want).abs() < 2e-5), "{k}: {:?} against {want}", h.0[&k]);
    }
}

#[test]
fn anh_functions_follow_their_pages() {
    // [documented: Ar_Max, Ar_Min, Ar_RangeMath ((6,2,10,7): 10, 2, 8); V3Mult ((1,2,3),
    // (4,5,6): (4,10,18)); V3MultByScalar ((1,2,3) by 2: (2,4,6)); Sv_Capitalize
    // ("pepeglare": "Pepeglare"); GetAngleQuadrant (0.785: 1); Sv_PadStart; DegToRad].
    let h = run(
        "float a
float b
float c
float d
float e
float f
float g
float q
string_var s
string_var t
array_var aA
array_var aB
array_var aOut
begin GameMode
let aA := ar_List 6, 2, 10, 7
let a := Ar_Max aA
let b := Ar_Min aA
let c := Ar_RangeMath aA
let aA := ar_List 1, 2, 3
let aB := ar_List 4, 5, 6
let aOut := V3Mult aA aB
let d := aOut[2]
let aOut := V3MultByScalar aA 2
let e := aOut[1]
let f := V3Dotproduct aA aB
let g := DegToRad 180
let q := GetAngleQuadrant 0.785
let s := Sv_Capitalize \"pepeglare snig\"
let t := Sv_PadStart \"5\" 3 \"0\"
end",
    );
    assert_eq!((h.0[&1], h.0[&2], h.0[&3]), (Value::Num(10.0), Value::Num(2.0), Value::Num(8.0)));
    assert_eq!((h.0[&4], h.0[&5], h.0[&6]), (Value::Num(18.0), Value::Num(4.0), Value::Num(32.0)));
    assert!(matches!(h.0[&7], Value::Num(x) if (x - std::f64::consts::PI).abs() < 1e-6));
    assert_eq!(h.0[&8], Value::Num(1.0));
    assert_eq!(h.text(9), "Pepeglare snig");
    assert_eq!(h.text(10), "005");
}

#[test]
fn jip_aux_variables_and_refmaps_follow_the_probes() {
    // [observed: jip-stores rows, jip-variable-stores.md] on the calling reference.
    let src = [
        "float a\nfloat b\nfloat c\nfloat d\nfloat e\nfloat f\nfloat g\nfloat h\nstring_var s\nfloat k\nfloat m\nfloat q\nbegin GameMode",
        "AuxVarSetFlt \"nvfA\" 3.5",
        "let a := AuxVarGetFlt \"NVFa\"",
        "AuxiliaryVariableSetFloat \"nvfL\", 1, -1\nAuxiliaryVariableSetFloat \"nvfL\", 2, -1\nAuxiliaryVariableSetFloat \"nvfL\", 3, -1",
        "let b := AuxiliaryVariableGetSize \"nvfL\"",
        "let c := AuxiliaryVariableGetFloat \"nvfL\", -1",
        "AuxiliaryVariableSetFloat \"nvfGap\" 1 5",
        "let d := AuxiliaryVariableGetSize \"nvfGap\"",
        "AuxiliaryVariableSetString \"nvfA\" \"abc\"",
        "let e := AuxiliaryVariableGetType \"nvfA\"",
        "let f := AuxiliaryVariableErase \"nvfL\" 0",
        "let g := AuxiliaryVariableGetFloat \"nvfL\"",
        "let h := AuxiliaryVariableErase \"nvfL\"",
        "let s := AuxiliaryVariableGetString \"nvfA\"",
        "RefMapArraySetFloat \"*nvfM\" 4.5\nRefMapArraySetFloat \"*nvfM\" 5.5",
        "let k := RefMapArrayGetFloat \"*nvfM\"",
        "let m := RefMapArrayGetSize \"*nvfM\"",
        "let q := ar_Size (RefMapArrayGetNext \"*nvfM\")",
        "end",
    ]
    .join("\n");
    let t = table();
    let (c, p) = build(&src);
    let mut h = host_for(&c);
    h.5 = Some(PLAYER);
    Vm::new(&t, &mut h, RunOpts { strict: true, ..RunOpts::default() }).run(&p.blocks[0].body).expect("runs");
    assert_eq!(h.0[&1], Value::Num(3.5), "names ignore case");
    assert_eq!(h.0[&2], Value::Num(3.0), "-1 appends");
    assert_eq!(h.0[&3], Value::Num(3.0), "-1 reads the last");
    assert_eq!(h.0[&4], Value::Num(0.0), "past the end does nothing");
    assert_eq!(h.0[&5], Value::Num(4.0), "a new type replaces the element");
    assert_eq!(h.0[&6], Value::Num(2.0), "erase gives the new count");
    assert_eq!(h.0[&7], Value::Num(2.0), "later elements move down");
    assert_eq!(h.0[&8], Value::Num(-1.0), "erasing the whole variable gives -1");
    assert_eq!(h.text(9), "abc");
    assert_eq!(h.0[&10], Value::Num(5.5));
    assert_eq!(h.0[&11], Value::Num(1.0), "one key, set twice");
    // GetNext without a GetFirst starts at the first pair; ar_Size of ar_Null is -1 only
    // after the last pair, here the pair itself has 2 elements.
    assert_eq!(h.0[&12], Value::Num(2.0));
    let x = &h.1;
    assert!(x.jip_temp.refmaps.contains_key("*nvfm"), "a * name is temporary");
    assert!(!x.jip.aux.is_empty() && x.jip.refmaps.is_empty());
}

#[test]
fn string_edits_from_the_probes() {
    let h = run(
        "string_var s\nstring_var t\nstring_var u\nfloat c\nbegin GameMode\nlet s := \"lo\"\nSv_Insert \"Hel\", s, 0\nlet t := \"abcdef\"\nSv_Erase t, 1, 2\nlet u := Sv_SubString \"abcdef\", 1, 3\nlet c := Sv_Compare \"Beta\", \"Alpha\"\nend",
    );
    assert_eq!(h.text(1), "Hello");
    assert_eq!(h.text(2), "adef");
    assert_eq!(h.text(3), "bcd");
    assert_eq!(h.0[&4], Value::Num(1.0));
}

#[test]
fn let_macro_assigns_to_a_subscript() {
    // [observed: JustAssortedMods.esp's handlers, `aEvent["Count"] = …`, compile in the
    // original]
    let h = run("array_var a\nfloat n\nbegin GameMode\nlet a := Ar_List 10, 20\na[1] = 7\na[0] += 1\nlet n := a[0] + a[1]\nend");
    assert_eq!(h.0[&2], Value::Num(18.0));
}

#[test]
fn ignore_crippled_limbs_is_an_actor_value() {
    // [documented: xEdit ActorValue 72; observed: JustAssortedMods.esp names it]
    let (c, _) = build("float f\nbegin GameMode\nlet f := player.GetAV IgnoreCrippledLimbs\nend");
    assert!(c.diags.iter().all(|d| !d.error), "{:?}", c.diags);
}

// ---- grammar forms of the owner's mods' scripts (#1470) ----------------------------

#[test]
fn a_typed_declaration_as_an_output_argument() {
    // `(float f)` where a function writes a variable declares it there (#1470: ln_
    // files of the owner's mod lists, GetNifRotAlt and GetAngleEx).
    let (c, p) = build("begin GameMode\nPlayer.GetNifRotAlt \"RadNeedle\" (float fA) (float fB) (float fC) 2 2\nPlayer.GetAngleEx (float fX) (float fY) (float fZ)\nend");
    assert_eq!(c.variables.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["fA", "fB", "fC", "fX", "fY", "fZ"]);
    let Stmt::Eval { expr: XExpr::Call { call } } = &body(&p)[0] else { panic!("{:?}", body(&p)[0]) };
    assert_eq!(call.args.iter().map(x).collect::<Vec<_>>(), ["\"RadNeedle\"", "v1", "v2", "v3", "2", "2"]);
}

#[test]
fn a_declaration_inside_an_arguments_inline_expression() {
    let (c, p) = build("begin GameMode\nSetUIFloatAlt \"HudMainMenu\\X\\_y\" (int iState = GetPCDetectionState)\nend");
    assert_eq!(c.variables.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["iState"]);
    let Stmt::Eval { expr: XExpr::Call { call } } = &body(&p)[0] else { panic!("{:?}", body(&p)[0]) };
    assert_eq!(x(&call.args[1]), "(v1 Set= GetPCDetectionState())");
}

#[test]
fn a_format_string_takes_its_values_before_the_next_parameter() {
    // [documented: SetModelPathEx, Bugs: 20 `%z` values, then the base form.]
    let z = "%z".repeat(20);
    let s = vec!["s"; 20].join(" ");
    let (_, p) = build(&format!("begin GameMode\nstring_var s = \"\"\nref rB = Caps001\nSetModelPathEx \"a\\b.nif{z}\" {s} rB\nend"));
    let Some(Stmt::Eval { expr: XExpr::Call { call } }) = body(&p).iter().find(|s| matches!(s, Stmt::Eval { .. })) else { panic!() };
    assert_eq!(call.args.len(), 22);
    assert_eq!(x(&call.args[21]), "v2");
}

#[test]
fn values_follow_variables_among_dispatched_arguments() {
    let (_, p) = build("int xkD\nbegin GameMode\nDispatchEventAlt \"B42OCND\" 0\nDispatchEventAlt \"name\" xkD 0\nend");
    let calls: Vec<_> = body(&p).iter().filter_map(|s| match s {
        Stmt::Eval { expr: XExpr::Call { call } } => Some(call.args.iter().map(x).collect::<Vec<_>>()),
        _ => None,
    }).collect();
    assert_eq!(calls, [vec!["\"B42OCND\"", "0"], vec!["\"name\"", "v1", "0"]]);
}

#[test]
fn a_call_lines_surplus_arguments_are_dropped() {
    // ln_B42Optics.txt hands SetJohnnyOnRenderUpdateEventHandler (three parameters) four:
    // the fourth is dropped with a warning, as the vanilla compiler drops them (#1470).
    let (c, _) = build("begin GameMode\nSetJohnnyOnRenderUpdateEventHandler 1 (begin function {}\nDispatchEventAlt \"B42OCND\" 0\nfloat xiR = 1\nend) 0 0\nend");
    let warned: Vec<_> = c.diags.iter().filter(|d| d.kind == "surplus_arguments").collect();
    assert!(warned.len() == 1 && !warned[0].error, "{:?}", c.diags);
    // What reading the dropped ones registered is put back: no reference entry for
    // Caps001, no warning for the bare word, and the next lambda keeps the next number.
    let (c, p) = build("begin GameMode\nSetJohnnyOnRenderUpdateEventHandler 1 ({} => PrintC \"a\") 0 Caps001 Nothing ({} => PrintC \"b\")\nSetJohnnyOnRenderUpdateEventHandler 1 ({} => PrintC \"c\") 0\nend");
    assert!(c.refs.is_empty(), "{:?}", c.refs);
    assert_eq!(c.diags.iter().map(|d| d.kind).collect::<Vec<_>>(), ["surplus_arguments"]);
    let lambdas: Vec<u32> = body(&p).iter().filter_map(|s| match s {
        Stmt::Eval { expr: XExpr::Call { call } } => match &call.args[1] {
            XExpr::Lambda { index, .. } => Some(*index),
            _ => None,
        },
        _ => None,
    }).collect();
    assert_eq!(lambdas, [0, 1]);
    // An argument that cannot be read is not dropped: the line fails.
    let c = compile("begin GameMode\nSetJohnnyOnRenderUpdateEventHandler 1 ({} => PrintC \"a\") 0 Nobody.GetAV Luck\nend", &table(), &World, None);
    assert!(c.errors().any(|d| d.kind == "syntax"), "{:?}", c.diags);
}

#[test]
fn a_block_lambda_may_open_with_a_space() {
    // `CallWhen ( begin function {}` (Bullet Belt Framework's ln_ file, #1470).
    let (c, _) = build("begin GameMode\nCallWhen ( begin function {}\nPrintC \"x\"\nend) ({}=> (GetSecondsPassed) != 0) (0b10100)\nend");
    assert!(c.errors().next().is_none(), "{:?}", c.diags);
}
