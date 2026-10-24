//! Arrowhead names and flags: `arrow_flags` and the name parser of `lib/common/arrows.c`,
//! shared by the layouts (which clip each spline for its arrowheads, as `arrow_clip` does) and
//! the renderers (which draw each type).

use crate::AttrMap;

pub const NUMB_OF_ARROW_HEADS: u32 = 4;
pub const BITS_PER_ARROW: u32 = 8;
pub const BITS_PER_ARROW_TYPE: u32 = 4;

pub const ARR_TYPE_NONE: u32 = 0;
pub const ARR_TYPE_NORM: u32 = 1;
pub const ARR_TYPE_CROW: u32 = 2;
pub const ARR_TYPE_TEE: u32 = 3;
pub const ARR_TYPE_BOX: u32 = 4;
pub const ARR_TYPE_DIAMOND: u32 = 5;
pub const ARR_TYPE_DOT: u32 = 6;
pub const ARR_TYPE_CURVE: u32 = 7;
pub const ARR_TYPE_GAP: u32 = 8;

pub const ARR_MOD_OPEN: u32 = 1 << BITS_PER_ARROW_TYPE;
pub const ARR_MOD_INV: u32 = 1 << (BITS_PER_ARROW_TYPE + 1);
pub const ARR_MOD_LEFT: u32 = 1 << (BITS_PER_ARROW_TYPE + 2);
pub const ARR_MOD_RIGHT: u32 = 1 << (BITS_PER_ARROW_TYPE + 3);

const SYNONYMS: &[(&str, u32)] = &[("invempty", ARR_TYPE_NORM | ARR_MOD_INV | ARR_MOD_OPEN)];
const MODS: &[(&str, u32)] = &[
    ("o", ARR_MOD_OPEN),
    ("r", ARR_MOD_RIGHT),
    ("l", ARR_MOD_LEFT),
    ("e", ARR_MOD_OPEN),
    ("half", ARR_MOD_LEFT),
];
const NAMES: &[(&str, u32)] = &[
    ("normal", ARR_TYPE_NORM),
    ("crow", ARR_TYPE_CROW),
    ("tee", ARR_TYPE_TEE),
    ("box", ARR_TYPE_BOX),
    ("diamond", ARR_TYPE_DIAMOND),
    ("dot", ARR_TYPE_DOT),
    ("none", ARR_TYPE_GAP),
    ("inv", ARR_TYPE_NORM | ARR_MOD_INV),
    ("vee", ARR_TYPE_CROW | ARR_MOD_INV),
    ("pen", ARR_TYPE_CROW | ARR_MOD_INV),
    ("mpty", ARR_TYPE_NORM),
    ("curve", ARR_TYPE_CURVE),
    ("icurve", ARR_TYPE_CURVE | ARR_MOD_INV),
];

fn match_frag<'s>(name: &'s str, table: &[(&str, u32)], flag: &mut u32) -> &'s str {
    for (n, t) in table {
        if let Some(rest) = name.strip_prefix(n) {
            *flag |= t;
            return rest;
        }
    }
    name
}

fn match_shape(name: &str) -> (u32, &str) {
    let mut f = ARR_TYPE_NONE;
    let mut rest = match_frag(name, SYNONYMS, &mut f);
    if rest.len() == name.len() {
        loop {
            let next = rest;
            rest = match_frag(next, MODS, &mut f);
            if rest.len() == next.len() {
                break;
            }
        }
        rest = match_frag(rest, NAMES, &mut f);
    }
    if f != 0 && f & ((1 << BITS_PER_ARROW_TYPE) - 1) == 0 {
        f |= ARR_TYPE_NORM;
    }
    (f, rest)
}

/// `arrow_match_name`: the flags of an `arrowhead`/`arrowtail` value, warning about the first
/// unknown part as C does.
pub fn match_name(name: &str) -> u32 {
    let mut flag = 0u32;
    let mut rest = name;
    let mut i = 0;
    while !rest.is_empty() && i < NUMB_OF_ARROW_HEADS {
        let next = rest;
        let (mut f, r) = match_shape(next);
        rest = r;
        if f == ARR_TYPE_NONE {
            // C warns on every arrow_flags call, all of them in the layout: arrow_clip
            // (splines.c:79), user_spline (neatoinit.c:278) and the opposing edge of a
            // concentrated one (arrows.c:247). Not for a repeat C does not make.
            if crate::diag::reporting() {
                crate::diag::agwarningf(&format!("Arrow type \"{next}\" unknown - ignoring\n"));
            }
            return flag;
        }
        if f == ARR_TYPE_GAP && i == NUMB_OF_ARROW_HEADS - 1 {
            f = ARR_TYPE_NONE;
        }
        if f == ARR_TYPE_GAP && i == 0 && rest.is_empty() {
            f = ARR_TYPE_NONE;
        }
        if f != ARR_TYPE_NONE {
            flag |= f << (i * BITS_PER_ARROW);
            i += 1;
        }
    }
    flag
}

/// `arrow_flags` without the `ED_conc_opp_flag` merge (dot's spline graph adds it): the start
/// and end arrow flags of an edge with attributes `attrs`.
pub fn arrow_flags(attrs: &AttrMap, directed: bool) -> (u32, u32) {
    let mut sflag = ARR_TYPE_NONE;
    let mut eflag = if directed {
        ARR_TYPE_NORM
    } else {
        ARR_TYPE_NONE
    };
    if let Some(d) = attrs.get("dir").filter(|s| !s.is_empty()) {
        match d.as_str() {
            "forward" => (sflag, eflag) = (ARR_TYPE_NONE, ARR_TYPE_NORM),
            "back" => (sflag, eflag) = (ARR_TYPE_NORM, ARR_TYPE_NONE),
            "both" => (sflag, eflag) = (ARR_TYPE_NORM, ARR_TYPE_NORM),
            "none" => (sflag, eflag) = (ARR_TYPE_NONE, ARR_TYPE_NONE),
            _ => {}
        }
    }
    if eflag == ARR_TYPE_NORM
        && let Some(a) = attrs.get("arrowhead").filter(|s| !s.is_empty())
    {
        eflag = match_name(a);
    }
    if sflag == ARR_TYPE_NORM
        && let Some(a) = attrs.get("arrowtail").filter(|s| !s.is_empty())
    {
        sflag = match_name(a);
    }
    (sflag, eflag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_names() {
        assert_eq!(match_name("normal"), ARR_TYPE_NORM);
        assert_eq!(match_name("onormal"), ARR_TYPE_NORM | ARR_MOD_OPEN);
        assert_eq!(match_name("inv"), ARR_TYPE_NORM | ARR_MOD_INV);
        assert_eq!(match_name("none"), ARR_TYPE_NONE);
        assert_eq!(
            match_name("invdot"),
            (ARR_TYPE_NORM | ARR_MOD_INV) | (ARR_TYPE_DOT << 8)
        );
    }

    #[test]
    fn flags_follow_dir_and_directedness() {
        let mut m = AttrMap::new();
        assert_eq!(arrow_flags(&m, true), (0, 1));
        assert_eq!(arrow_flags(&m, false), (0, 0));
        m.insert("dir".into(), "both".into());
        assert_eq!(arrow_flags(&m, false), (1, 1));
    }
}
