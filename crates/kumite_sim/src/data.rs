//! Typed views of the converted game data (`assets/gamedata/*.json`, written by export_gamedata).
//! Only the fields the simulation uses are declared; serde ignores the rest.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

/// How many moves a moveset file has (its moves skipped over, not built).
pub fn move_count(path: &Path) -> Result<usize> {
    #[derive(Deserialize)]
    struct Count {
        moves: Vec<serde::de::IgnoredAny>,
    }
    Ok(load::<Count>(path)?.moves.len())
}

pub fn load<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let b = kumite_fs::read(path).with_context(|| format!("opening {}", path.display()))?;
    serde_json::from_slice(&b).with_context(|| format!("parsing {}", path.display()))
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Bank {
    Own,
    Common,
    None,
    /// A move of the opponent's set, played on this fighter: throw victims play the thrower's victim
    /// animations (the game points the thrown actor at the thrower's records). Never in the data files.
    Partner,
}

/// A move reference: which moveset (the character's own, or the shared common set) and the record index.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
pub struct MoveRef {
    pub bank: Bank,
    #[serde(rename = "move")]
    pub index: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Moveset {
    /// Moveset id (descriptor +9); `None` for the common set.
    pub moveset_id: Option<i64>,
    pub moves: Vec<Move>,
    pub cancels: Vec<Cancel>,
    /// Alias (move id) table: 4023 entries in a character's moveset; absent in the common set.
    #[serde(default)]
    pub aliases: Option<Vec<MoveRef>>,
    /// Scripted camera motions, by camera selector index (decoder 0x80038CE4).
    #[serde(default)]
    pub camera_motions: std::collections::HashMap<String, CameraMotion>,
}

impl Moveset {
    /// The move references in range: some moves, each alias's own move in this set, its common one in the common set
    /// (`common` moves). A fighter indexes its moves by them every frame.
    pub fn check(&self, common: usize) -> Result<()> {
        anyhow::ensure!(!self.moves.is_empty(), "no moves");
        for (k, r) in self.aliases.iter().flatten().enumerate() {
            let len = match r.bank {
                Bank::Own => self.moves.len(),
                Bank::Common => common,
                _ => continue,
            };
            anyhow::ensure!(r.index.is_none_or(|i| i < len), "alias {k}: {:?} move {:?} past the {len} moves", r.bank, r.index);
        }
        Ok(())
    }
}

/// A scripted camera motion: per frame, its channels (mode 1: eye x y z, target x y z, ×2; game space).
#[derive(Clone, Debug, Deserialize)]
pub struct CameraMotion {
    pub frames: usize,
    pub values: Vec<Vec<i32>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Move {
    pub index: usize,
    /// glTF animation name in the character's .glb (`motion_NNN` or `common_NNN`).
    pub anim: String,
    pub frames: u32,
    pub posture: Posture,
    pub attack_mask: AttackMask,
    pub cancel_start: usize,
    /// The motion's family tag (reversals, cond 0x4B).
    #[serde(default)]
    pub motion_tag: i32,
    pub next: Next,
    pub damage: i32,
    pub damage_type: u32,
    /// Record +0x16: the drift along the facing over the jump window (0x8003F3A8), per frame / 2048 units.
    #[serde(default)]
    pub air_drift: i32,
    /// First and last active frame (0, 0 = no attack).
    pub active: [u32; 2],
    #[serde(default)]
    pub hitstop: u32,
    pub reaction: Reaction,
    #[serde(default)]
    pub attack: Option<Attack>,
    #[serde(default)]
    pub properties: Option<Vec<Property>>,
    #[serde(default)]
    pub flags24: Flags24,
    /// Voice/SFX event list (move-properties.md §5).
    #[serde(default)]
    pub sounds: Option<Vec<SoundEntry>>,
    /// Close-range reaction (record +0x34; index into reactions.json `close_range`, 0 = none).
    #[serde(default)]
    pub close_range: usize,
    #[serde(default)]
    pub bytes_18: Bytes18,
    /// Attack descriptor byte 0 (record +0x28, built-in descriptors included): the CPU AI tests the opponent's
    /// (0x18 and 0x1A: the two characters the data names).
    #[serde(default)]
    pub desc0: Option<u32>,
}

/// One sound-list entry: `type << 12 | index`.
#[derive(Clone, Debug, Deserialize)]
pub struct SoundEntry {
    #[serde(rename = "type")]
    pub kind: u32,
    pub index: u32,
}

/// Move word +0x24: flag bits plus the camera id byte (0x27..0x2A cap the fight camera's yaw speed).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Flags24 {
    pub raw: u32,
    pub camera_script: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Posture {
    pub raw: u32,
    pub base: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AttackMask {
    /// Full word: hit mask in the low bits, CPU AI range/class hints from bit 16 (0x40000 close, 0x80000 mid,
    /// 0x100000 long, 0x400000 throw).
    #[serde(default)]
    pub raw: u32,
    pub mask: u32,
    pub level: u32,
    pub level_name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Next {
    pub alias: usize,
    /// Record +0x12: the animation's net turn (u16, 0x10000 = 360°), added to the facing by END / buffered links.
    #[serde(default)]
    pub hi: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Reaction {
    pub index: usize,
    /// "reactions" (the 14-slot table) or "throw_victims" (id & 0x1000: the victim alias list 0x800174C4).
    #[serde(default)]
    pub table: String,
}

impl Reaction {
    pub fn is_throw(&self) -> bool {
        self.table == "throw_victims"
    }
}

/// Record bytes 0x18..0x1B: the jump window and the frame a fall animation holds until landing.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Bytes18 {
    /// Record +0x18: the head may follow an attack from frame (this − 17) on (0x80039104).
    #[serde(default)]
    pub frame_count_ram: u32,
    #[serde(default)]
    pub jump_start: u32,
    #[serde(default)]
    pub jump_end: u32,
    #[serde(default)]
    pub hold_frame: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Property {
    pub frame: u32,
    pub op: u32,
    pub arg: u32,
}

/// Baked strike path: per active frame, up to two segments in character space (original-hardware axes, game units).
#[derive(Clone, Debug, Deserialize)]
pub struct Attack {
    #[serde(default)]
    pub frames: Vec<AttackFrame>,
    /// Descriptor bytes 0..5: the striking limbs' bone slots first (reversal limb test 0x8002EB4C).
    #[serde(default)]
    pub header_bytes: Vec<u32>,
    #[serde(default)]
    pub bones: Option<AttackBones>,
}

/// Bone slots of the strike path (end0 < 0xC = an arm, else a leg; used to pick block sounds).
#[derive(Clone, Debug, Deserialize)]
pub struct AttackBones {
    pub end0: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AttackFrame {
    pub frame: u32,
    pub segments: Vec<[[i32; 3]; 2]>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CancelKind {
    Command,
    Sequence,
    End,
    Call,
    Return,
    Dash,
    Backdash,
    #[serde(other)]
    Other,
}

/// A command/cancel entry (12 bytes in the game; kumite_converter's reference engine.md §7.2).
#[derive(Clone, Debug, Deserialize)]
pub struct Cancel {
    pub cmd: u32,
    pub kind: CancelKind,
    #[serde(default)]
    pub input: String,
    pub req: u32,
    pub cond: u32,
    pub param: u32,
    pub dest: usize,
    pub win_start: u32,
    pub win_end: u32,
    /// Frame at which the buffered move takes over (255 = at animation end); for `Call`, unused.
    pub exec: u32,
    /// Transition byte; 0x2C = "use this list's END entry" (a match ends the walk, nothing is queued).
    #[serde(default)]
    pub transition: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Shared {
    pub shared_cancels: SharedCancels,
    pub sequences: Sequences,
    #[serde(default)]
    pub reversal_families: ReversalFamilies,
}

/// Reversal families (cond 0x4B): lists of motion tags.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReversalFamilies {
    pub families: Vec<Vec<i32>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SharedCancels {
    pub entries: Vec<Cancel>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Sequences {
    pub entries: Vec<Sequence>,
}

/// A motion input (e.g. qcf+1+2): steps matched backwards through the input history within `window` frames.
#[derive(Clone, Debug, Deserialize)]
pub struct Sequence {
    pub cmd: u32,
    pub window: u32,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Step {
    /// The step word (dir | buttons << 8 | any-of << 12): the CPU AI plays it as one frame of input.
    #[serde(default)]
    pub raw: u32,
    pub dir: u32,
    pub buttons: Vec<u32>,
    pub any_of: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Reactions {
    pub entries: Vec<ReactionEntry>,
    pub pushback_curves: PushbackCurves,
    pub air_reactions: AirReactions,
    /// Fixed pushback of air (juggle) hits (0x80019D2C), per frame.
    pub air_pushback_curve: AirPushback,
    pub throw_victims: ThrowVictims,
    pub close_range: CloseRange,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AirPushback {
    pub samples: Vec<i32>,
}

/// Common air (juggle) reactions (0x8001A630): target height and alias.
#[derive(Clone, Debug, Deserialize)]
pub struct AirReactions {
    pub entries: Vec<AirReaction>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AirReaction {
    pub target_height: i32,
    pub alias: usize,
}

/// Victim aliases of throws and holds (0x800174C4), by reaction id & 0xFFF.
#[derive(Clone, Debug, Deserialize)]
pub struct ThrowVictims {
    pub entries: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CloseRange {
    pub entries: Vec<CloseRangeEntry>,
}

/// Close-range reaction (0x800177DC): used when the pair is nearer than `max_distance`.
#[derive(Clone, Debug, Deserialize)]
pub struct CloseRangeEntry {
    pub reaction_id: usize,
    pub max_distance: i32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PushbackCurves {
    pub entries: Vec<PushbackCurve>,
}

/// Pushback after a hit or block (table 0x8001075C): `constant_frames` frames at `velocity`, plus an impulse on
/// each of the first 8 frames; units per frame along the pushback direction.
#[derive(Clone, Debug, Deserialize)]
pub struct PushbackCurve {
    /// Reference value used by the reaction parameters (a multiple of 10).
    pub index: u32,
    pub constant_frames: i32,
    pub velocity: i32,
    pub impulses: [i32; 8],
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReactionEntry {
    /// 14 victim move aliases (slot order: kumite_converter's reference engine.md §7.5).
    pub victims: Vec<usize>,
    #[serde(default)]
    pub params: Vec<i32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MovesetIndex {
    pub models: Vec<ModelEntry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModelEntry {
    pub model_record: usize,
    pub glb: String,
    pub moveset_file: String,
    pub moveset_id: i64,
}
