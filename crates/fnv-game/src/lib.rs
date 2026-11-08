//! Layer 5 game rules over the Layer 4 world (docs/design/layer5.md §3.1): script
//! instances and their variables, event dispatch, the quest model, condition (`CTDA`)
//! evaluation, the base-game function implementations, and the headless script runner
//! with its trace (`fnv.script.trace/1`). The interpreter itself is
//! `fnv_script::vm`; this crate is the `Host` it runs against.
//!
//! - [`aid`]: aid items (ingestibles) and what magic effects do to actor values
//! - [`data`]: compiled scripts, quests, dialogue/package results, messages, form lists
//! - [`quest`]: quest definitions and run-time state
//! - [`cond`]: the conditions of `CTDA` records
//! - [`package`]: AI packages as typed data (docs/spec-notes/packages.md)
//! - [`ai`]: which package an actor runs and when it chooses again
//! - [`game`]: the game state, instances, dispatch, SetStage, activation, snapshots
//! - [`functions`]: function implementations by opcode
//! - [`trace`]: trace events
//! - [`terminal`]: whether a terminal opens, must be hacked or is locked out; its menu items
//! - [`detection`]: how well one actor perceives another (hidden, noticed, detected)
//! - [`dialogue`]: conversations between the player and an actor (Layer 6, step D2)
//! - [`hacking`]: the terminal hacking game (words, likeness, bracket pairs, lockout)
//! - [`lockpick`]: the lockpicking game (sweet spot, torque, bobby pins, Force Lock)
//! - [`combat_lines`]: Combat-tab lines (Hit, Attack, PowerAttack, Death …) and effort sounds
//! - [`crime`]: karma, ownership, theft, trespass, assault and murder
//! - [`pickpocket`]: pickpocketing, its chance, being caught, planted explosives
//! - [`idles`]: the idle animations scripts, packages and dialogue lines ask for
//! - [`chargen`]: the character-creation menus' rules (name, S.P.E.C.I.A.L., tag skills, traits)
//! - [`intro`]: the intro chain's units (coverage)
//! - [`runner`]: `fnv.script.actions/1` → `fnv.script.trace/1`
//! - [`respawn`]: cell resets: respawning actors back alive after `iHoursToRespawnCell`
//! - [`zones`]: encounter zones' levels and each placed actor's leveled level
//! - [`voice`]: the hook that gives a dialogue response the length of its voice file
//! - [`weather`]: the weather scripts ask for and the one the sky shows
//!
//! Behaviour notes with confidence tags: docs/spec-notes/scripts.md §4 and later.

pub mod abilities;
pub mod activate_choices;
pub mod actor_values;
pub mod aggro;
pub mod ash_piles;
pub mod aid;
pub mod area_effects;
pub mod appearance;
pub mod ai;
pub mod anim_state;
pub mod anims;
pub mod awareness;
pub mod blackjack;
pub mod breath;
pub mod camera_paths;
pub mod books;
pub mod caravan;
pub mod challenges;
pub mod casino;
pub mod chargen;
pub mod combat;
pub mod combat_group;
pub mod combat_lines;
pub mod combat_style;
pub mod companion_wheel;
pub mod coop;
pub mod cond;
pub mod coverage;
pub mod crafting;
pub mod crime;
pub mod data;
pub mod source_compile;
pub mod sweep;
pub mod death_items;
pub mod detection;
pub mod followers;
pub mod dialogue;
pub mod dialogue_edits;
pub mod difficulty;
pub mod experience;
pub mod functions;
pub mod hacking;
pub mod holotapes;
pub mod game;
pub mod idles;
pub mod intro;
pub mod jip_events;
pub mod nvse;
pub mod nvse_ttw;
pub mod delayed;
pub mod nvse_refs;
pub mod nvse_misc;
pub mod nvse_data;
pub mod nvse_notes;
pub mod nvse_nx;
pub mod nvse_actor;
pub mod nvse_items;
pub mod nvse_amata;
pub mod nvse_jip;
pub mod nvse_other;
pub mod nvse_console;
pub mod nvse_lists;
pub mod nvse_anh;
pub mod nvse_itr1;
pub mod nvse_itr2;
pub mod nav_ask;
pub mod nvse_jippp;
pub mod nvse_knvse;
pub mod nvse_small;
pub mod script_runner;
pub mod nvse_sup1;
pub mod nvse_sup3;
pub mod nvse_sup5;
pub mod image_files;
pub mod xls;
pub mod nvse_dialogue;
pub mod nvse_vheyes;
pub mod nvse_one111;
pub mod nvse_jg1;
pub mod nvse_jg2;
pub mod nvse_jg3;
pub mod nvse_jg4;
pub mod nvse_jg5;
pub mod nvse_lutana;
pub mod nvse_jip5;
pub mod nvse_jip6;
pub mod nvse_jip7;
pub mod nvse_jip8;
pub mod nvse_pp;
pub mod nvse_rest;
pub mod nvse_showoff2;
pub mod nvse_showoff3;
pub mod nvse_showoff4;
pub mod nvse_leveled;
pub mod nvse_jip2;
pub mod nvse_gaps;
pub mod nvse_jip3;
pub mod nvse_jip4;
pub mod nvse_proj;
pub mod nvse_sound;
pub mod nvse_hud;
pub mod nvse_showoff;
pub mod nvse_plugins;
pub mod ray;
pub mod mod_files;
pub mod nvse_traits;
pub mod nvse_anim;
pub mod nvse_anim_folders;
pub mod nif_blocks;
pub mod nvse_settings;
pub mod nvse_tokens;
pub mod nvse_handler_lists;
pub mod nvse_debug;
pub mod text_codes;
pub mod script_input;
pub mod levelup;
pub mod lockpick;
pub mod magic_archetypes;
pub mod misc_stats;
pub mod needs;
pub mod npc_doors;
pub mod npc_stimpaks;
pub mod rearm;
pub mod package;
pub mod perks;
pub mod pickpocket;
pub mod poker;
pub mod provocation;
pub mod revision;
pub mod quest;
pub mod quest_targets;
pub mod radiation;
pub mod radio;
pub mod repair;
pub mod respawn;
pub mod resurrect;
pub mod rest;
pub mod roulette;
pub mod slots;
pub mod sound;
pub mod style_weapon;
pub mod talking_activators;
pub mod runner;
pub mod services;
pub mod sight;
pub mod templates;
pub mod terminal;
pub mod trace;
pub mod swing_traps;
pub mod traps;
pub mod ui_cache;
pub mod ui_functions;
pub mod ui_gradients;
pub mod ui_hudbar;
pub mod ui_loading;
pub mod ui_hud;
pub mod ui_more;
pub mod ui_input;
pub mod unconscious;
pub mod use_weapon;
pub mod vats;
pub mod voice;
pub mod weather;
pub mod zones;

pub use data::GameData;
pub use game::{Game, GameConfig, InstKey};
