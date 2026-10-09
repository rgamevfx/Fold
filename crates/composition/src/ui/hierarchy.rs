//! View identities qualify provider objects by Composition layer occurrence.
//! No scene hierarchy or graph is copied into the Composition document.
use fold_foundation::{DocumentId, ObjectId};
use fold_platform::hierarchy::Hierarchy;
use fold_ui::sdk::animation_editor::{LayerControls, Track};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Projection {
    ids: BTreeMap<(ObjectId, ObjectId), ObjectId>,
    order: Vec<ObjectId>,
    pub rows: BTreeMap<ObjectId, Row>,
}
pub(super) struct Row {
    pub layer: ObjectId,
    pub document: DocumentId,
    pub object: ObjectId,
    pub track: Track,
}
impl Projection {
    pub fn clear(&mut self) {
        self.rows.clear();
        self.order.clear();
    }
    pub fn insert(
        &mut self,
        layer: &crate::Layer,
        document: DocumentId,
        hierarchy: Hierarchy,
        locked: bool,
    ) {
        for row in &hierarchy.rows {
            self.ids
                .entry((layer.id, row.id))
                .or_insert_with(ObjectId::new);
        }
        for row in hierarchy.rows {
            let id = self.ids[&(layer.id, row.id)];
            self.order.push(id);
            let parent = row
                .parent
                .and_then(|parent| self.ids.get(&(layer.id, parent)).copied())
                .unwrap_or(layer.id);
            self.rows.insert(
                id,
                Row {
                    layer: layer.id,
                    document,
                    object: row.id,
                    track: Track {
                        id,
                        parent: Some(parent),
                        label: row.name,
                        locked: locked || row.locked,
                        range: None,
                        layer: Some(LayerControls {
                            accepts_children: true,
                            visible: row.visible,
                            icon: match row.kind.as_str() {
                                "Camera" => "C",
                                "Light" => "L",
                                _ => "O",
                            },
                        }),
                    },
                },
            );
        }
    }
    pub fn view(&self, layer: ObjectId, object: ObjectId) -> Option<ObjectId> {
        self.ids
            .get(&(layer, object))
            .copied()
            .filter(|id| self.rows.contains_key(id))
    }
    pub fn tracks(&self) -> impl Iterator<Item = Track> + '_ {
        self.order.iter().map(|id| self.rows[id].track.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_rows_are_views_of_the_same_objects_with_distinct_layer_occurrences() {
        let mut composition = crate::Composition::default();
        let document = DocumentId::new();
        let object = ObjectId::new();
        let layer = crate::Layer::new("Scene", None, composition.info.clone()).unwrap();
        let mut alias = layer.clone();
        alias.id = ObjectId::new();
        alias.name = "Scene copy".into();
        alias.origin = fold_foundation::Time::new(3, 1).unwrap();
        composition.layers = vec![layer.clone(), alias.clone()];
        let hierarchy = Hierarchy {
            rows: vec![fold_platform::hierarchy::Row {
                id: object,
                parent: None,
                name: "Box".into(),
                kind: "Object".into(),
                locked: false,
                visible: true,
            }],
            create: vec![],
        };
        let mut projection = Projection::default();
        projection.insert(&layer, document, hierarchy.clone(), false);
        projection.insert(&alias, document, hierarchy.clone(), false);
        let a = projection.view(layer.id, object).unwrap();
        let b = projection.view(alias.id, object).unwrap();
        assert_ne!(a, b);
        assert_eq!(projection.rows[&a].object, projection.rows[&b].object);
        let native = fold_platform::animation::Channel {
            target: fold_platform::animation::Target {
                group: Some(object),
                object,
                path: "scale".into(),
            },
            node_label: "Box".into(),
            property: "scale".into(),
            property_label: "Scale".into(),
            component: String::new(),
            curve: fold_animation::Curve {
                keys: vec![fold_animation::Key::new(fold_foundation::Time::ZERO, 1.)],
            },
        };
        let mut graph_channels =
            BTreeMap::from([(layer.id, vec![native.clone()]), (alias.id, vec![native])]);
        let ids = super::super::channels::KeyIds::default();
        let mut channels =
            super::super::channels::Channels::collect(&composition, &graph_channels, &ids).unwrap();
        channels.project_objects(&projection);
        assert_eq!(channels.rows[0].object, a);
        assert_eq!(channels.rows[1].object, b);
        assert_ne!(
            channels.rows[0].curve.keys[0].id,
            channels.rows[1].curve.keys[0].id
        );
        channels.rows[1].curve.keys[0].value = 2.;
        channels
            .apply(
                &mut composition,
                &mut graph_channels,
                fold_foundation::Time::ZERO,
                &ids,
            )
            .unwrap();
        assert_eq!(graph_channels[&alias.id][0].curve.keys[0].value, 2.);
        assert_eq!(
            graph_channels[&alias.id][0].curve.keys[0].time,
            fold_foundation::Time::ZERO
        );
        assert_eq!(graph_channels[&layer.id][0].curve.keys[0].value, 1.);
        projection.clear();
        projection.insert(&layer, document, hierarchy, true);
        assert_eq!(projection.view(layer.id, object), Some(a));
        assert!(projection.rows[&a].track.locked);
        assert!(projection.view(alias.id, object).is_none());
        assert_eq!(composition.layers.len(), 2);
    }
}
