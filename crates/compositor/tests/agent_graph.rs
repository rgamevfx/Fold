use fold_compositor::{Composite, Node, Parameters, package};
use fold_foundation::ObjectId;
use fold_platform::packages::PackageRegistry;
use serde_json::json;
#[test]
fn agent_creation_insertion_query_and_receipt() {
    let mut r = PackageRegistry::default();
    package::register(&mut r).unwrap();
    let source = Node::new(
        Parameters::Solid {
            rgba: [1., 0., 0., 1.],
        },
        vec![],
    );
    let mut output = Node::disconnected(Parameters::Output);
    output.inputs[0] = Some(source.id);
    let sid = source.id;
    let oid = output.id;
    let c = Composite {
        nodes: vec![source, output],
        output: oid,
        info: fold_media::VideoInfo {
            width: 16,
            height: 16,
            rate: [24, 1],
            frames: 24,
        },
        extensions: Default::default(),
    };
    let d = c.document(Default::default()).unwrap();
    let blur = ObjectId::new();
    let (next,receipt)=r.author_cancellable(&d,&[
        json!({"op":"create_node","id":blur,"parameters":{"GaussianBlur":{"size":[2,2],"edges":"Clamp"}}}),
        json!({"op":"insert","node":oid,"input":"image","modifier":blur,"modifier_input":"image"})
    ],&Default::default()).unwrap();
    let c = Composite::from_document(&next).unwrap();
    assert_eq!(c.node(blur).unwrap().inputs[0], Some(sid));
    assert_eq!(c.node(oid).unwrap().inputs[0], Some(blur));
    assert_eq!(receipt["total_changed_nodes"], 2);
    let query = r
        .query(&next, &json!({"mode":"upstream","node":oid,"depth":2}))
        .unwrap();
    assert_eq!(query["total"], 3);
}
