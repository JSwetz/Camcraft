//! STEP length units (#306): how many millimetres one of a file's
//! length units is, so a part saved in inches or metres imports at its
//! true size instead of having its numbers read as millimetres.
//!
//! Provenance: the unit entities of ISO 10303-41 as Part 21 files write
//! them - `GLOBAL_UNIT_ASSIGNED_CONTEXT`, `LENGTH_UNIT`, `SI_UNIT`
//! (prefix, name), `CONVERSION_BASED_UNIT` (name, measure) and
//! `LENGTH_MEASURE_WITH_UNIT` (value, unit). Public standard; nothing
//! from any CAM vendor.

use std::collections::HashMap;

use ruststep::ast::{DataSection, EntityInstance, Name, Parameter, Record};

/// How deep a conversion-based unit may chain to other units before it
/// is refused: a real file goes INCH -> millimetre in one step.
const MAX_CHAIN: usize = 4;
/// Two contexts whose units differ by less than this (relative) state
/// the same unit (a file's `25.4` and `25.400000000000002`).
const SAME_UNIT: f64 = 1e-9;

/// Millimetres in one length unit of the file, read from every
/// representation context's `GLOBAL_UNIT_ASSIGNED_CONTEXT`.
///
/// `Ok(None)`: no context states a length unit; the caller keeps
/// millimetres, as every import did before. `Err`: a stated unit this
/// reader can't convert, or contexts that disagree. A refusal names
/// it, because guessing a part's scale is the silent wrong answer.
pub(super) fn mm_per_unit(section: &DataSection) -> Result<Option<f64>, String> {
    let records = by_id(section);
    let mut found: Option<f64> = None;
    for recs in records.values() {
        for r in recs
            .iter()
            .filter(|r| r.name == "GLOBAL_UNIT_ASSIGNED_CONTEXT")
        {
            let Some(Parameter::List(units)) = args(r).first() else {
                return Err("GLOBAL_UNIT_ASSIGNED_CONTEXT has no unit list".into());
            };
            for unit in units {
                let Parameter::Ref(Name::Entity(id)) = unit else {
                    continue;
                };
                let is_length = records
                    .get(id)
                    .is_some_and(|rs| rs.iter().any(|r| r.name == "LENGTH_UNIT"));
                if !is_length {
                    continue; // the angle and solid-angle units ride along
                }
                let k = length_mm(&records, *id, 0)?;
                match found {
                    Some(prev) if ((prev - k) / prev).abs() > SAME_UNIT => {
                        return Err(format!(
                            "the file states two length units ({prev} mm and {k} mm per unit)"
                        ));
                    }
                    _ => found = Some(k),
                }
            }
        }
    }
    Ok(found)
}

/// Every entity's records by id: a simple instance is one record, a
/// complex one (`#72 = ( CONVERSION_BASED_UNIT(..) LENGTH_UNIT() .. )`)
/// is its list.
pub(super) fn by_id(section: &DataSection) -> HashMap<u64, Vec<&Record>> {
    section
        .entities
        .iter()
        .map(|e| match e {
            EntityInstance::Simple { id, record } => (*id, vec![record]),
            EntityInstance::Complex { id, subsuper } => (*id, subsuper.0.iter().collect()),
        })
        .collect()
}

/// A record's arguments: the parser wraps them in one list.
pub(super) fn args(r: &Record) -> &[Parameter] {
    match &r.parameter {
        Parameter::List(a) => a,
        _ => &[],
    }
}

/// Millimetres in the length unit entity `#id`.
fn length_mm(records: &HashMap<u64, Vec<&Record>>, id: u64, depth: usize) -> Result<f64, String> {
    if depth > MAX_CHAIN {
        return Err(format!(
            "length unit #{id} chains through more than {MAX_CHAIN} units"
        ));
    }
    let recs = records
        .get(&id)
        .ok_or_else(|| format!("length unit #{id} is not in the file"))?;
    if let Some(si) = recs.iter().find(|r| r.name == "SI_UNIT") {
        return si_mm(id, args(si));
    }
    if let Some(conv) = recs.iter().find(|r| r.name == "CONVERSION_BASED_UNIT") {
        let Some(Parameter::Ref(Name::Entity(measure))) = args(conv).get(1) else {
            return Err(format!("conversion-based unit #{id} names no measure"));
        };
        let m = records
            .get(measure)
            .and_then(|rs| rs.iter().find(|r| r.name == "LENGTH_MEASURE_WITH_UNIT"))
            .ok_or_else(|| {
                format!("conversion-based unit #{id}: #{measure} is no length measure")
            })?;
        let (value, base) = match args(m) {
            [v, Parameter::Ref(Name::Entity(base)), ..] => (real(v), *base),
            _ => (None, 0),
        };
        let Some(value) = value.filter(|v| v.is_finite() && *v > 0.0) else {
            return Err(format!(
                "length measure #{measure} states no positive value"
            ));
        };
        return Ok(value * length_mm(records, base, depth + 1)?);
    }
    Err(format!(
        "length unit #{id} is neither an SI unit nor a conversion-based one"
    ))
}

/// `SI_UNIT(prefix, name)` for a length: the metre, with its prefix.
fn si_mm(id: u64, a: &[Parameter]) -> Result<f64, String> {
    let word = |p: Option<&Parameter>| match p {
        Some(Parameter::Enumeration(e)) => Some(e.trim_matches('.').to_ascii_uppercase()),
        _ => None,
    };
    if word(a.get(1)).as_deref() != Some("METRE") {
        return Err(format!("length unit #{id} is an SI unit but not the metre"));
    }
    // Millimetres per prefixed metre; no prefix (`$`) is the metre itself.
    match word(a.first()).as_deref() {
        None => Ok(1000.0),
        Some("KILO") => Ok(1.0e6),
        Some("DECI") => Ok(100.0),
        Some("CENTI") => Ok(10.0),
        Some("MILLI") => Ok(1.0),
        Some("MICRO") => Ok(1.0e-3),
        Some("NANO") => Ok(1.0e-6),
        Some(p) => Err(format!("length unit #{id}: SI prefix {p} is not supported")),
    }
}

/// A measure value: a bare real, or one typed as `LENGTH_MEASURE(..)`.
pub(super) fn real(p: &Parameter) -> Option<f64> {
    match p {
        Parameter::Real(x) => Some(*x),
        Parameter::Integer(i) => Some(*i as f64),
        Parameter::Typed { parameter, .. } => real(parameter),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Part 21 header as exporters write it: the reader's tokenizer
    /// refuses an empty HEADER section.
    const HEADER: &str = "HEADER;\nFILE_DESCRIPTION( ( '' ), '2;1' );\n\
                          FILE_NAME( 'units.step', '2026-11-08T00:00:00', ( '' ), ( '' ), '', '', '' );\n\
                          FILE_SCHEMA( ( 'AUTOMOTIVE_DESIGN' ) );\nENDSEC;";

    /// The units of a file whose DATA section is `data`.
    fn units(data: &str) -> Result<Option<f64>, String> {
        let text = format!("ISO-10303-21;\n{HEADER}\nDATA;\n{data}\nENDSEC;\nEND-ISO-10303-21;\n");
        let ex = ruststep::parser::parse(&text).expect("the test file parses");
        mm_per_unit(ex.data.first().expect("a data section"))
    }

    /// The angle and solid-angle units every exporter writes beside the
    /// length unit, so the reader is shown to skip them.
    const ANGLES: &str = "#73 = ( NAMED_UNIT( * ) PLANE_ANGLE_UNIT( ) SI_UNIT( $, .RADIAN. ) );\n\
                          #74 = ( NAMED_UNIT( * ) SI_UNIT( $, .STERADIAN. ) SOLID_ANGLE_UNIT( ) );";

    fn context(length: u64) -> String {
        format!(
            "#10 = ( GEOMETRIC_REPRESENTATION_CONTEXT( 3 ) GLOBAL_UNIT_ASSIGNED_CONTEXT( ( #{length}, #73, #74 ) ) REPRESENTATION_CONTEXT( '', '' ) );\n{ANGLES}"
        )
    }

    #[test]
    fn an_si_length_unit_reads_its_prefix() {
        let si = |prefix: &str| {
            units(&format!(
                "{}\n#72 = ( LENGTH_UNIT( ) NAMED_UNIT( * ) SI_UNIT( {prefix}, .METRE. ) );",
                context(72)
            ))
        };
        assert_eq!(si(".MILLI."), Ok(Some(1.0)));
        assert_eq!(si(".CENTI."), Ok(Some(10.0)));
        assert_eq!(si("$"), Ok(Some(1000.0)), "no prefix is the metre");
    }

    #[test]
    fn an_inch_unit_is_twenty_five_point_four_millimetres() {
        let data = format!(
            "{}\n#72 = ( CONVERSION_BASED_UNIT( 'INCH', #76 ) LENGTH_UNIT( ) NAMED_UNIT( #75 ) );\n\
             #75 = DIMENSIONAL_EXPONENTS( 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0 );\n\
             #76 = LENGTH_MEASURE_WITH_UNIT( LENGTH_MEASURE( 25.4 ), #77 );\n\
             #77 = ( LENGTH_UNIT( ) NAMED_UNIT( * ) SI_UNIT( .MILLI., .METRE. ) );",
            context(72)
        );
        let k = units(&data).expect("inch reads").expect("a length unit");
        assert!((k - 25.4).abs() < 1e-12, "{k}");
    }

    #[test]
    fn a_file_with_no_unit_context_keeps_millimetres() {
        assert_eq!(units(ANGLES), Ok(None));
    }

    #[test]
    fn contexts_that_disagree_or_a_unit_that_cannot_be_read_refuse_by_name() {
        let two = format!(
            "{}\n#11 = ( GLOBAL_UNIT_ASSIGNED_CONTEXT( ( #82 ) ) REPRESENTATION_CONTEXT( '', '' ) );\n\
             #72 = ( LENGTH_UNIT( ) NAMED_UNIT( * ) SI_UNIT( .MILLI., .METRE. ) );\n\
             #82 = ( LENGTH_UNIT( ) NAMED_UNIT( * ) SI_UNIT( $, .METRE. ) );",
            context(72)
        );
        assert!(
            units(&two).is_err_and(|e| e.contains("two length units")),
            "{:?}",
            units(&two)
        );
        let odd = format!(
            "{}\n#72 = ( LENGTH_UNIT( ) NAMED_UNIT( * ) SI_UNIT( .EXA., .METRE. ) );",
            context(72)
        );
        assert!(
            units(&odd).is_err_and(|e| e.contains("EXA")),
            "{:?}",
            units(&odd)
        );
    }
}
