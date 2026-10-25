use super::*;

pub(super) fn check_and_apply(world: &World, mut file: ProjectFile) -> Result<(), ProjectError> {
    // Migration hook: step UP one version at a time, each migration exact,
    // before any entity touches the world. Anything newer than this build
    // remains refused. The chain must run its steps sequentially - a v1
    // file passes through v2's effects before v3's are applied.
    if file.version == 1 {
        // v1 -> v2: entities gained `rotation`; v1 had translation-only
        // placements, so identity is the exact migration. Kind and stock
        // fields default (absent = model), so nothing else changes.
        for dto in &mut file.entities {
            dto.rotation = IDENTITY_Q;
        }
        file.version = 2;
    }
    if file.version == 2 {
        // v2 -> v3: `kind` gained tool/operation and their payload fields
        // appeared; every existing entity keeps its fields and defaults
        // the new ones (absent), which is exactly what a v2 file meant.
        file.version = 3;
    }
    if file.version == 3 {
        // v3 -> v4: feature picks graduate from "the src enum can carry
        // them" to "they persist". The shape is already right - a v3 file
        // with a selection reads verbatim, one without gains nothing -
        // so the step-up is the stamp itself.
        file.version = 4;
    }
    if file.version == 4 {
        // v4 -> v5: the M1.5.2 landing shape (patch picks, arc moves,
        // face stepdowns, pocket retract) is declared in the DTOs with
        // serde defaults that mean exactly the v4 behavior, so a v4
        // file needs no content change to live in v5 - the step-up is
        // the stamp itself, like v3 -> v4.
        file.version = 5;
    }
    if file.version == 5 {
        // v5 -> v6: heights became From+Offset expressions, and the
        // wire (`HeightWire`) already read a bare number as the exact
        // `Fixed` expression - every v5 height keeps its z to the bit.
        // Like v4 -> v5, the step-up is the stamp itself.
        file.version = 6;
    }
    if file.version == 6 {
        // v6 -> v7: contour gained the side choice, and the wire already
        // reads an absent `side` as `auto` - which is exactly what a v6
        // contour meant, since Auto resolves at generate time and the
        // stored path is unchanged. Step-up is the stamp itself.
        file.version = 7;
    }
    if file.version == 7 {
        // v7 -> v8: document units arrive as a top-level `units` field.
        // Every v7 file displayed millimeters (its numbers always were
        // millimeters), and the field's default - absent means mm - is
        // exactly that. Step-up is the stamp itself, like v6 -> v7.
        file.version = 8;
    }
    if file.version == 8 {
        // v8 -> v9: the Lead vocabulary grows a tangential member, and
        // `lead` has been a required key since v1 - no v8 file can
        // contain the new tag, and no v8 file needs a content change to
        // live in v9. Step-up is the stamp itself, like v7 -> v8.
        file.version = 9;
    }
    if file.version == 9 {
        // v9 -> v10: drilling arrives - the `drill` params variant, the
        // `drill`/`tap`/`bore` tool-kind tags, `tip_angle`/`pitch`, and
        // cached `cycles`. A v9 file can contain none of them (the tags
        // and keys did not exist), and the serde defaults for the two
        // optional-shaped additions (`cycles` -> empty, tool floats ->
        // the inert 0.0) are exactly v9's behavior. No content change;
        // step-up is the stamp itself, like every bump since v6.
        file.version = 10;
    }
    if file.version == 10 {
        // v10 -> v11: the 3-D pocket arrives as one new params variant
        // (`pocket3d`), which no v10 file can contain - the tag did not
        // exist. Nothing existing changed shape, so no content change;
        // step-up is the stamp itself, like every bump since v6.
        file.version = 11;
    }
    if file.version == 11 {
        // v11 -> v12: the 3-D pocket's rest pass arrives as one new
        // params key (`rest_rough`), absent from every v11 file - the
        // serde default is false, which is exactly what v11 generated.
        // Nothing existing changed shape; step-up is the stamp itself,
        // like every bump since v6.
        file.version = 12;
    }
    if file.version == 12 {
        // v12 -> v13: the constant-engagement strategy arrives as one
        // new params key (`max_engagement`) and one new `strategy` tag
        // (`adaptive`), neither of which any v12 file can contain - the
        // serde default of 45° is exactly what a v12 file meant, since
        // its rings were stepover-spaced and the angle reads only under
        // Adaptive. No content change; step-up is the stamp itself, like
        // every bump since v6.
        file.version = 13;
    }
    if file.version == 13 {
        // v13 -> v14: the feed modulation arrives as one new params key
        // (`feed_opt`), absent from every v13 file - the serde default
        // is off, which is exactly what v13 generated (every F word at
        // the programmed feed). No content change; step-up is the stamp
        // itself, like every bump since v6.
        file.version = 14;
    }
    if file.version == 14 {
        // v14 -> v15: the 3-D finish arrives as one new params variant
        // (`finish3d`), which no v14 file can contain - the tag did not
        // exist. Nothing existing changed shape; step-up is the stamp
        // itself, like every bump since v6.
        file.version = 15;
    }
    if file.version == 15 {
        // v15 -> v16: the parallel finish arrives as two new keys
        // inside `finish3d` (`pattern`, `stepover`), absent from every
        // v15 file - the serde defaults (`waterline`, derived
        // stepover) are exactly what v15 ran. No content change;
        // step-up is the stamp itself, like every bump since v6.
        file.version = 16;
    }
    if file.version == 16 {
        // v16 -> v17: one more new key inside `finish3d` - `ramp`,
        // absent from every v16 file, whose serde default (false: the
        // hop) is exactly what v16 ran. Same pure step-up, same law.
        file.version = 17;
    }
    if file.version == 17 {
        // v17 -> v18: the surface-finish pair inside `finish3d` -
        // `min_step` (absent: None, the march's own floor) and
        // `ramp_angle` (absent: 45°, the slope limit the raster's
        // jurisdiction already granted). Both defaults are exactly
        // what v17 ran; no content change. Same pure step-up, same
        // law.
        file.version = 18;
    }
    if file.version == 18 {
        // v18 -> v19: the machine arrives as a top-level `machine` key.
        // No v18 file can contain it - the checker did not exist - and
        // the field's default, absent means no machine, is exactly what
        // every v18 document was: a program judged against nothing,
        // which is what the unstated-limit law already does with an
        // empty machine. Step-up is the stamp itself, like v7 -> v8.
        file.version = 19;
    }
    if file.version == 19 {
        // v19 -> v20: the model `rev` key (M3.1's revision counter). No
        // v19 file can contain it - the component did not exist - and the
        // field's default, absent means 0, is exactly what every v19
        // document was: geometry with no remembered landings. The
        // staleness machinery needs nothing migrated - it recomputes the
        // signature from the persisted facets. Step-up is the stamp
        // itself, like v7 -> v8 and v18 -> v19.
        file.version = 20;
    }
    if file.version == 20 {
        // v20 -> v21: setups arrive (M3.2 stage A) as a `setup` kind, a
        // `setup` name inside each operation, and a top-level
        // `active_setup` key. No v20 file can contain any of them - the
        // kind tag did not exist - and the defaults read as exactly the
        // v20 world: no setup rows and no setup names is the implicit-
        // setup law's legacy mode verbatim, and an absent active choice
        // resolves to the first setup by name (nothing). No content
        // change; step-up is the stamp itself, like every bump since v6.
        file.version = 21;
    }
    if file.version == 21 {
        // v21 -> v22: fixtures arrive (M3.2 stage B) as an optional
        // `fixture` marker on model entities - a tag, not a new kind,
        // because a fixture IS the model the user loaded. No v21 file
        // can contain it: the component did not exist, and its default
        // (no marker) is exactly the v21 collision world. Step-up is
        // the stamp itself, like every bump since v6.
        file.version = 22;
    }
    if file.version == 22 {
        // v22 -> v23: the post choice arrives as one optional top-level
        // `post` key (M3.3 stage A). No v22 file can contain it - the
        // component did not exist, and its default (absent, the
        // built-in) is exactly the v22 posting world: one embedded
        // script, no choices. Step-up is the stamp itself, like every
        // bump since v6.
        file.version = 23;
    }
    if file.version == 23 {
        // v23 -> v24: the 5-axis twins arrive as two new params
        // variants (`contour5d`, `parallel5d`, M5.1 stage C), which no
        // v23 file can contain - the kind tags did not exist, and
        // `validate_kind` already refuses an unknown params tag before
        // anything is written. No existing shape changed; step-up is
        // the stamp itself, like every bump since v6.
        file.version = 24;
    }
    if file.version == 24 {
        // v24 -> v25: the swarf family arrives as two new params
        // variants (`swarf`, `advanced_swarf`, M5.1 stage D), under
        // the same law as the twins above: no v24 file can contain
        // them, and `validate_kind` already refuses the tags before
        // anything is written. No existing shape changed.
        file.version = 25;
    }
    if file.version == 25 {
        // v25 -> v26: the rotary twins arrive as two new params
        // variants (`rotary_contour`, `rotary_parallel`, M5.1 stage
        // E), under the same law: no v25 file can contain them, and
        // `validate_kind` already refuses the tags before anything is
        // written. No existing shape changed - the fitted axis is
        // model data, so the file gains no key for it.
        file.version = 26;
    }
    if file.version == 26 {
        // v26 -> v27: the rotary rough arrives as one new params
        // variant (`rotary_offset`, M5.1 stage E2), under the same
        // law: no v26 file can contain it, and `validate_kind`
        // already refuses the tag before anything is written. No
        // existing shape changed.
        file.version = 27;
    }
    if file.version != PROJECT_VERSION {
        return Err(ProjectError::UnsupportedVersion(file.version));
    }
    // The document's unit first (it belongs to the whole file, not to an
    // entity): an unspellable value is a corrupt file, refused before any
    // world write. Opening always lands the file's value - including mm,
    // so a stale singleton from the previous document can never leak in.
    let doc_units = match file.units.as_deref() {
        None | Some("mm") => Unit::Millimeters,
        Some("in") => Unit::Inches,
        Some(other) => return Err(corrupt(format!("unknown units {other:?}"))),
    };
    // Whole-file validation BEFORE any world write: a corrupt entity deeper
    // in the list must not leave a half-loaded document behind.
    for dto in &file.entities {
        validate_kind(dto, dto.kind.unwrap_or(EntityKind::Model))?;
    }
    if let Some(m) = &file.machine
        && !m.machine.is_sane()
    {
        return Err(corrupt("machine definition states a limit that cannot be meant"));
    }
    if file.active_setup.as_deref().is_some_and(str::is_empty) {
        return Err(corrupt("the active setup names nothing - an empty setup name"));
    }
    units::set(world, doc_units);
    // Opening lands the machine too - including the mute state, so a
    // stale singleton from the previous document can never leak in
    // (the units law, restated for the checker's input).
    match file.machine.as_ref() {
        Some(m) => crate::machine::set(world, &m.path, Some(&m.machine)),
        None => crate::machine::set(world, "", None),
    }
    // Opening lands the active-setup choice too - including None, so a
    // stale singleton from the previous document can never leak in (the
    // units law, restated for setups). A name matching no setup row is
    // legal: `setup::active`'s existence fallback is the same law the
    // live world uses mid-history, so the file reads as it was saved.
    crate::setup::set_active(world, file.active_setup);
    // Opening lands the post choice too - including the default, so a
    // stale singleton from the previous document can never leak in (the
    // units law again). Paths and registry names are landed as written,
    // unvalidated: each names a script, not a limit, and the post-time
    // refusal is the net (`script_source` reads both, and both refuse
    // by name when gone).
    crate::posting::post_set(
        world,
        &match file.post {
            Some(PostDto::Named(name)) => crate::schema::PostRef::Named(name),
            Some(PostDto::File(path)) => crate::schema::PostRef::File(path),
            None => crate::schema::PostRef::Builtin,
        },
    );
    for dto in file.entities {
        let kind = dto.kind.unwrap_or(EntityKind::Model);
        let entity = model::spawn(world, &dto.name);
        model::set_transform(
            world,
            &entity,
            Location {
                pos: Point3::new(dto.translation[0], dto.translation[1], dto.translation[2]),
                rot: dto.rotation,
            },
        );
        match kind {
            EntityKind::Model => {
                if let Some(tris) = dto.facets {
                    model::set_facets(world, &entity, dto_to_facets(tris));
                }
                // 0 means the key was absent, i.e. a pre-v20 document: no
                // counter component is the world a v19 loader would have
                // built, and `rev_of` reads it as 0 anyway. A model first
                // updated after such a load lands rev 1 - the counter
                // counts landings this document remembers, not the file's
                // whole history. Staleness never reads it; the stamp's
                // signature does the work.
                if dto.rev > 0 {
                    model::set_rev(world, &entity, dto.rev);
                }
            }
            EntityKind::Stock => {
                // validate_kind already refused these; Result, never unwrap.
                let shape = dto
                    .stock
                    .ok_or_else(|| corrupt("stock entity without parameters"))?
                    .to_shape();
                let facets = match &shape {
                    StockShape::FromModel { .. } => {
                        dto_to_facets(dto.facets.ok_or_else(|| {
                            corrupt("from-model stock is missing its facets")
                        })?)
                    }
                    _ => stock::stock_mesh(&shape)
                        .filter(|f| !f.is_empty())
                        .ok_or_else(|| corrupt("stock mesh regeneration failed"))?,
                };
                model::set_facets(world, &entity, facets);
                entity
                    .entity_view(world)
                    .set(StockDef { shape: shape.clone() });
            }
            EntityKind::Wcs => {
                entity.entity_view(world).add(Wcs);
            }
            EntityKind::Tool => {
                // validate_kind already refused a missing or insane one.
                let tool = dto
                    .tool
                    .ok_or_else(|| corrupt("tool entity without a definition"))?
                    .to_tool();
                entity.entity_view(world).set(ToolDef { tool });
            }
            EntityKind::Operation => {
                let odto = dto
                    .operation
                    .ok_or_else(|| corrupt("operation entity without a definition"))?;
                let src = match &odto.src {
                    SrcDto::Stock => SrcRef::Stock,
                    SrcDto::Entity { name } => SrcRef::Entity(name.clone()),
                    SrcDto::Selection { items } => SrcRef::Selection(
                        items
                            .iter()
                            .map(|g| GeoSel {
                                entity: g.entity.clone(),
                                feature: match g.feature {
                                    FeatureSelDto::Face { id } => FeatureSel::Face(id),
                                    FeatureSelDto::Chain { id } => FeatureSel::Chain(id),
                                    FeatureSelDto::Patch { id } => FeatureSel::Patch(id),
                                },
                            })
                            .collect(),
                    ),
                };
                entity.entity_view(world).set(Operation {
                    tool: odto.tool,
                    src,
                    params: odto.params.to_params(),
                    stamp: odto.stamp.map(GenStamp),
                    setup: odto.setup,
                });
                if let Some(cache) = &odto.cache {
                    entity.entity_view(world).set(cache.to_cache());
                }
            }
            EntityKind::Setup => {
                // validate_kind already refused a missing one.
                let sdto = dto
                    .setup
                    .ok_or_else(|| corrupt("setup entity without a definition"))?;
                entity.entity_view(world).set(SetupDef {
                    stock: sdto.stock,
                    wcs: sdto.wcs,
                    datum_from: sdto.datum_from,
                });
            }
        }
        // v22: the fixture marker rides a finished model row (the kind
        // match above built the solid; validate_kind refused the tag on
        // anything else). Setting it after keeps the geometry-first
        // order of every other load.
        if let Some(f) = &dto.fixture {
            entity.entity_view(world).set(FixtureDef {
                setup: f.setup.clone(),
            });
        }
    }
    Ok(())
}
