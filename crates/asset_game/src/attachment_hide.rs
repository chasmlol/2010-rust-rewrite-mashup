use anim_iw4::xmodel_no_scale_bit;

use crate::weapon_catalog::WeaponRegistry;

pub fn effective_hide_tags(registry: &WeaponRegistry, weapon_id: u32) -> Vec<String> {
    let mut tags = registry.hide_tags_of(weapon_id).to_vec();
    if registry.identity_namespace_of(weapon_id) == Some(crate::AssetNamespace::Iw5) {
        let scoped = registry
            .iw5_configuration_of(weapon_id)
            .is_some_and(|(_, selection)| selection.scope != 0);
        tags.push(
            if scoped {
                "tag_sight_on"
            } else {
                "tag_sight_off"
            }
            .to_owned(),
        );
    }
    tags
}

pub fn bone_has_hidden_ancestor(
    bone_names: &[String],
    parent_of: impl Fn(usize) -> Option<usize>,
    bone: usize,
    hidden_tags: &[String],
) -> bool {
    let mut current = Some(bone);
    while let Some(index) = current {
        if bone_names
            .get(index)
            .is_some_and(|name| hidden_tags.iter().any(|t| t.eq_ignore_ascii_case(name)))
        {
            return true;
        }
        current = parent_of(index);
    }
    false
}

pub fn surface_visible(
    part_bits: &[u32; 6],
    bone_names: &[String],
    parent_of: impl Fn(usize) -> Option<usize>,
    hidden_tags: &[String],
) -> bool {
    if hidden_tags.is_empty() {
        return true;
    }
    !(0..bone_names.len()).any(|bone| {
        xmodel_no_scale_bit(part_bits, bone)
            && bone_has_hidden_ancestor(bone_names, &parent_of, bone, hidden_tags)
    })
}
