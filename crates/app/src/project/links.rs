//! The file mirror of `links::Links` (M11.2 C/D): one optional `links`
//! object per ladder kind, absent when the op carries its kind's
//! pre-M11.2 shape and, inside it, only the fields that differ from that
//! shape - so every earlier file reads unchanged and every op that never
//! touched the choice saves byte-identically (no version step: the
//! serde-default law). Provenance: our own file format.

use super::*;

use crate::links::{Link, Links};

/// The file word for a [`Link`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkDto {
    Clearance,
    Retract,
    KeepDown,
}

/// The changed fields of an op's [`Links`]; every field absent is the
/// kind's default.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct LinksDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<EntryDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp_angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helix_pitch: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<LinkDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass: Option<LinkDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_stay_down: Option<f64>,
    /// M11.3 B/C/D: the entry, lead and feed parameters, diff-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approach: Option<ApproachDto>,
}

fn link_dto(l: Link) -> LinkDto {
    match l {
        Link::Clearance => LinkDto::Clearance,
        Link::Retract => LinkDto::Retract,
        Link::KeepDown => LinkDto::KeepDown,
    }
}

fn link_of(d: LinkDto) -> Link {
    match d {
        LinkDto::Clearance => Link::Clearance,
        LinkDto::Retract => Link::Retract,
        LinkDto::KeepDown => Link::KeepDown,
    }
}

fn entry_dto(e: Entry3D) -> EntryDto {
    match e {
        Entry3D::Plunge => EntryDto::Plunge,
        Entry3D::Ramp => EntryDto::Ramp,
        Entry3D::Helix => EntryDto::Helix,
        Entry3D::Arc => EntryDto::Arc,
    }
}

fn entry_of(d: EntryDto) -> Entry3D {
    match d {
        EntryDto::Plunge => Entry3D::Plunge,
        EntryDto::Ramp => Entry3D::Ramp,
        EntryDto::Helix => Entry3D::Helix,
        EntryDto::Arc => Entry3D::Arc,
    }
}

/// `l` as written against the kind default `def`: `None` when equal
/// (the key stays out of the file), else only the differing fields.
#[must_use]
pub fn links_to_dto(l: &Links, def: &Links) -> Option<LinksDto> {
    if l == def {
        return None;
    }
    let num = |v: f64, d: f64| (v.to_bits() != d.to_bits()).then_some(v);
    Some(LinksDto {
        transition: if l.transition == def.transition { None } else { l.transition.map(entry_dto) },
        ramp_angle: num(l.ramp_angle, def.ramp_angle),
        helix_pitch: num(l.helix_pitch, def.helix_pitch),
        level: (l.level != def.level).then(|| link_dto(l.level)),
        pass: (l.pass != def.pass).then(|| link_dto(l.pass)),
        max_stay_down: num(l.max_stay_down, def.max_stay_down),
        approach: approach_to_dto(&l.approach),
    })
}

/// The op's links from the file: the kind default `def` with every
/// stated field laid over it.
#[must_use]
pub fn links_from_dto(d: Option<&LinksDto>, def: Links) -> Links {
    let Some(d) = d else {
        return def;
    };
    Links {
        transition: d.transition.map(entry_of).or(def.transition),
        ramp_angle: d.ramp_angle.unwrap_or(def.ramp_angle),
        helix_pitch: d.helix_pitch.unwrap_or(def.helix_pitch),
        level: d.level.map_or(def.level, link_of),
        pass: d.pass.map_or(def.pass, link_of),
        max_stay_down: d.max_stay_down.unwrap_or(def.max_stay_down),
        approach: if d.approach.is_some() { approach_from_dto(d.approach.as_ref()) } else { def.approach },
    }
}

/// The contour's pre-M11.2 keys, read: `stay_down_ramp` at `ramp_angle`
/// is a Keep-down level link descending by a Ramp transition at that
/// angle; `retract_reenter` is the default. A `links` object, when the
/// file has one, lays over that. The migration is in memory only.
#[must_use]
pub fn contour_links(level_link: LevelLinkDto, ramp_angle: f64, d: Option<&LinksDto>) -> Links {
    let base = match level_link {
        LevelLinkDto::RetractReenter => Links { ramp_angle, ..Links::PLANAR },
        LevelLinkDto::StayDownRamp => Links {
            transition: Some(Entry3D::Ramp),
            level: Link::KeepDown,
            ramp_angle,
            ..Links::PLANAR
        },
    };
    links_from_dto(d, base)
}

/// The contour's links as written: the old keys where they say it all
/// (so the previous build still reads a stay-down ramp right), the
/// `links` object otherwise.
#[must_use]
pub fn contour_links_dto(l: &Links) -> (LevelLinkDto, f64, Option<LinksDto>) {
    let legacy = Links {
        transition: Some(Entry3D::Ramp),
        level: Link::KeepDown,
        ramp_angle: l.ramp_angle,
        ..Links::PLANAR
    };
    if *l == legacy {
        return (LevelLinkDto::StayDownRamp, l.ramp_angle, None);
    }
    let def = Links::PLANAR;
    (LevelLinkDto::RetractReenter, def.ramp_angle, links_to_dto(l, &def))
}
