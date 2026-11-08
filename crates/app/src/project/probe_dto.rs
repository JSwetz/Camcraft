//! The Probe operation's file mirror (#108): the params as plain keys,
//! the mode and the directions as lowercase words. Provenance: our own
//! file format (`project/mod.rs`); the names are ours.

use super::*;

use crate::probing::{ProbeDir, ProbeMode};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProbeModeDto {
    Face,
    Edge,
    Corner,
    Bore,
    Boss,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProbeDirDto {
    XPos,
    XNeg,
    YPos,
    YNeg,
    ZNeg,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProbeDto {
    pub clearance_z: HeightDto,
    pub mode: ProbeModeDto,
    pub dir: ProbeDirDto,
    pub dir_y: ProbeDirDto,
    pub target: [f64; 3],
    pub radius: f64,
    pub approach: f64,
    pub overtravel: f64,
    pub backoff: f64,
    pub fast_feed: f64,
    pub slow_feed: f64,
    pub depth: f64,
    pub inset: f64,
    /// Absent: report and stop for the operator; 0..=5: G54..G59.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_wcs: Option<u8>,
    pub tolerance: f64,
}

fn mode_dto(m: ProbeMode) -> ProbeModeDto {
    match m {
        ProbeMode::Face => ProbeModeDto::Face,
        ProbeMode::Edge => ProbeModeDto::Edge,
        ProbeMode::Corner => ProbeModeDto::Corner,
        ProbeMode::Bore => ProbeModeDto::Bore,
        ProbeMode::Boss => ProbeModeDto::Boss,
    }
}

fn mode_of(m: ProbeModeDto) -> ProbeMode {
    match m {
        ProbeModeDto::Face => ProbeMode::Face,
        ProbeModeDto::Edge => ProbeMode::Edge,
        ProbeModeDto::Corner => ProbeMode::Corner,
        ProbeModeDto::Bore => ProbeMode::Bore,
        ProbeModeDto::Boss => ProbeMode::Boss,
    }
}

fn dir_dto(d: ProbeDir) -> ProbeDirDto {
    match d {
        ProbeDir::XPos => ProbeDirDto::XPos,
        ProbeDir::XNeg => ProbeDirDto::XNeg,
        ProbeDir::YPos => ProbeDirDto::YPos,
        ProbeDir::YNeg => ProbeDirDto::YNeg,
        ProbeDir::ZNeg => ProbeDirDto::ZNeg,
    }
}

fn dir_of(d: ProbeDirDto) -> ProbeDir {
    match d {
        ProbeDirDto::XPos => ProbeDir::XPos,
        ProbeDirDto::XNeg => ProbeDir::XNeg,
        ProbeDirDto::YPos => ProbeDir::YPos,
        ProbeDirDto::YNeg => ProbeDir::YNeg,
        ProbeDirDto::ZNeg => ProbeDir::ZNeg,
    }
}

impl ProbeDto {
    /// The probe's keys; any other kind (never passed - the caller
    /// matched the probe) files the probe's own draft numbers.
    pub(super) fn from_params(p: &OpParams) -> Self {
        let OpParams::Probe {
            clearance_z,
            mode,
            dir,
            dir_y,
            target,
            radius,
            approach,
            overtravel,
            backoff,
            fast_feed,
            slow_feed,
            depth,
            inset,
            set_wcs,
            tolerance,
        } = p
        else {
            return ProbeDto::from_params(&crate::operation::retype_params(
                p,
                crate::operation::OpType::Probe.ordinal(),
            ));
        };
        ProbeDto {
            clearance_z: (*clearance_z).into(),
            mode: mode_dto(*mode),
            dir: dir_dto(*dir),
            dir_y: dir_dto(*dir_y),
            target: *target,
            radius: *radius,
            approach: *approach,
            overtravel: *overtravel,
            backoff: *backoff,
            fast_feed: *fast_feed,
            slow_feed: *slow_feed,
            depth: *depth,
            inset: *inset,
            set_wcs: *set_wcs,
            tolerance: *tolerance,
        }
    }

    pub(super) fn to_params(&self) -> OpParams {
        OpParams::Probe {
            clearance_z: self.clearance_z.into(),
            mode: mode_of(self.mode),
            dir: dir_of(self.dir),
            dir_y: dir_of(self.dir_y),
            target: self.target,
            radius: self.radius,
            approach: self.approach,
            overtravel: self.overtravel,
            backoff: self.backoff,
            fast_feed: self.fast_feed,
            slow_feed: self.slow_feed,
            depth: self.depth,
            inset: self.inset,
            set_wcs: self.set_wcs,
            tolerance: self.tolerance,
        }
    }
}
