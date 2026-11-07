use super::*;

pub(super) fn identity_rotation() -> [f64; 4] {
    IDENTITY_Q
}

pub(super) fn is_unrevised(rev: &u64) -> bool {
    *rev == 0
}

pub(super) fn corrupt(msg: impl Into<String>) -> ProjectError {
    ProjectError::Corrupt(msg.into())
}

pub(super) fn facets_to_dto(tris: &[Triangle]) -> Vec<[[f64; 3]; 3]> {
    tris.iter()
        .map(|t| {
            [
                [t.a.x, t.a.y, t.a.z],
                [t.b.x, t.b.y, t.b.z],
                [t.c.x, t.c.y, t.c.z],
            ]
        })
        .collect()
}

pub(super) fn dto_to_facets(v: Vec<[[f64; 3]; 3]>) -> Vec<Triangle> {
    v.into_iter()
        .map(|t| {
            Triangle::new(
                Point3::new(t[0][0], t[0][1], t[0][2]),
                Point3::new(t[1][0], t[1][1], t[1][2]),
                Point3::new(t[2][0], t[2][1], t[2][2]),
            )
        })
        .collect()
}

pub fn collect(world: &World) -> ProjectFile {
    let entities = model::named_entities(world)
        .into_iter()
        .map(|(name, entity)| {
            let loc = model::transform_of(world, &entity).unwrap_or_default();
            let kind = if model::is_stock(world, &entity) {
                Some(EntityKind::Stock)
            } else if model::is_wcs(world, &entity) {
                Some(EntityKind::Wcs)
            } else if model::is_tool(world, &entity) {
                Some(EntityKind::Tool)
            } else if model::is_operation(world, &entity) {
                Some(EntityKind::Operation)
            } else if model::is_setup(world, &entity) {
                Some(EntityKind::Setup)
            } else {
                None
            };
            let stock = model::stock_def_of(world, &entity).map(|s| StockDto::from_shape(&s));
            let tool = model::tool_of(world, &entity).map(|t| ToolDto::from_tool(&t));
            // The setup payload only ever rides a setup row (the kind test
            // above and the component below are the same SetupDef).
            let setup = if matches!(kind, Some(EntityKind::Setup)) {
                entity
                    .entity_view(world)
                    .try_get::<&SetupDef>(|d| SetupDto {
                        stock: d.stock.clone(),
                        wcs: d.wcs.clone(),
                        datum_from: d.datum_from.clone(),
                    })
            } else {
                None
            };
            // The fixture marker rides whatever model it tags; the kind
            // test above deliberately does NOT look at it (a fixture is
            // a model, not a kind).
            let fixture = entity
                .entity_view(world)
                .try_get::<&FixtureDef>(|d| FixtureDto {
                    setup: d.setup.clone(),
                    fixed: d.fixed,
                });
            let operation = model::operation_of(world, &entity).map(|op| OperationDto {
                label: model::operation_meta_of(world, &entity).label,
                order: model::operation_meta_of(world, &entity).order,
                tool: op.tool,
                src: match &op.src {
                    SrcRef::Stock => SrcDto::Stock,
                    SrcRef::Entity(name) => SrcDto::Entity { name: name.clone() },
                    SrcRef::Selection(items) => SrcDto::Selection {
                        items: items
                            .iter()
                            .map(|g| GeoSelDto {
                                entity: g.entity.clone(),
                                feature: match g.feature {
                                    FeatureSel::Face(id) => FeatureSelDto::Face { id },
                                    FeatureSel::Chain(id) => FeatureSelDto::Chain { id },
                                    FeatureSel::Patch(id) => FeatureSelDto::Patch { id },
                                    FeatureSel::Vertex(id) => FeatureSelDto::Vertex { id },
                                },
                            })
                            .collect(),
                    },
                },
                params: ParamsDto::from_params(&op.params),
                stamp: op.stamp.map(|g| g.0),
                cache: model::toolpath_of(world, &entity).map(|c| PathDto::from_cache(&c)),
                area: AreaDto::from_controls(&op.area),
                setup: op.setup,
            });
            // Parametric stock regenerates its facets on load; offsets, tools,
            // operations and setups never have geometry, so none writes a
            // facet array.
            let facets = match &kind {
                Some(EntityKind::Wcs)
                | Some(EntityKind::Tool)
                | Some(EntityKind::Operation)
                | Some(EntityKind::Setup) => None,
                Some(EntityKind::Stock)
                    if matches!(
                        stock,
                        Some(StockDto::Box { .. }) | Some(StockDto::Cylinder { .. })
                    ) =>
                {
                    None
                }
                _ => model::facets_of(world, &entity).map(|t| facets_to_dto(&t)),
            };
            EntityDto {
                name,
                translation: [loc.pos.x, loc.pos.y, loc.pos.z],
                rotation: loc.rot,
                facets,
                // The counter is only ever set on models (import/update
                // commands); everywhere else it reads 0 and omits itself.
                rev: model::rev_of(world, &entity),
                kind,
                stock,
                tool,
                operation,
                setup,
                fixture,
            }
        })
        .collect();
    ProjectFile {
        version: PROJECT_VERSION,
        // mm omits the field (the default); only an inch document pays
        // for saying so.
        units: match units::of(world) {
            Unit::Millimeters => None,
            Unit::Inches => Some("in".into()),
        },
        // The mute singleton (no path, no machine) omits the field -
        // the mm/units law again: only a loaded machine pays for
        // saying so, and an absent key is the mute state verbatim.
        machine: {
            let (path, m) = crate::machine::of(world);
            m.map(|machine| MachineDto { path, machine })
        },
        // The raw singleton, not the resolved one: a document where the
        // user never chose a setup writes no key, exactly like the mm
        // units and mute machine laws - and a name left dangling by a
        // deleted setup saves as it reads (the fallback is the law, not
        // a defect to hide).
        active_setup: crate::setup::stored_active(world),
        // The default post matches every pre-v23 document, so it
        // writes no key (the mute-machine law, worn in the post's clothes).
        post: match crate::posting::post_of(world) {
            crate::schema::PostRef::Builtin => None,
            crate::schema::PostRef::Named(name) => Some(PostDto::Named(name)),
            crate::schema::PostRef::File(path) => Some(PostDto::File(path)),
        },
        // The default theme matches every pre-v41 document, so it
        // writes no key (the post's law, worn in the theme's clothes).
        theme: match crate::commands::theme_of(world) {
            crate::schema::ThemeRef::Dark => None,
            crate::schema::ThemeRef::Named(name) => Some(ThemeDto::Named(name)),
            crate::schema::ThemeRef::File(path) => Some(ThemeDto::File(path)),
        },
        // An unpinned ribbon writes no key (the theme's law, worn in
        // the pin's clothes - and the default is what every pre-M6.2
        // document is).
        pinned: crate::ribbon::pinned_of(world),
        // No stated material writes no key (the theme's law again).
        material: crate::cutting::material_of(world).map(|m| m.word().to_owned()),
        entities,
    }
}

pub fn save(world: &World, path: &Path) -> Result<(), ProjectError> {
    let file = collect(world);
    let text = serde_json::to_string_pretty(&file).map_err(|e| corrupt(e.to_string()))?;
    std::fs::write(path, text)?;
    Ok(())
}

/// Rebuild world content from a project file. Not a command: opening a
/// project replaces the document and clears the history (M1.2 wires that).
pub fn load(world: &World, path: &Path) -> Result<(), ProjectError> {
    let text = std::fs::read_to_string(path)?;
    open(world, &text)
}

pub fn open(world: &World, text: &str) -> Result<(), ProjectError> {
    let file: ProjectFile = serde_json::from_str(text).map_err(|e| corrupt(e.to_string()))?;
    check_and_apply(world, file)
}
