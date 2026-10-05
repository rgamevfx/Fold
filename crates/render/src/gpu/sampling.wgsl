// Separable area reduction / linear enlargement in premultiplied working color.
struct Params { sizes: vec4<u32>, region: vec4<u32> }
// sizes = native width,height, target width,height
// region = target x,y, first native row, axis
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var output: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var<uniform> p: Params;
@group(0) @binding(3) var<storage, read_write> invalid: atomic<u32>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(output)) { return; }
    let axis = p.region.w;
    let input_size = p.sizes[axis]; let target_size = p.sizes[axis+2u];
    let index = id[axis] + p.region[axis];
    let scale = f32(input_size)/f32(target_size);
    var left = f32(index)*scale; var right = f32(index+1u)*scale;
    var start = i32(floor(left)); var end = i32(ceil(right))-1;
    if input_size < target_size {
        left = (f32(index)+0.5)*scale-0.5; right = left;
        start = i32(floor(left)); end = start+1;
    }
    var value = vec4<f32>(0.0);
    for (var tap=start; tap<=end; tap+=1) {
        var weight = max(0.0, min(f32(tap+1),right)-max(f32(tap),left))/scale;
        if input_size < target_size { weight=max(0.0, 1.0-abs(f32(tap)-left)); }
        let coord = clamp(tap,0,i32(input_size)-1);
        var q = vec2<i32>(coord, i32(id.y+p.region.z));
        if axis==1u { q = vec2<i32>(i32(id.x), coord-i32(p.region.z)); }
        value += textureLoad(source,q,0)*weight;
    }
    value.a=clamp(value.a,0.0,1.0);
    let exponent = bitcast<vec4<u32>>(value) & vec4<u32>(0x7f800000u);
    if any(exponent==vec4<u32>(0x7f800000u)) { atomicOr(&invalid,1u); }
    textureStore(output,vec2<i32>(id.xy),value);
}
