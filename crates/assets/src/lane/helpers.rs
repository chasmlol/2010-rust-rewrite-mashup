use std::collections::HashMap;

use bevy::prelude::{Mat3, Quat, Transform, Vec3};
use fastfile_iw4::{Ptr, ZoneStream};

use asset_material::MaterialCatalog;
use asset_world::{
    MapXModelAssetKey, MapXModelSceneAsset, MapXModelSceneCatalog, ModelMesh, PreparedMapModels,
    ScriptModelMetadata, ScriptModelPlacement, ScriptModelSceneInstance, StaticModelDraw,
    StaticModelDrawError, StaticModelInstance, StaticModelPlacement,
    build_iw5_static_model_instances, build_iw5_xmodel_mesh, build_static_model_instances,
    build_t5_static_model_instances, build_t5_xmodel_mesh, build_xmodel_mesh, flag_descriptors,
    flag_descriptors_iw5, flag_descriptors_t5, map_script_structs, map_script_structs_iw5,
    map_script_structs_t5, map_use_triggers_iw5, map_use_triggers_t5,
    script_brush_model_placements, script_brush_model_placements_iw5,
    script_brush_model_placements_t5, script_model_placements, script_model_placements_iw5,
    script_model_placements_t5,
};

#[derive(Default)]
pub(crate) struct MapXModelCatalog {
    pub shared_surfaces: asset_model::SharedXModelSurfaces,
    meshes: Vec<ModelMesh>,
    direct: HashMap<Ptr, usize>,
    aliases: HashMap<Ptr, Ptr>,
    failed: usize,
    scene_assets: MapXModelSceneCatalog,

    t5_destructibles: HashMap<String, Result<T5DestructibleInitial, &'static str>>,
}

struct T5DestructibleInitial {
    definition: std::sync::Arc<xmodel_runtime::T5DestructibleDef>,
    model: MapXModelAssetKey,
    hide_parts: xmodel_runtime::HidePartBits,
}

impl MapXModelCatalog {
    pub(crate) fn take_named_mesh(&mut self, name: &str) -> Option<ModelMesh> {
        let index = self.meshes.iter().position(|mesh| mesh.name == name)?;
        self.direct.retain(|_, slot| *slot != index);
        for slot in self.direct.values_mut() {
            if *slot > index {
                *slot -= 1;
            }
        }
        Some(self.meshes.remove(index))
    }

    pub(crate) fn set_capture_zone(&mut self, zone: asset_core::ZoneOwner) {
        self.scene_assets.set_capture_zone(zone);
    }

    pub(crate) fn capture(
        &mut self,
        stream: &ZoneStream<'_>,
        materials: &MaterialCatalog,
        slot: Ptr,
        insert_slot: Option<Ptr>,
        strings: &fastfile_iw4::ScriptStrings,
        phys_presets: &asset_world::PhysPresetCatalog,
    ) {
        let Some(geometry) = stream.xmodel() else {
            self.failed += 1;
            return;
        };
        let slot_live = geometry.phys_preset.is_some();
        let named = geometry
            .phys_preset
            .and_then(|preset_slot| phys_presets.at_slot(preset_slot))
            .is_some();
        self.scene_assets.note_xmodel_phys_preset(slot_live, named);
        let Some(name) = geometry.name.and_then(|p| stream.cstr(p).ok()) else {
            self.failed += 1;
            return;
        };
        let scene_key = MapXModelAssetKey(name.to_owned());
        let scene_asset = asset_model::capture_xmodel_skel_with_shared(
            stream,
            strings,
            geometry,
            Some(materials),
            &self.shared_surfaces,
        )
        .map(|skel| MapXModelSceneAsset::Iw4(std::sync::Arc::new(skel)))
        .unwrap_or(MapXModelSceneAsset::Unavailable {
            reason: "IW4 XModel scene skeleton capture failed",
        });
        self.scene_assets.insert(scene_key, scene_asset);
        let Ok(mesh) = build_xmodel_mesh(stream, geometry, Some(materials)) else {
            self.failed += 1;
            return;
        };
        let index = self.meshes.len();
        self.meshes.push(mesh);
        self.direct.insert(slot, index);
        if let Some(insert_slot) = insert_slot {
            self.direct.insert(insert_slot, index);
        }
    }

    pub(crate) fn capture_t5(
        &mut self,
        stream: &fastfile_t5::ZoneStream<'_>,
        materials: &MaterialCatalog,
        slot: fastfile_t5::Ptr,
        insert_slot: Option<fastfile_t5::Ptr>,
        strings: &fastfile_t5::ScriptStrings,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            self.failed += 1;
            return;
        };
        let Ok(mesh) = build_t5_xmodel_mesh(stream, geometry, Some(materials)) else {
            self.failed += 1;
            return;
        };
        let scene_key = MapXModelAssetKey(mesh.name.clone());
        let scene_asset = asset_model::capture_xmodel_skel_t5(stream, strings, geometry, materials)
            .map(|skel| MapXModelSceneAsset::T5(std::sync::Arc::new(skel)))
            .unwrap_or(MapXModelSceneAsset::Unavailable {
                reason: "T5 XModel scene skeleton capture failed",
            });
        self.scene_assets.insert(scene_key, scene_asset);
        let index = self.meshes.len();
        self.meshes.push(mesh);
        let slot = Ptr {
            block: slot.block,
            offset: slot.offset,
        };
        self.direct.insert(slot, index);
        if let Some(insert_slot) = insert_slot {
            self.direct.insert(
                Ptr {
                    block: insert_slot.block,
                    offset: insert_slot.offset,
                },
                index,
            );
        }
    }

    pub(crate) fn capture_t5_destructible(
        &mut self,
        stream: &fastfile_t5::ZoneStream<'_>,
        header: fastfile_t5::Ptr,
        strings: &fastfile_t5::ScriptStrings,
        fx: &asset_game::FxCatalog,
    ) -> fastfile_t5::Result<()> {
        use fastfile_t5::ZonePtr;
        let ZonePtr::Offset(name) = stream.ptr_at(header, 0)? else {
            return Ok(());
        };
        let name = stream.cstr(stream.resolve_alias(name))?.to_owned();
        let initial = (|| {
            let pristine = stream.ptr_at(header, 8).ok()? != ZonePtr::Null;
            let model_slot = header.at(if pristine { 8 } else { 4 });
            let model = self
                .name_at_slot(Ptr {
                    block: model_slot.block,
                    offset: model_slot.offset,
                })?
                .to_owned();
            let read_string = |p, offset| -> Option<Option<String>> {
                match stream.ptr_at(p, offset).ok()? {
                    ZonePtr::Null => Some(None),
                    ZonePtr::Offset(p) => {
                        Some(Some(stream.cstr(stream.resolve_alias(p)).ok()?.to_owned()))
                    }
                    _ => None,
                }
            };
            let count = usize::try_from(stream.i32_at(header, 12).ok()?).ok()?;
            let ZonePtr::Offset(pieces_ptr) = stream.ptr_at(header, 16).ok()? else {
                return None;
            };
            let pieces_ptr = stream.resolve_alias(pieces_ptr);
            let mut pieces = Vec::with_capacity(count);
            for i in 0..count {
                let p = pieces_ptr.at(i * 0x138);
                let mut stages = Vec::with_capacity(5);
                for j in 0..5 {
                    let st = p.at(j * 0x30);
                    let bone = stream.u16_at(st, 0).ok()?;
                    let break_effect = if stream.ptr_at(st, 16).ok()? == ZonePtr::Null {
                        None
                    } else {
                        Some(
                            fx.name_at_slot(Ptr {
                                block: st.block,
                                offset: st.offset + 16,
                            })?
                            .to_owned(),
                        )
                    };
                    let mut spawn_models = [None, None, None];
                    for (k, value) in spawn_models.iter_mut().enumerate() {
                        if stream.ptr_at(st, 32 + k * 4).ok()? != ZonePtr::Null {
                            *value = Some(
                                self.name_at_slot(Ptr {
                                    block: st.block,
                                    offset: st.at(32 + k * 4).offset,
                                })?
                                .to_owned(),
                            );
                        }
                    }
                    stages.push(xmodel_runtime::T5DestructibleStage {
                        show_bone: if bone == 0 {
                            None
                        } else {
                            Some(strings.get(stream, bone)?.to_owned())
                        },
                        break_health: stream.f32_at(st, 4).ok()?,
                        max_time: stream.f32_at(st, 8).ok()?,
                        flags: stream.u32_at(st, 12).ok()?,
                        break_effect,
                        break_sound: read_string(st, 20)?,
                        break_notify: read_string(st, 24)?,
                        loop_sound: read_string(st, 28)?,
                        has_phys_preset: stream.ptr_at(st, 44).ok()? != ZonePtr::Null,
                        spawn_models,
                    });
                }
                let mut hide_bones = [0; 5];
                for (j, word) in hide_bones.iter_mut().enumerate() {
                    *word = stream.u32_at(p, 0x124 + j * 4).ok()?;
                }
                pieces.push(xmodel_runtime::T5DestructiblePiece {
                    stages: stages.try_into().ok()?,
                    parent_piece: stream.slice_at(p, 0xf0, 1).ok()?[0],
                    parent_damage_percent: stream.f32_at(p, 0xf4).ok()?,
                    bullet_damage_scale: stream.f32_at(p, 0xf8).ok()?,
                    explosive_damage_scale: stream.f32_at(p, 0xfc).ok()?,
                    health: stream.i32_at(p, 0x110).ok()?,
                    launch: match stream.ptr_at(p, 0x10c).ok()? {
                        ZonePtr::Offset(c) => {
                            let c = stream.resolve_alias(c);
                            let data = c.at(8);
                            if stream.u32_at(c, 4).ok()? > 0 && stream.i32_at(data, 4).ok()? == 7 {
                                Some((
                                    [
                                        stream.f32_at(data, 116).ok()?,
                                        stream.f32_at(data, 120).ok()?,
                                        stream.f32_at(data, 124).ok()?,
                                    ],
                                    stream.f32_at(data, 112).ok()?,
                                ))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    },
                    hide_bones,
                });
            }
            let base_slot = header.at(4);
            let definition = std::sync::Arc::new(xmodel_runtime::T5DestructibleDef {
                name: name.clone(),
                model: self
                    .name_at_slot(Ptr {
                        block: base_slot.block,
                        offset: base_slot.offset,
                    })?
                    .to_owned(),
                pieces,
                client_only: stream.i32_at(header, 20).ok()? != 0,
            });
            let hide_parts = if pristine {
                xmodel_runtime::HidePartBits::from_words([0; 6])
            } else {
                let MapXModelSceneAsset::T5(skel) = self.scene_assets.get_name(&model)? else {
                    return None;
                };
                let health: Vec<_> = definition.pieces.iter().map(|p| p.health as i16).collect();
                definition.hide_parts(&health, &skel.bone_names)
            };
            Some(T5DestructibleInitial {
                definition,
                model: MapXModelAssetKey(model),
                hide_parts,
            })
        })()
        .ok_or("T5 DestructibleDef initial model/parts capture failed");
        self.t5_destructibles.insert(name, initial);
        Ok(())
    }

    pub(crate) fn capture_iw5(
        &mut self,
        stream: &fastfile_iw5::ZoneStream<'_>,
        materials: &MaterialCatalog,
        slot: fastfile_iw5::Ptr,
        insert_slot: Option<fastfile_iw5::Ptr>,
        strings: &fastfile_iw5::ScriptStrings,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            self.failed += 1;
            return;
        };
        let Some(name) = geometry.name.and_then(|p| stream.cstr(p).ok()) else {
            self.failed += 1;
            return;
        };
        let scene_key = MapXModelAssetKey(name.to_owned());
        let scene_asset = asset_model::capture_xmodel_skel_iw5_with_shared(
            stream,
            strings,
            geometry,
            materials,
            &self.shared_surfaces,
        )
        .map(|skel| MapXModelSceneAsset::Iw5(std::sync::Arc::new(skel)))
        .unwrap_or(MapXModelSceneAsset::Unavailable {
            reason: "IW5 XModel scene skeleton capture failed",
        });
        self.scene_assets.insert(scene_key, scene_asset);
        let Ok(mesh) = build_iw5_xmodel_mesh(stream, geometry, Some(materials)) else {
            self.failed += 1;
            return;
        };
        let index = self.meshes.len();
        self.meshes.push(mesh);
        let slot = Ptr {
            block: slot.block,
            offset: slot.offset,
        };
        self.direct.insert(slot, index);
        if let Some(insert_slot) = insert_slot {
            self.direct.insert(
                Ptr {
                    block: insert_slot.block,
                    offset: insert_slot.offset,
                },
                index,
            );
        }
    }

    pub(crate) fn alias(&mut self, slot: Ptr, target: Ptr) {
        self.aliases.insert(slot, target);
    }

    pub(crate) fn mesh_index(&self, slot: Ptr) -> Option<usize> {
        let mut current = slot;
        for _ in 0..self.aliases.len().saturating_add(1) {
            if let Some(&index) = self.direct.get(&current) {
                return Some(index);
            }
            current = *self.aliases.get(&current)?;
        }
        None
    }

    pub(crate) fn name_at_slot(&self, slot: Ptr) -> Option<&str> {
        let index = self.mesh_index(slot)?;
        Some(self.meshes[index].name.as_str())
    }

    pub(crate) fn attach_t5_clip_models(
        &self,
        stream: &fastfile_t5::ZoneStream<'_>,
        geometry: fastfile_t5::ClipMapGeometry,
        clip: &mut asset_world::ClipCollision,
    ) -> Result<(), asset_world::ClipCollisionError> {
        use asset_world::ClipCollisionError::{MissingTables, Truncated};
        let Some(rows) = geometry.static_models else {
            return if geometry.static_model_count == 0 {
                Ok(())
            } else {
                Err(MissingTables)
            };
        };
        for index in 0..geometry.static_model_count {
            let row = rows.at(index * fastfile_t5::size::C_STATIC_MODEL);
            let slot = row.at(fastfile_t5::size::C_STATIC_MODEL_XMODEL_OFF);
            let name = self
                .name_at_slot(Ptr {
                    block: slot.block,
                    offset: slot.offset,
                })
                .ok_or(MissingTables)?;
            let Some(MapXModelSceneAsset::T5(skel)) = self.scene_assets.get_name(name) else {
                return Err(MissingTables);
            };
            let read = |offset| stream.f32_at(row, offset).map_err(|_| Truncated);
            let origin = [read(8)?, read(12)?, read(16)?];
            let mut inv_scaled_axis = [[0.0; 3]; 3];
            let mut bounds_mid = [0.0; 3];
            let mut bounds_half = [0.0; 3];

            for axis in 0..3 {
                for (column, value) in inv_scaled_axis[axis].iter_mut().enumerate() {
                    *value = read(20 + (axis * 3 + column) * 4)?;
                }
                let min = read(56 + axis * 4)?;
                let max = read(68 + axis * 4)?;
                if !min.is_finite() || !max.is_finite() || min > max {
                    return Err(Truncated);
                }
                bounds_mid[axis] = (min + max) * 0.5;
                bounds_half[axis] = (max - min) * 0.5;
            }
            let coll = clipmap_iw4::XModelColl {
                coll_lod: skel.coll_lod,
                contents: skel.contents.ok_or(MissingTables)?,
                surfs: skel
                    .coll_surfs
                    .iter()
                    .map(|surf| clipmap_iw4::XModelCollSurf {
                        midpoint: surf.midpoint,
                        half_size: surf.half_size,
                        bone_idx: i32::from(surf.bone),
                        contents: surf.contents,
                        surf_flags: surf.surf_flags,
                        tris: surf
                            .tris
                            .iter()
                            .map(|tri| clipmap_iw4::XModelCollTri {
                                plane: tri.plane,
                                svec: tri.svec,
                                tvec: tri.tvec,
                            })
                            .collect(),
                    })
                    .collect(),
            };
            clip.static_models.push(asset_world::ClipPlacedStaticModel {
                index: index as u32,
                name: name.to_owned(),
                model: clipmap_iw4::ClipStaticModel {
                    origin,
                    inv_scaled_axis,
                    bounds_mid,
                    bounds_half,
                    coll,
                },
            });
        }
        Ok(())
    }

    pub(crate) fn phys_preset_slot_n(&self) -> usize {
        self.scene_assets.phys_preset_slot_n()
    }

    pub(crate) fn phys_preset_name_hint_n(&self) -> usize {
        self.scene_assets.phys_preset_name_hint_n()
    }

    pub(crate) fn captured_model_n(&self) -> usize {
        self.scene_assets.len()
    }

    pub(crate) fn set_dynent_phys_preset_n(&mut self, n: usize) {
        self.scene_assets.set_dynent_phys_preset_n(n);
    }
}

pub(crate) fn build_static_model_draw(
    stream: &ZoneStream<'_>,
    geometry: fastfile_iw4::GfxWorldGeometry,
    catalog: MapXModelCatalog,
) -> PreparedMapModels {
    let (placements, static_error) = match build_static_model_instances(stream, geometry) {
        Ok(placements) => (placements, None),
        Err(_) => (
            Vec::new(),
            Some(StaticModelDrawError::InstancesUnavailable {
                smodel_count: geometry.smodel_count,
            }),
        ),
    };
    let scripts = script_model_placements(stream);
    let brushes = script_brush_model_placements(stream);
    let descriptors = flag_descriptors(stream);
    let structs = map_script_structs(stream);
    link_model_placements(
        placements,
        static_error,
        &scripts,
        brushes,
        descriptors,
        structs,
        catalog,
        geometry.smodel_count,
    )
}

pub(crate) fn build_t5_static_model_draw(
    stream: &fastfile_t5::ZoneStream<'_>,
    geometry: fastfile_t5::GfxWorldGeometry,
    catalog: MapXModelCatalog,
) -> PreparedMapModels {
    let (placements, static_error) = match build_t5_static_model_instances(stream, geometry) {
        Ok(placements) => (placements, None),
        Err(_) => (
            Vec::new(),
            Some(StaticModelDrawError::InstancesUnavailable {
                smodel_count: geometry.smodel_count,
            }),
        ),
    };
    let mut scripts = script_model_placements_t5(stream);
    let mut brushes = script_brush_model_placements_t5(stream);
    normalize_bomb_sites(
        &mut scripts,
        &mut brushes,
        &map_use_triggers_t5(stream),
        "bombzone_dem",
    );
    let descriptors = flag_descriptors_t5(stream);
    let structs = map_script_structs_t5(stream);
    link_model_placements(
        placements,
        static_error,
        &scripts,
        brushes,
        descriptors,
        structs,
        catalog,
        geometry.smodel_count,
    )
}

pub(crate) fn build_iw5_static_model_draw(
    stream: &fastfile_iw5::ZoneStream<'_>,
    geometry: fastfile_iw5::GfxWorldGeometry,
    catalog: MapXModelCatalog,
) -> PreparedMapModels {
    let (placements, static_error) = match build_iw5_static_model_instances(stream, geometry) {
        Ok(placements) => (placements, None),
        Err(_) => (
            Vec::new(),
            Some(StaticModelDrawError::InstancesUnavailable {
                smodel_count: geometry.smodel_count,
            }),
        ),
    };
    let mut scripts = script_model_placements_iw5(stream);
    let mut brushes = script_brush_model_placements_iw5(stream);
    normalize_bomb_sites(
        &mut scripts,
        &mut brushes,
        &map_use_triggers_iw5(stream),
        "dd_bombzone",
    );
    let descriptors = flag_descriptors_iw5(stream);
    let structs = map_script_structs_iw5(stream);
    link_model_placements(
        placements,
        static_error,
        &scripts,
        brushes,
        descriptors,
        structs,
        catalog,
        geometry.smodel_count,
    )
}

fn normalize_bomb_sites(
    scripts: &mut [asset_world::ScriptModelPlacement],
    brushes: &mut [asset_world::ScriptBrushModelPlacement],
    triggers: &[asset_world::MapUseTrigger],
    demolition_tag: &str,
) {
    let has_dedicated_sites = ["a", "b"].into_iter().all(|label| {
        triggers.iter().any(|trigger| {
            trigger.targetname == demolition_tag
                && trigger.script_label.trim_start_matches('_') == label
        })
    });
    let normalize = |value: &mut String| {
        *value = value
            .split_ascii_whitespace()
            .map(|token| {
                if token == demolition_tag {
                    "bombzone"
                } else if token == "bombzone" && has_dedicated_sites {
                    "sd_bombzone"
                } else {
                    token
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    };
    for script in scripts {
        normalize(&mut script.gameobject);
    }
    for brush in brushes {
        normalize(&mut brush.gameobject);
    }
}

pub(crate) fn link_model_placements(
    placements: Vec<StaticModelInstance>,
    mut static_error: Option<StaticModelDrawError>,
    scripts: &[ScriptModelPlacement],
    script_brush_models: Vec<asset_world::ScriptBrushModelPlacement>,
    flag_descriptors: Vec<asset_world::FlagDescriptor>,
    script_structs: Vec<asset_world::MapScriptStruct>,
    mut catalog: MapXModelCatalog,
    smodel_count: usize,
) -> PreparedMapModels {
    if static_error.is_none() && placements.len() != smodel_count {
        static_error = Some(StaticModelDrawError::PlacementCountMismatch {
            decoded: placements.len(),
            smodel_count,
        });
    }
    let mut remap = vec![None; catalog.meshes.len()];
    let mut used = Vec::new();
    let mut gaps = catalog.failed;
    let mut instances: Vec<Option<StaticModelPlacement>> = vec![None; smodel_count];
    let mut script_instances = Vec::with_capacity(scripts.len());

    let mut take_mesh = |source_mesh: usize| -> usize {
        match remap[source_mesh] {
            Some(mesh) => mesh,
            None => {
                let mesh = used.len();
                remap[source_mesh] = Some(mesh);
                used.push(source_mesh);
                mesh
            }
        }
    };

    if static_error.is_none() {
        for (slot, placement) in placements.into_iter().enumerate() {
            let Some(source_mesh) = catalog.mesh_index(placement.model_slot) else {
                gaps += 1;
                continue;
            };
            let mesh = take_mesh(source_mesh);
            let basis = Mat3::from_cols(
                Vec3::from_array(placement.axis[0]),
                Vec3::from_array(placement.axis[1]),
                Vec3::from_array(placement.axis[2]),
            );
            instances[slot] = Some(StaticModelPlacement {
                mesh,
                transform: Transform {
                    translation: Vec3::from_array(placement.origin),
                    rotation: Quat::from_mat3(&basis),
                    scale: Vec3::splat(placement.scale),
                },
                origin: placement.origin,
                axis: placement.axis,
                scale: placement.scale,
                cull_dist: placement.cull_dist,
                reflection_probe_index: placement.reflection_probe_index,
                primary_light_index: placement.primary_light_index,
                flags: placement.flags,
            });
        }
    } else {
        gaps = gaps.saturating_add(smodel_count);
    }

    let mut script_gaps = 0usize;
    for placement in scripts {
        let initial = catalog.t5_destructibles.get(&placement.destructible_def);
        let current_model = match initial {
            Some(Ok(initial)) => initial.model.clone(),
            _ if !placement.destructible_def.is_empty() => {
                MapXModelAssetKey(placement.destructible_def.clone())
            }
            _ => MapXModelAssetKey(placement.model.clone()),
        };
        if catalog.scene_assets.get(&current_model).is_none() {
            script_gaps += 1;
            catalog.scene_assets.insert(
                current_model.clone(),
                MapXModelSceneAsset::Unavailable {
                    reason: "map XModel was not captured as a scene asset",
                },
            );
        }
        let mut dobj_state =
            xmodel_runtime::DObjSemanticState::bind_pose(current_model.0.clone(), 1, 1);
        match initial {
            Some(Ok(initial)) => dobj_state.hide_part_bits = initial.hide_parts,
            Some(Err(reason)) => {
                catalog.scene_assets.insert(
                    current_model.clone(),
                    MapXModelSceneAsset::Unavailable { reason },
                );
            }
            None if !placement.destructible_def.is_empty() => {
                catalog.scene_assets.insert(
                    current_model.clone(),
                    MapXModelSceneAsset::Unavailable {
                        reason: "T5 MapEnts DestructibleDef was not captured",
                    },
                );
            }
            None => {}
        }
        let [pitch, yaw, roll] = placement.angles.map(f32::to_radians);
        script_instances.push(ScriptModelSceneInstance {
            id: placement.id,
            dobj_state,
            current_model,
            transform: Transform {
                translation: Vec3::from_array(placement.origin),
                rotation: Quat::from_euler(bevy::math::EulerRot::ZYX, yaw, pitch, roll),
                scale: Vec3::ONE,
            },
            lighting_origin: placement.lighting_origin.unwrap_or(placement.origin),
            metadata: ScriptModelMetadata {
                gameobject: placement.gameobject.clone(),
                targetname: placement.targetname.clone(),
                script_noteworthy: placement.script_noteworthy.clone(),
                destructible_type: placement.destructible_type.clone(),
                destructible_def: placement.destructible_def.clone(),
                t5_destructible: initial
                    .and_then(|row| row.as_ref().ok())
                    .map(|row| row.definition.clone()),
                target: placement.target.clone(),
                script_exploder: placement.script_exploder.clone(),
                brush_link: placement.brush_link.clone(),
                script_accumulate: placement.script_accumulate,
                script_threshold: placement.script_threshold,
                script_destructable_area: placement.script_destructable_area.clone(),
                script_fxid: placement.script_fxid.clone(),
            },
        });
    }

    let mut meshes = catalog.meshes.into_iter().map(Some).collect::<Vec<_>>();
    let meshes = used
        .into_iter()
        .filter_map(|index| meshes.get_mut(index).and_then(Option::take))
        .collect();
    PreparedMapModels {
        static_draw: StaticModelDraw {
            meshes,
            placements: instances,
            gaps,
        },
        static_error,
        scene_assets: catalog.scene_assets,
        script_instances,
        script_brush_models,
        flag_descriptors,
        script_structs,
        script_gaps,
    }
}

pub(super) fn report_map_models(
    report: &mut Vec<String>,
    models: &PreparedMapModels,
    authored_slots: impl std::fmt::Display,
) {
    if let Some(error) = models.static_error.as_ref() {
        report.push(format!("static models: {error}"));
    }
    let smodels = &models.static_draw;
    report.push(format!(
        "static models: {}/{authored_slots} authored slots resolved to {} unique meshes ({} unresolved)",
        smodels.resolved_count(),
        smodels.meshes.len(),
        smodels.gaps
    ));
    report.push(format!(
        "script_model: {} placements linked (MapEnts props; separate visibility owner)",
        models.script_instances.len()
    ));
    report.push(format!(
        "script_brushmodel: {} *N placements (SP_script_brushmodel, not DrawInst)",
        models.script_brush_models.len()
    ));
}

pub(super) fn report_ffa_spawns(report: &mut Vec<String>, spawns: &[asset_world::SpawnPoint]) {
    report.push(format!(
        "ffa spawns: {} mp_dm_spawn* ({} start)",
        spawns.len(),
        spawns.iter().filter(|p| p.is_initial()).count()
    ));
}

pub(super) fn report_intermission(
    report: &mut Vec<String>,
    view: Option<&asset_world::IntermissionView>,
) {
    report.push(match view {
        Some(view) => format!(
            "camera: mp_global_intermission origin={:?} angles={:?}",
            view.origin, view.angles
        ),
        None => "camera: mp_global_intermission not found".into(),
    });
}

pub(super) fn report_world_batches(report: &mut Vec<String>, draw: &asset_world::WorldDraw) {
    let lightmapped = draw.surface_lightmapped.iter().filter(|&&lit| lit).count();
    report.push(format!(
        "world batches: {lightmapped} lightmapped, {} fallback surfaces",
        draw.surface_lightmapped.len() - lightmapped
    ));
}

pub(super) fn report_dpvs(report: &mut Vec<String>, draw: &asset_world::WorldDraw) {
    report.push(format!(
        "dpvs: cells={} planes={} nodes={} sorted={} portal_verts={} cleared_boxes={}",
        draw.dpvs.cell_count,
        draw.dpvs.planes.len(),
        draw.dpvs.nodes.len(),
        draw.dpvs.sorted_surf_index.len(),
        draw.dpvs.portal_verts.len(),
        draw.dpvs.cleared_boxes
    ));
}

pub(super) fn decode_reflection_probes(
    report: &mut Vec<String>,
    draw: &asset_world::WorldDraw,
    materials: &MaterialCatalog,
) -> Vec<Option<bevy::prelude::Image>> {
    let images: Vec<_> = draw
        .reflection_probes
        .iter()
        .map(|probe| {
            let source = materials.images.get(probe.image?)?;
            if let Some(decoded) = &source.decoded {
                return Some(decoded.as_ref().clone());
            }
            asset_material::decode_reflection_probe_cubemap(source)
                .map_err(|error| {
                    report.push(format!("reflection probe {} gap: {error}", source.name))
                })
                .ok()
        })
        .collect();
    report.push(format!(
        "reflection probes: {}/{} cubemaps decoded",
        images.iter().flatten().count(),
        images.len()
    ));
    images
}

pub(super) fn smodel_lighting_samples(
    report: &mut Vec<String>,
    grid: &asset_model::OwnedLightGrid,
    origins: Vec<(usize, [f32; 3])>,
    placements: &[Option<StaticModelPlacement>],
    clip: Option<&asset_world::ClipCollision>,
) -> Vec<asset_model::SmodelLightingSample> {
    use asset_model::model_lighting::{
        build_smodel_lighting_samples_with_sight, census_lit_fragment_tiles,
    };
    use lighting_iw4::LIGHT_GRID_SIGHT_CONTENT_MASK as MASK;
    let origins: Vec<_> = origins
        .into_iter()
        .filter(|(slot, _)| placements.get(*slot).is_some_and(Option::is_some))
        .collect();
    let tiles = match clip {
        Some(clip_map) => {
            let clear = |start, end| clip_map.box_sight_clear(start, end, MASK);
            let (tiles, census) =
                build_smodel_lighting_samples_with_sight(&grid.view(), &origins, Some(&clear));
            report.push(format!(
                "smodel lighting: lit={} / candidates={} (blocked row={} trunc={} empty={}; CM sight mask=0x{MASK:x} corners need={} cleared={} suppressed={})",
                census.lit,
                census.candidates,
                census.blocked_unmodelled_row,
                census.blocked_truncated,
                census.blocked_no_live_corner,
                census.corners_needing_sight,
                census.corners_needing_sight.saturating_sub(census.corners_sight_suppressed),
                census.corners_sight_suppressed,
            ));
            tiles
        }
        None => {
            let (tiles, census) =
                build_smodel_lighting_samples_with_sight(&grid.view(), &origins, None);
            report.push(format!(
                "smodel lighting: lit={} / candidates={} (no clipmap — needsTrace corners suppressed; blocked row={} trunc={} empty={})",
                census.lit,
                census.candidates,
                census.blocked_unmodelled_row,
                census.blocked_truncated,
                census.blocked_no_live_corner,
            ));
            tiles
        }
    };
    if let Some(frag) = census_lit_fragment_tiles(&tiles) {
        report.push(format!(
            "smodel lit_fragment mid-grey: tiles={} lum min={:.4} max={:.4} mean={:.4} (specular=0)",
            frag.tiles, frag.lum_min, frag.lum_max, frag.lum_mean
        ));
    }
    tiles
}
