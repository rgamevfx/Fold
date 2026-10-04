//! Provider-neutral edit proposals. Changes are atomic across selected channels.
use fold_animation::{Curve, Key};
use fold_foundation::{ObjectId, Time};
use std::collections::BTreeSet;

#[derive(Clone)]
pub struct Channel {
    pub object: ObjectId,
    pub node_label: String,
    pub property: String,
    pub property_label: String,
    pub component: String,
    pub path: String,
    pub curve: Curve,
}
#[derive(Clone, Default)]
pub struct Clipboard(pub Vec<(ObjectId, String, Vec<Key>)>);
impl Clipboard {
    pub fn copy(channels: &[Channel], selected: &BTreeSet<ObjectId>) -> Self {
        Self(
            channels
                .iter()
                .filter_map(|c| {
                    let keys: Vec<_> = c
                        .curve
                        .keys
                        .iter()
                        .filter(|k| selected.contains(&k.id))
                        .cloned()
                        .collect();
                    (!keys.is_empty()).then(|| (c.object, c.path.clone(), keys))
                })
                .collect(),
        )
    }
    pub fn paste(&self, channels: &mut [Channel], at: Time) -> Result<BTreeSet<ObjectId>, String> {
        let first = self
            .0
            .iter()
            .flat_map(|(_, _, keys)| keys.iter().map(|k| k.time))
            .min()
            .ok_or("No copied keys")?;
        let delta = at.checked_sub(first).map_err(|e| e.to_string())?;
        let mut result = channels.to_vec();
        let mut selected = BTreeSet::new();
        for (object, path, keys) in &self.0 {
            let target = result
                .iter_mut()
                .find(|c| c.object == *object && c.path == *path)
                .ok_or("Copied animation channel is no longer available")?;
            for key in keys {
                let mut key = key.clone();
                key.id = ObjectId::new();
                key.time = key.time.checked_add(delta).map_err(|e| e.to_string())?;
                if target.curve.at(key.time).is_some() {
                    return Err("Paste overlaps an existing keyframe".into());
                }
                selected.insert(key.id);
                target.curve.keys.push(key);
            }
            target.curve.keys.sort_by_key(|k| k.time);
            target.curve.validate()?;
        }
        channels.clone_from_slice(&result);
        Ok(selected)
    }
}
pub fn move_keys(
    original: &[Channel],
    selected: &BTreeSet<ObjectId>,
    delta: Time,
    value_delta: f64,
) -> Result<Vec<Channel>, String> {
    let mut result = original.to_vec();
    for channel in &mut result {
        for key in &mut channel.curve.keys {
            if selected.contains(&key.id) {
                key.time = key.time.checked_add(delta).map_err(|e| e.to_string())?;
                key.value += value_delta;
            }
        }
        channel.curve.keys.sort_by_key(|k| k.time);
        channel.curve.validate()?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn channels() -> Vec<Channel> {
        let mut curve = Curve::default();
        curve.insert(Time::new(1, 24).unwrap(), 2.);
        curve.insert(Time::new(3, 24).unwrap(), 6.);
        vec![Channel {
            object: ObjectId::new(),
            node_label: "Transform".into(),
            property: "position".into(),
            property_label: "Position".into(),
            component: "X".into(),
            path: "position.0".into(),
            curve,
        }]
    }
    #[test]
    fn multi_key_moves_preserve_spacing_and_reject_collisions_atomically() {
        let channels = channels();
        let selected = channels[0].curve.keys.iter().map(|k| k.id).collect();
        let shifted = move_keys(&channels, &selected, Time::new(1, 48).unwrap(), 3.).unwrap();
        assert_eq!(shifted[0].curve.keys[0].time, Time::new(1, 16).unwrap());
        assert_eq!(shifted[0].curve.keys[1].time, Time::new(7, 48).unwrap());
        assert_eq!(shifted[0].curve.keys[0].value, 5.);
        let selected = BTreeSet::from([channels[0].curve.keys[0].id]);
        assert!(move_keys(&channels, &selected, Time::new(2, 24).unwrap(), 0.).is_err());
        assert_eq!(channels[0].curve.keys[0].value, 2.);
    }
    #[test]
    fn clipboard_preserves_offsets_uses_new_ids_and_rejects_overlapping_paste() {
        let mut channels = channels();
        let selected = channels[0].curve.keys.iter().map(|k| k.id).collect();
        let clipboard = Clipboard::copy(&channels, &selected);
        let ids = clipboard
            .paste(&mut channels, Time::new(1, 1).unwrap())
            .unwrap();
        assert!(ids.is_disjoint(&selected));
        assert_eq!(channels[0].curve.keys.len(), 4);
        assert_eq!(channels[0].curve.keys[3].time, Time::new(13, 12).unwrap());
        let before = channels[0].curve.clone();
        assert!(
            clipboard
                .paste(&mut channels, Time::new(1, 1).unwrap())
                .is_err()
        );
        assert_eq!(channels[0].curve, before);
    }
}
