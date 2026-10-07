use fold_render::{ImageOp, RenderGraph, shader::Program};
use std::sync::Arc;
fn fixture(gain: f32) -> RenderGraph {
    RenderGraph {width:17,height:11,nodes:vec![ImageOp::Solid {rgba:[0.2,0.3,0.1,0.5]},ImageOp::Shader {inputs:vec![0],program:Arc::new(Program {source:"fn effect(uv:vec2<f32>)->vec4<f32>{let p=source(uv);return vec4<f32>(p.rgb*fold_params.values[0].x,p.a);}".into(),images:vec!["source".into()],values:vec![[gain,0.,0.,0.]],logical_size:[1920.,1080.],time:0.,progress:0.})}],output:1}
}
#[test]
fn cpu_execution_reports_gpu_requirement_and_regions_preserve_full_coordinates() {
    assert!(
        fold_render::render(fixture(1.))
            .unwrap_err()
            .to_string()
            .contains("GPU")
    );
    let plan = fold_render::region::Plan::new(
        fixture(1.),
        fold_render::region::Region {
            x: 2,
            y: 3,
            width: 4,
            height: 5,
        },
        false,
    )
    .unwrap();
    assert_eq!(plan.graph.width, 17);
    assert_eq!(plan.graph.height, 11);
}
#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires a native Vulkan adapter"]
fn native_shader_parameters_errors_and_recovery() {
    use fold_render::gpu::{Host, Renderer};
    let (host, info) = pollster::block_on(Host::headless(32 * 1024 * 1024)).unwrap();
    eprintln!("Shader adapter: {info:?}");
    let mut renderer = Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    for (i, gain) in [1., 0.5, 1.].into_iter().enumerate() {
        let mut gpu = renderer
            .evaluate(fixture(gain), None, &mut decoder, &cancel)
            .unwrap();
        assert_eq!(gpu.statistics.shader_compilations, u32::from(i == 0));
        let output = gpu.readback(&cancel).unwrap();
        for pixel in output.pixels() {
            for (a, b) in pixel.iter().zip([0.2 * gain, 0.3 * gain, 0.1 * gain, 0.5]) {
                assert!((*a - b).abs() < 1e-6);
            }
        }
    }
    let mut invalid = fixture(1.);
    if let ImageOp::Shader { program, .. } = &mut invalid.nodes[1] {
        Arc::make_mut(program).source = "fn effect(uv:vec2<f32>)->vec4<f32>{ broken }".into();
    }
    assert!(
        renderer
            .evaluate(invalid, None, &mut decoder, &cancel)
            .is_err()
    );
    renderer
        .evaluate(fixture(1.), None, &mut decoder, &cancel)
        .unwrap()
        .readback(&cancel)
        .unwrap();
}

#[test]
fn source_validation_rejects_loops_undeclared_resources_and_direct_output_writes() {
    let ImageOp::Shader { program, .. } = fixture(1.).nodes.pop().unwrap() else {
        panic!()
    };
    for source in [
        "fn effect(uv:vec2<f32>)->vec4<f32>{loop{} return source(uv);}",
        "@group(1) @binding(0) var extra:texture_2d<f32>; fn effect(uv:vec2<f32>)->vec4<f32>{return textureLoad(extra,vec2<i32>(0),0);}",
        "fn effect(uv:vec2<f32>)->vec4<f32>{textureStore(fold_output,vec2<i32>(0),vec4<f32>(1.));return source(uv);}",
    ] {
        let mut invalid = (*program).clone();
        invalid.source = source.into();
        assert!(invalid.validate_source().is_err(), "{source}");
    }
    program.validate_source().unwrap();
}
