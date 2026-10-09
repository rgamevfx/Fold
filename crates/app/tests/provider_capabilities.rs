use fold_foundation::DocumentId;
use fold_platform::packages::Capability;

#[test]
fn builtin_capabilities_match_callable_contracts() {
    let registry = fold_app::packages::builtins();
    for (kind, animation, hierarchy, settings, inputs, query) in [
        (fold_timeline::SEQUENCE, false, false, true, false, false),
        (
            fold_composition::COMPOSITION,
            false,
            false,
            true,
            false,
            false,
        ),
        (fold_compositor::COMPOSITE, true, false, true, true, true),
        (fold_motion::CONSTRUCTION, true, false, false, false, false),
        (fold_motion::scene::TYPE, true, true, false, false, false),
    ] {
        let document = registry.create(kind, DocumentId::new()).unwrap();
        assert!(
            registry.has_capability(&document, Capability::Authoring),
            "{kind}"
        );
        assert!(
            registry.has_capability(&document, Capability::Parameters),
            "{kind}"
        );
        registry.inspect(&document).unwrap();
        registry.parameters(&document).unwrap();
        for (capability, supported) in [
            (Capability::Animation, animation),
            (Capability::Hierarchy, hierarchy),
            (Capability::Settings, settings),
            (Capability::ImageInputs, inputs),
            (Capability::Query, query),
        ] {
            assert_eq!(
                registry.has_capability(&document, capability),
                supported,
                "{kind} / {capability:?}"
            );
        }
        assert_eq!(
            registry.animation_channels(&document, None).is_ok(),
            animation
        );
        assert_eq!(registry.hierarchy(&document).is_ok(), hierarchy);
        assert_eq!(registry.document_settings(&document).is_ok(), settings);
        assert_eq!(registry.document_image_inputs(&document).is_ok(), inputs);
        let mut unavailable = document.clone();
        unavailable.schema_version = u32::MAX;
        assert!(!registry.has_capability(&unavailable, Capability::Authoring));
    }
}
